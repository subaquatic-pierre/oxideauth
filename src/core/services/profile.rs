use std::sync::Arc;
use uuid::Uuid;

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
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // Resolve the concrete workspace for the workspace-scoped guard checks.
        let workspace_id = params.workspace_id.unwrap_or_else(|| ctx.scoped_ws_id());

        // Guard: one profile per account per workspace
        if store
            .find_by_account_workspace(&store_ctx, params.account_id, workspace_id)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(format!(
                "profile already exists for account '{}' in workspace '{}'",
                params.account_id, workspace_id
            )));
        }

        // Guard: profile email must be unique within the workspace
        if store
            .find_by_email_workspace(&store_ctx, workspace_id, &params.email)
            .await?
            .is_some()
        {
            return Err(CoreError::EmailConflict(format!(
                "profile email '{}' already in use in workspace '{}'",
                params.email, workspace_id
            )));
        }

        let conflict_msg = format!(
            "profile email '{}' already in use in workspace '{}'",
            params.email, workspace_id
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        // Prefer id; fall back to email. At least one is required (validated).
        // Resolve the concrete workspace for the email lookup fallback.
        let workspace_id = params.workspace_id.unwrap_or_else(|| ctx.scoped_ws_id());
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
                    .find_by_email_workspace(&store_ctx, workspace_id, &email)
                    .await?
                    .ok_or_else(|| {
                        CoreError::NotFound(format!(
                            "profile with email '{}' not found in workspace '{}'",
                            email, workspace_id
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // Resolve the concrete workspace for the email-uniqueness guard.
        let workspace_id = params.workspace_id.unwrap_or_else(|| ctx.scoped_ws_id());

        // Guard: if the email is being changed, ensure it is not already used by
        // a *different* profile in this workspace (self-excluded).
        if let Some(email) = &params.email {
            if let Some(existing) = store
                .find_by_email_workspace(&store_ctx, workspace_id, email)
                .await?
            {
                if existing.id != params.id.into() {
                    return Err(CoreError::EmailConflict(format!(
                        "profile email '{}' already in use in workspace '{}'",
                        email, workspace_id
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
            workspace_id
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        // PostgreSQL's restrictive FK is authoritative; do not preflight with
        // a membership query because that introduces a check-then-delete race.
        let profile_row = match store.delete(&store_ctx, &params.id.into()).await {
            Ok(row) => row,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::InvalidParams(format!(
                    "cannot delete profile '{}' with attached memberships",
                    params.id
                )));
            }
            Err(err) => return Err(err.into()),
        };

        Ok(profile_row.into())
    }
}
