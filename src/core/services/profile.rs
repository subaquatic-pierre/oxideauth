use std::sync::Arc;
use uuid::Uuid;

use serde_json::json;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            list::ListResponse,
            profile::{
                Profile, ProfileCreateParams, ProfileDeleteParams, ProfileDescribeParams,
                ProfileListParams, ProfileUpdateParams,
            },
        },
        services::{
            permission::CANONICAL_PERMISSIONS, validator::AuthValidator,
            workspace::WorkspaceService,
        },
        traits::{
            list::RequestListParams,
            service::{
                CoreModelCreateService, CoreModelDeleteService, CoreModelDescribeService,
                CoreModelListService, CoreModelService, CoreModelUpdateService,
            },
        },
    },
    store::{
        ctx::StoreCtx,
        entities::{
            id::DbId,
            membership::MembershipFilter,
            profile::{ProfileForCreate, ProfileForUpdate, ProfileRow},
        },
        error::StoreError,
        manager::StoreManager,
        stores::profile::ProfileStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
    },
};

pub struct ProfileService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    // NOTE: cache manager is held for future phases (email-resolve membership
    // create, account-mutation subset rule) that will need auth-cache
    // invalidation when a profile changes.
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for ProfileService<D, C> {
    type CoreModel = Profile;

    type ServiceStore = ProfileStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.profile
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        true
    }
}

impl<D: DbExecutor, C: CacheExecutor> ProfileService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        cm: Arc<CacheManager<C>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            cm,
            ws_svc,
            validator,
        }
    }

    /// Finds the single profile for an account within a workspace (if any).
    ///
    /// This is the service-level helper used by the membership email-resolve
    /// flow to look up an existing profile before creating a new one.
    pub async fn find_by_account_workspace(
        &self,
        ctx: &mut CoreCtx,
        account_id: Uuid,
        workspace_id: Uuid,
    ) -> CoreResult<Option<Profile>> {
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(workspace_id), &[CANONICAL_PERMISSIONS.profile.describe])
            .await?;

        let row = self
            .store()
            .find_by_account_workspace(&store_ctx, account_id, workspace_id)
            .await?;

        Ok(row.map(Into::into))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for ProfileService<D, C> {
    type CreateParams = ProfileCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.profile.create;

    /// Creates a new Profile, scoped to the provided workspace ID.
    async fn create(
        &self,
        ctx: &mut CoreCtx,
        mut params: ProfileCreateParams,
    ) -> CoreResult<Profile> {
        // Validate & normalize the workspace-facing contact email
        params.email = crate::core::email::validate_email(&params.email)?;

        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::CREATE_PERMISSION])
            .await?;

        // Guard: one profile per account per workspace
        if store
            .find_by_account_workspace(&store_ctx, params.account_id, params.workspace_id)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(format!(
                "profile already exists for account '{}' in workspace '{}'",
                params.account_id, params.workspace_id
            )));
        }

        // Guard: profile email must be unique within the workspace
        if store
            .find_by_email_workspace(&store_ctx, params.workspace_id, &params.email)
            .await?
            .is_some()
        {
            return Err(CoreError::EmailConflict(format!(
                "profile email '{}' already in use in workspace '{}'",
                params.email, params.workspace_id
            )));
        }

        let conflict_msg = format!(
            "profile email '{}' already in use in workspace '{}'",
            params.email, params.workspace_id
        );

        let n_profile: ProfileForCreate = params.into();

        // The unique index is the authoritative backstop for concurrent inserts;
        // map a 23505 unique_violation to the same friendly email-conflict error.
        let profile_row = match store.create(&store_ctx, n_profile).await {
            Ok(row) => row,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::EmailConflict(conflict_msg));
            }
            Err(err) => return Err(err.into()),
        };

        Ok(profile_row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for ProfileService<D, C> {
    type DescribeParams = ProfileDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.profile.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: ProfileDescribeParams,
    ) -> CoreResult<Profile> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DESCRIBE_PERMISSION])
            .await?;

        // Prefer id; fall back to email. At least one is required (validated).
        let row = match params.id {
            Some(id) => store.get(&store_ctx, &id.into()).await?,
            None => {
                let email = params
                    .email
                    .as_deref()
                    .map(crate::core::email::validate_email)
                    .transpose()?
                    .ok_or_else(|| {
                        CoreError::InvalidParams("ID or email required".to_string())
                    })?;

                store
                    .find_by_email_workspace(&store_ctx, params.workspace_id, &email)
                    .await?
                    .ok_or_else(|| {
                        CoreError::NotFound(format!(
                            "profile with email '{}' not found in workspace '{}'",
                            email, params.workspace_id
                        ))
                    })?
            }
        };

        Ok(row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for ProfileService<D, C> {
    type ListParams = ProfileListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.profile.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: ProfileListParams,
    ) -> CoreResult<ListResponse<Profile>> {
        let store = self.store();

        // validate params
        let list_options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::LIST_PERMISSION])
            .await?;

        // Combined query: tags (@> containment) + field filter
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_with_tags_and_filter(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(list_options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let profiles: Vec<Profile> = data.into_iter().map(|el| el.into()).collect();

        Ok(ListResponse::new(profiles, total, list_options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for ProfileService<D, C> {
    type UpdateParams = ProfileUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.profile.update;

    async fn update(&self, ctx: &mut CoreCtx, mut params: ProfileUpdateParams) -> CoreResult<Profile> {
        // Validate & normalize the contact email when present
        if let Some(email) = &params.email {
            params.email = Some(crate::core::email::validate_email(email)?);
        }

        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::UPDATE_PERMISSION])
            .await?;

        // Guard: if the email is being changed, ensure it is not already used by
        // a *different* profile in this workspace (self-excluded).
        if let Some(email) = &params.email {
            if let Some(existing) = store
                .find_by_email_workspace(&store_ctx, params.workspace_id, email)
                .await?
            {
                if existing.id != params.id.into() {
                    return Err(CoreError::EmailConflict(format!(
                        "profile email '{}' already in use in workspace '{}'",
                        email, params.workspace_id
                    )));
                }
            }
        }

        // Bump version based on the current profile state
        let cur_row = store.get(&store_ctx, &params.id.into()).await?;
        let new_version = cur_row.version + 1;
        let id = params.id;
        let conflict_msg = format!(
            "profile email '{}' already in use in workspace '{}'",
            params.email.as_deref().unwrap_or_default(),
            params.workspace_id
        );
        let update_data: ProfileForUpdate = params.into_store_params(new_version);

        // Map a 23505 unique_violation to the friendly email-conflict error.
        let profile_row = match store.update(&store_ctx, &id.into(), update_data).await {
            Ok(row) => row,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::EmailConflict(conflict_msg));
            }
            Err(err) => return Err(err.into()),
        };

        Ok(profile_row.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for ProfileService<D, C> {
    type DeleteParams = ProfileDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.profile.delete;

    async fn delete(&self, ctx: &mut CoreCtx, params: ProfileDeleteParams) -> CoreResult<Profile> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DELETE_PERMISSION])
            .await?;

        // Guard: a profile is the workspace identity anchor for its memberships.
        // Refuse deletion while any membership still references this profile.
        let membership_filter: MembershipFilter = json!({
            "profile_id": params.id.to_string()
        })
        .try_into()?;

        let attached = self
            .sm
            .membership
            .list(&store_ctx, Some(membership_filter), None)
            .await?;

        if !attached.is_empty() {
            return Err(CoreError::InvalidParams(format!(
                "cannot delete profile '{}' with attached memberships",
                params.id
            )));
        }

        let profile_row = store.delete(&store_ctx, &params.id.into()).await?;

        Ok(profile_row.into())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{manager::CacheManager, mock::MockChx},
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{
            dbx::MockDbx,
            entities::{
                audit::AuditFields,
                id::DbId,
                membership::{MembershipMeta, MembershipRow, MembershipScope, MembershipStatus},
                profile::ProfileMeta,
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;

    /// Builds a `ProfileRow` for the in-memory mock.
    fn profile_row(id: Uuid, account_id: Uuid, ws_id: Uuid) -> ProfileRow {
        ProfileRow {
            id: id.into(),
            account_id,
            workspace_id: ws_id,
            email: "alice@example.com".to_string(),
            name: "test-profile".to_string(),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            version: 1,
            tags: vec![],
            meta: ProfileMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields::default(),
        }
    }

    fn ws_row(ws_id: Uuid) -> WorkspaceRow {
        WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        }
    }

    fn membership_row(mem_id: Uuid, account_id: Uuid, ws_id: Uuid) -> MembershipRow {
        MembershipRow {
            id: mem_id.into(),
            account_id,
            workspace_id: ws_id,
            profile_id: None,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            version: 1,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields::default(),
        }
    }

    /// Builds a `ProfileService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<ProfileService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.profile.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> account/workspace duplicate guard -> list (no existing profile)
            .with_all::<ProfileRow>(vec![])
            // create -> email uniqueness guard -> list (no existing email)
            .with_all::<ProfileRow>(vec![])
            // create -> store.create
            .with_one::<ProfileRow>(profile_row(profile_id, account_id, ws_id));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:create"])?;

        let params = ProfileCreateParams {
            account_id,
            workspace_id: ws_id,
            email: "alice@example.com".to_string(),
            name: "test-profile".to_string(),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: vec![],
            meta: ProfileMeta::default(),
        };

        let profile = svc.create(&mut ctx, params).await?;

        assert_eq!(profile.id, profile_id);
        assert_eq!(profile.workspace_id, ws_id);
        assert_eq!(profile.account_id, account_id);
        assert_eq!(profile.email, "alice@example.com");
        assert_eq!(profile.name, "test-profile");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_create_duplicate() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // create -> duplicate guard -> list finds an existing profile
            .with_all::<ProfileRow>(vec![profile_row(
                Uuid::new_v4(),
                account_id,
                ws_id,
            )]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:create"])?;

        let params = ProfileCreateParams {
            account_id,
            workspace_id: ws_id,
            email: "alice@example.com".to_string(),
            name: "dup".to_string(),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: vec![],
            meta: ProfileMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;

        assert!(
            matches!(res, Err(CoreError::AlreadyExists(_))),
            "duplicate profile must produce AlreadyExists"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_create_email_conflict() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let other_account_id = Uuid::new_v4();

        // Account/workspace guard finds nothing, but the email guard finds an
        // existing profile (a different account) already using the email.
        let dbx = MockDbx::new()
            .with_all::<ProfileRow>(vec![])
            .with_all::<ProfileRow>(vec![profile_row(
                Uuid::new_v4(),
                other_account_id,
                ws_id,
            )]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:create"])?;

        let params = ProfileCreateParams {
            account_id,
            workspace_id: ws_id,
            email: "alice@example.com".to_string(),
            name: "conflict".to_string(),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: vec![],
            meta: ProfileMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;

        assert!(
            matches!(res, Err(CoreError::EmailConflict(_))),
            "email conflict must produce EmailConflict"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_create_email_case_insensitive_conflict() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let other_account_id = Uuid::new_v4();

        // Stored email is lowercased; the guard normalizes the incoming email.
        let dbx = MockDbx::new()
            .with_all::<ProfileRow>(vec![])
            .with_all::<ProfileRow>(vec![profile_row(
                Uuid::new_v4(),
                other_account_id,
                ws_id,
            )]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:create"])?;

        let params = ProfileCreateParams {
            account_id,
            workspace_id: ws_id,
            email: "ALICE@example.com".to_string(),
            name: "conflict".to_string(),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: vec![],
            meta: ProfileMeta::default(),
        };

        let res = svc.create(&mut ctx, params).await;

        assert!(
            matches!(res, Err(CoreError::EmailConflict(_))),
            "case-insensitive email conflict must produce EmailConflict"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // store.get
            .with_optional::<ProfileRow>(Some(profile_row(profile_id, account_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:describe"])?;

        let profile = svc
            .describe(
                &mut ctx,
                ProfileDescribeParams {
                    id: Some(profile_id),
                    email: None,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(profile.id, profile_id);
        assert_eq!(profile.workspace_id, ws_id);
        assert_eq!(profile.account_id, account_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_describe_by_email() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // describe-by-email -> find_by_email_workspace -> list
            .with_all::<ProfileRow>(vec![profile_row(profile_id, account_id, ws_id)]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:describe"])?;

        let profile = svc
            .describe(
                &mut ctx,
                ProfileDescribeParams {
                    id: None,
                    email: Some("alice@example.com".to_string()),
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(profile.id, profile_id);
        assert_eq!(profile.email, "alice@example.com");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_describe_by_email_not_found() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // describe-by-email -> find_by_email_workspace -> list (no match)
            .with_all::<ProfileRow>(vec![]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:describe"])?;

        let res = svc
            .describe(
                &mut ctx,
                ProfileDescribeParams {
                    id: None,
                    email: Some("missing@example.com".to_string()),
                    workspace_id: ws_id,
                },
            )
            .await;

        assert!(
            matches!(res, Err(CoreError::NotFound(_))),
            "missing email must produce NotFound"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_describe_id_precedence_over_email() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // describe with id present -> store.get (email is ignored)
            .with_optional::<ProfileRow>(Some(profile_row(profile_id, account_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:describe"])?;

        let profile = svc
            .describe(
                &mut ctx,
                ProfileDescribeParams {
                    id: Some(profile_id),
                    email: Some("ignored@example.com".to_string()),
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(profile.id, profile_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_describe_neither_id_nor_email() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();

        let dbx = MockDbx::new();
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:describe"])?;

        let res = svc
            .describe(
                &mut ctx,
                ProfileDescribeParams {
                    id: None,
                    email: None,
                    workspace_id: ws_id,
                },
            )
            .await;

        assert!(
            matches!(res, Err(CoreError::InvalidParams(_))),
            "neither id nor email must produce InvalidParams"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // list_with_tags_and_filter
            .with_all::<ProfileRow>(vec![profile_row(profile_id, account_id, ws_id)])
            // count_with_tags_and_filter
            .with_one::<(i64,)>((1,));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:list"])?;

        let res = svc
            .list(
                &mut ctx,
                ProfileListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, profile_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // update -> email uniqueness guard -> list (returns self; no conflict)
            .with_all::<ProfileRow>(vec![profile_row(profile_id, account_id, ws_id)])
            // update -> store.get (current version)
            .with_optional::<ProfileRow>(Some(profile_row(profile_id, account_id, ws_id)))
            // update -> store.update
            .with_optional::<ProfileRow>(Some(ProfileRow {
                name: "renamed".to_string(),
                version: 2,
                ..profile_row(profile_id, account_id, ws_id)
            }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:update"])?;

        let params = ProfileUpdateParams {
            id: profile_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            email: Some("alice@example.com".to_string()),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;

        assert_eq!(updated.id, profile_id);
        assert_eq!(updated.workspace_id, ws_id);
        // US4: updating the profile email never alters the account linkage/identity
        assert_eq!(updated.account_id, account_id);
        assert_eq!(updated.email, "alice@example.com");
        assert_eq!(updated.name, "renamed");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_update_email_conflict() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        // The email guard finds a DIFFERENT profile already using the email.
        let dbx = MockDbx::new()
            .with_all::<ProfileRow>(vec![profile_row(
                Uuid::new_v4(),
                Uuid::new_v4(),
                ws_id,
            )]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:update"])?;

        let params = ProfileUpdateParams {
            id: profile_id,
            workspace_id: ws_id,
            name: None,
            email: Some("alice@example.com".to_string()),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: None,
            meta: None,
        };

        let res = svc.update(&mut ctx, params).await;

        assert!(
            matches!(res, Err(CoreError::EmailConflict(_))),
            "updating to a duplicate email must produce EmailConflict"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_update_email_omitted_no_conflict() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        // email omitted -> the email guard is skipped entirely.
        let dbx = MockDbx::new()
            .with_optional::<ProfileRow>(Some(profile_row(profile_id, account_id, ws_id)))
            .with_optional::<ProfileRow>(Some(ProfileRow {
                name: "renamed".to_string(),
                version: 2,
                ..profile_row(profile_id, account_id, ws_id)
            }));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:update"])?;

        let params = ProfileUpdateParams {
            id: profile_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            email: None,
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;
        assert_eq!(updated.name, "renamed");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> guard -> membership list (no attached memberships)
            .with_all::<MembershipRow>(vec![])
            // delete -> store.delete
            .with_optional::<ProfileRow>(Some(profile_row(profile_id, account_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:delete"])?;

        let deleted = svc
            .delete(
                &mut ctx,
                ProfileDeleteParams {
                    id: profile_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(deleted.id, profile_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_profile_delete_with_attached_membership_rejected() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();
        let profile_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(ws_row(ws_id)))
            // delete -> guard -> membership list finds an attached membership
            .with_all::<MembershipRow>(vec![MembershipRow {
                profile_id: Some(profile_id),
                ..membership_row(Uuid::new_v4(), account_id, ws_id)
            }]);
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["profile:delete"])?;

        let err = svc
            .delete(
                &mut ctx,
                ProfileDeleteParams {
                    id: profile_id,
                    workspace_id: ws_id,
                },
            )
            .await;

        assert!(
            matches!(err, Err(CoreError::InvalidParams(_))),
            "deleting a profile with attached memberships must be rejected"
        );

        Ok(())
    }
}
