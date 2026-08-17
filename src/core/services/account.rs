use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{
                Account, AccountCreateParams, AccountDeleteParams, AccountDescribeParams,
                AccountListParams, AccountUpdateParams,
            },
            list::{ListResponse, ListResponseMeta},
            membership::MembershipFilter,
            permission::PermissionRule,
            workspace::{Workspace, WorkspaceDescribeParams},
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
        contains::FilterByContains,
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            account::{
                AccountFilter, AccountForCreate, AccountForUpdate, AccountKind, AccountMeta,
            },
            id::DbId,
        },
        error::StoreError,
        manager::StoreManager,
        meta::{ContainsFilterStore, StoreId},
        stores::account::AccountStore,
        stores::workspace::SYSTEM_CONST,
        traits::{crud::*, dbx::DbExecutor},
        utils::ListOptionsValidator,
    },
};

pub struct AccountService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    cm: Arc<CacheManager<C>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for AccountService<D, C> {
    type CoreModel = Account;
    type ServiceStore = AccountStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.account
    }

    fn ws_svc(&self) -> &WorkspaceService<D, C> {
        self.ws_svc.as_ref()
    }

    fn validator(&self) -> &AuthValidator {
        self.validator.as_ref()
    }

    fn is_scoped(&self) -> bool {
        false
    }
}

impl<D: DbExecutor, C: CacheExecutor> AccountService<D, C> {
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

    async fn get_account_by_id_or_email(
        &self,
        ctx: &mut CoreCtx,
        id_or_email: &str,
    ) -> CoreResult<Account> {
        let store = self.store();
        // NOTE(workspace-scope): unscoped — the account table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);

        let acc = match Uuid::parse_str(&id_or_email) {
            Ok(id) => self.store().get(&store_ctx, &id.into()).await?.into(),
            Err(_) => self
                .get_by_email(
                    ctx,
                    &AccountDescribeParams {
                        id: None,
                        email: Some(id_or_email.to_string()),
                    },
                )
                .await?
                .ok_or(CoreError::NotFound(format!(
                    "Unable to get account id email: {id_or_email}"
                )))?,
        };

        Ok(acc)
    }

    /// Resolves an account from a typed id-or-email descriptor.
    ///
    /// When `params.id` is present the account is fetched by id (returning
    /// `Ok(None)` if it does not exist), otherwise `params.email` is used for
    /// an email lookup.
    pub async fn get_by_email(
        &self,
        ctx: &CoreCtx,
        params: &AccountDescribeParams,
    ) -> CoreResult<Option<Account>> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped — the account table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);

        let acc = match params.id {
            Some(id) => store.get_opt(&store_ctx, &id.into()).await?,
            None => {
                let Some(email) = params.email.as_deref() else {
                    return Ok(None);
                };
                store.get_by_email(&store_ctx, email).await?
            }
        };

        Ok(acc.map(|el| el.into()))
    }

    async fn invalidate_all_memberships(&self, ctx: &CoreCtx, acc_id: Uuid) -> CoreResult<()> {
        // NOTE(workspace-scope): unscoped — the account table has no workspace_id column.
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);
        let filter = json!({"account_id":&acc_id.to_string()}).try_into()?;
        // list all memberships with account in workspace all workspaces
        let memberships = self
            .sm
            .membership
            .list(&store_ctx, Some(filter), None)
            .await?;

        for mem in memberships {
            self.cm.auth.invalidate(mem.id.into()).await?;
        }

        Ok(())
    }

    /// Lists the `workspace_id`s of every membership the account holds, across
    /// all workspaces.
    ///
    /// NOTE(workspace-scope): the membership table is scoped, but the account
    /// is a global table — the subset rule (FR-009/FR-010) and the describe
    /// guard (FR-017) must see the account's full membership set, so this runs
    /// through an unscoped store context (mirrors `invalidate_all_memberships`).
    async fn account_workspace_ids(
        &self,
        ctx: &CoreCtx,
        account_id: Uuid,
    ) -> CoreResult<Vec<Uuid>> {
        let mut store_ctx: StoreCtx = ctx.into();
        store_ctx.set_workspace_scope(None);
        let filter = json!({"account_id": &account_id.to_string()}).try_into()?;
        let memberships = self
            .sm
            .membership
            .list(&store_ctx, Some(filter), None)
            .await?;

        Ok(memberships.into_iter().map(|m| m.workspace_id).collect())
    }

    /// Whether the caller is a system-namespace admin holding the global
    /// wildcard (`*:*`). Mirrors `AuthValidator`'s `is_system_namespace_admin`
    /// computation — such callers bypass the account-mutation subset rule and
    /// the cross-workspace describe guard.
    fn is_system_admin(&self, ctx: &CoreCtx) -> bool {
        ctx.auth_cache.auth_scope.workspace_slug == SYSTEM_CONST.system_ws_slug
            && ctx.permissions().has_global_wildcard()
    }

    /// FR-009/FR-010 subset rule: a non-system caller may mutate account `A`
    /// only if *every* workspace `A` is a member of is the caller's scoped
    /// workspace, and `A` is a member of at least one workspace (an account
    /// with no memberships is not "exclusively theirs").
    ///
    /// System admins bypass the rule entirely. Rejection returns a deliberately
    /// generic error that does not reveal whether the account has memberships
    /// elsewhere (or how many).
    async fn check_account_mutation_allowed(
        &self,
        ctx: &CoreCtx,
        account_id: Uuid,
    ) -> CoreResult<()> {
        if self.is_system_admin(ctx) {
            return Ok(());
        }

        let ws_ids = self.account_workspace_ids(ctx, account_id).await?;
        let exclusive = !ws_ids.is_empty() && ws_ids.iter().all(|id| *id == ctx.scoped_ws_id());

        if exclusive {
            Ok(())
        } else {
            Err(CoreError::Auth("not allowed".to_string()))
        }
    }

    /// FR-017 (T019): whether the account holds at least one membership in the
    /// caller's scoped workspace. Non-system callers may only `describe`
    /// accounts they can see in their own workspace.
    async fn account_is_member_of_scoped_workspace(
        &self,
        ctx: &CoreCtx,
        account_id: Uuid,
    ) -> CoreResult<bool> {
        let ws_ids = self.account_workspace_ids(ctx, account_id).await?;
        Ok(ws_ids.contains(&ctx.scoped_ws_id()))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for AccountService<D, C> {
    type CreateParams = AccountCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.create;

    async fn create(&self, ctx: &mut CoreCtx, params: AccountCreateParams) -> CoreResult<Account> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self
            .scope_and_validate(ctx, None, &[Self::CREATE_PERMISSION])
            .await?;
        if store
            .get_by_email(&store_ctx, &params.email)
            .await?
            .is_some()
        {
            return Err(CoreError::AlreadyExists(format!(
                "Account with email: {} already exists",
                params.email
            )));
        }

        let new_account = store.create(&store_ctx, params.into()).await?;

        Ok(new_account.into())
    }
}
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for AccountService<D, C> {
    type DescribeParams = AccountDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: AccountDescribeParams,
    ) -> CoreResult<Account> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self
            .scope_and_validate(ctx, None, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let identifier =
            params
                .id
                .map(|id| id.to_string())
                .or(params.email)
                .ok_or(CoreError::InvalidParams(
                    "Email or ID required to describe account".to_string(),
                ))?;

        let acc = self.get_account_by_id_or_email(ctx, &identifier).await?;

        // FR-017 (T019): prevent cross-workspace account enumeration — a
        // non-system caller may only describe an account that holds a
        // membership in their scoped workspace. The rejection is a generic
        // "not allowed" error: it does not distinguish "not found" from
        // "forbidden" (no membership probing is revealed).
        if !self.is_system_admin(ctx)
            && !self
                .account_is_member_of_scoped_workspace(ctx, acc.id)
                .await?
        {
            return Err(CoreError::Auth("not allowed".to_string()));
        }

        Ok(acc)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for AccountService<D, C> {
    type ListParams = AccountListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: AccountListParams,
    ) -> CoreResult<ListResponse<Account>> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(ctx.scoped_ws_id()), &[Self::LIST_PERMISSION])
            .await?;

        let options = params.list_options();

        let tags_filter = params.validate_filter_tags()?;

        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let data = store
            .list_in_namespace_by_join_table(
                &store_ctx,
                tags.clone(),
                filter.clone(),
                Some(options.clone()),
            )
            .await?;
        let total = store
            .count_with_tags_and_filter(&store_ctx, tags, filter)
            .await?;

        let accounts: Vec<Account> = data.into_iter().map(|el| el.into()).collect();
        Ok(ListResponse::new(accounts, total, options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for AccountService<D, C> {
    type UpdateParams = AccountUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.update;

    async fn update(&self, ctx: &mut CoreCtx, params: AccountUpdateParams) -> CoreResult<Account> {
        let store = self.store();
        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self
            .scope_and_validate(ctx, None, &[Self::UPDATE_PERMISSION])
            .await?;

        // NOTE: email is immutable here — `AccountForUpdate` deliberately omits
        // it (`into_store_params` never maps it). Email changes are reserved to
        // system admins (FR-012/FR-013), who operate on the account store
        // directly, so a non-system caller can never rewrite an email through
        // this path.
        // TODO: make email immutable
        let identifier = params.id_or_email()?;

        let current = self.get_account_by_id_or_email(ctx, &identifier).await?;
        let id = current.id;

        // FR-009/FR-010 subset rule: a non-system caller may only update an
        // account exclusively administered in their scoped workspace. This also
        // gates FR-011 (`verified`/`enabled`) since those flow through `update`.
        self.check_account_mutation_allowed(ctx, id).await?;

        let security_change = params.enabled.map_or(false, |e| e != current.enabled);

        let new_version = if security_change {
            Some(current.version + 1)
        } else {
            Some(current.version)
        };

        let mut update_data: AccountForUpdate = params.into_store_params(new_version);

        let updated_account = store.update(&store_ctx, &id.into(), update_data).await?;

        if security_change {
            self.invalidate_all_memberships(ctx, updated_account.id.into());
        } else {
            tracing::debug!("non-security update, cache not invalidated");
        }

        Ok(updated_account.into())
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for AccountService<D, C> {
    type DeleteParams = AccountDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.account.delete;

    async fn delete(&self, ctx: &mut CoreCtx, params: AccountDeleteParams) -> CoreResult<Account> {
        let store = self.store();

        // NOTE(workspace-scope): unscoped - global table (no workspace_id column).
        let store_ctx = self
            .scope_and_validate(ctx, None, &[Self::DELETE_PERMISSION])
            .await?;

        let identifier = params.id_or_email()?;
        let acc = self.get_account_by_id_or_email(ctx, &identifier).await?;

        // FR-009/FR-010 subset rule: a non-system caller may only delete an
        // account exclusively administered in their scoped workspace.
        self.check_account_mutation_allowed(ctx, acc.id).await?;

        let deleted = store.delete(&store_ctx, &acc.id.into()).await?.into();

        let account_id: Uuid = acc.id.into();
        self.invalidate_all_memberships(ctx, account_id);

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{
        cache::{
            entities::{
                auth::{AuthCache, AuthScopeCache},
                workspace::WorkspaceCache,
            },
            manager::CacheManager,
            mock::MockChx,
        },
        config::Config,
        core::services::registry::ServiceRegistry,
        store::{
            dbx::MockDbx,
            entities::{
                account::AccountRow,
                audit::AuditFields,
                membership::{MembershipMeta, MembershipRow, MembershipScope, MembershipStatus},
            },
        },
    };
    use serial_test::serial;

    fn account_row(account_id: Uuid) -> AccountRow {
        AccountRow {
            id: account_id.into(),
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

    /// Builds a workspace-scoped (non-system) caller context with the given
    /// permissions granted via escalation.
    fn scoped_ctx(ws_id: Uuid, perms: &[&str]) -> CoreResult<CoreCtx> {
        let ws_cache = WorkspaceCache {
            id: ws_id,
            slug: "acme".to_string(),
            ..WorkspaceCache::default()
        };
        let auth_cache = AuthCache::new_keyed(Uuid::new_v4(), Uuid::new_v4(), None);
        let mut ctx = CoreCtx::new(auth_cache, ws_cache)?;
        ctx.escalate_perms(perms)?;
        Ok(ctx)
    }

    fn update_params(account_id: Uuid, name: Option<String>) -> AccountUpdateParams {
        AccountUpdateParams {
            email: None,
            id: Some(account_id),
            name,
            description: None,
            avatar_url: None,
            enabled: None,
            verified: None,
            tags: None,
            meta: None,
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_account_list() -> CoreResult<()> {
        let config = Config::test_config();

        // build store manager backed by an in-memory fake database
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<AccountRow>(vec![AccountRow {
                    email: "list@example.com".into(),
                    ..Default::default()
                }])
                .with_one::<(i64,)>((1,)),
        );
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with an in-memory cache (no real Redis connection)
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:list"])?;

        let accounts = svc
            .list(
                &mut ctx,
                AccountListParams {
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(accounts.data.len(), 1);
        assert_eq!(accounts.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_mock_account_success() -> CoreResult<()> {
        let config = Config::test_config();

        // build store manager backed by an in-memory fake database
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<AccountRow>(vec![]) // duplicate-email check -> none found
                .with_one::<AccountRow>(AccountRow::default()), // INSERT ... RETURNING
        );
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with an in-memory cache (no real Redis connection)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:create"])?;

        let params = AccountCreateParams::default();

        let new_acc = svc.create(&mut ctx, params).await?;

        let expected = Account::default();

        assert_eq!(
            new_acc.id, expected.id,
            "incorrect account id returned from AccountService.create()"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_mock_account_error() -> CoreResult<()> {
        let config = Config::test_config();

        // A matching account already exists -> the duplicate-email check finds it
        let existing = AccountRow {
            email: "user@user.com".to_string(),
            ..Default::default()
        };

        let dbx = Arc::new(MockDbx::new().with_all::<AccountRow>(vec![existing]));
        let sm = Arc::new(StoreManager::new(dbx));

        // build cache manager with an in-memory cache (no real Redis connection)
        let mock_cache = Arc::new(MockChx::default());
        let cm = Arc::new(CacheManager::new(mock_cache));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:create"])?;

        let params = AccountCreateParams::default();
        let new_acc = svc.create(&mut ctx, params).await;

        assert!(
            matches!(new_acc, Err(CoreError::AlreadyExists(..))),
            "should be CoreError::AlreadyExists"
        );

        Ok(())
    }

    // --- FR-009/FR-010 subset rule (T013-T016) ---

    #[tokio::test]
    #[serial]
    async fn test_update_allowed_when_account_exclusive_to_scoped_workspace() -> CoreResult<()> {
        let config = Config::test_config();
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // check_account_mutation_allowed -> account_workspace_ids -> membership.list
                .with_all::<MembershipRow>(vec![membership_row(Uuid::new_v4(), account_id, ws_id)])
                // store.update
                .with_optional::<AccountRow>(Some(AccountRow {
                    id: account_id.into(),
                    name: "updated".to_string(),
                    ..Default::default()
                })),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = scoped_ctx(ws_id, &["account:update"])?;

        let updated = svc
            .update(
                &mut ctx,
                update_params(account_id, Some("updated".to_string())),
            )
            .await?;

        assert_eq!(updated.id, account_id);
        assert_eq!(updated.name, "updated");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_forbidden_when_account_shared_with_other_workspace() -> CoreResult<()> {
        let config = Config::test_config();
        let ws_id = Uuid::new_v4();
        let other_ws = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // account is a member of the caller's ws AND another ws
                .with_all::<MembershipRow>(vec![
                    membership_row(Uuid::new_v4(), account_id, ws_id),
                    membership_row(Uuid::new_v4(), account_id, other_ws),
                ]),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = scoped_ctx(ws_id, &["account:update"])?;

        let err = svc
            .update(
                &mut ctx,
                update_params(account_id, Some("renamed".to_string())),
            )
            .await;

        assert!(
            matches!(err, Err(CoreError::Auth(_))),
            "mutating an account shared with another workspace must be rejected with a generic Auth/forbidden error"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_forbidden_when_account_has_no_memberships() -> CoreResult<()> {
        let config = Config::test_config();
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // account_workspace_ids -> no memberships at all
                .with_all::<MembershipRow>(vec![]),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = scoped_ctx(ws_id, &["account:update"])?;

        let err = svc
            .update(
                &mut ctx,
                update_params(account_id, Some("renamed".to_string())),
            )
            .await;

        assert!(
            matches!(err, Err(CoreError::Auth(_))),
            "a non-system caller must not mutate an account with no memberships"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_allowed_for_system_admin() -> CoreResult<()> {
        let config = Config::test_config();
        let account_id = Uuid::new_v4();

        // System admin bypasses the membership check entirely — no MembershipRow mocks.
        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // store.update
                .with_optional::<AccountRow>(Some(AccountRow {
                    id: account_id.into(),
                    name: "admin-renamed".to_string(),
                    ..Default::default()
                })),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["account:update"])?;

        let updated = svc
            .update(
                &mut ctx,
                update_params(account_id, Some("admin-renamed".to_string())),
            )
            .await?;

        assert_eq!(updated.id, account_id);
        assert_eq!(updated.name, "admin-renamed");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_forbidden_when_account_shared_with_other_workspace() -> CoreResult<()> {
        let config = Config::test_config();
        let ws_id = Uuid::new_v4();
        let other_ws = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // account is a member of the caller's ws AND another ws
                .with_all::<MembershipRow>(vec![
                    membership_row(Uuid::new_v4(), account_id, ws_id),
                    membership_row(Uuid::new_v4(), account_id, other_ws),
                ]),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = scoped_ctx(ws_id, &["account:delete"])?;

        let err = svc
            .delete(
                &mut ctx,
                AccountDeleteParams {
                    email: None,
                    id: Some(account_id),
                },
            )
            .await;

        assert!(
            matches!(err, Err(CoreError::Auth(_))),
            "deleting an account shared with another workspace must be rejected with a generic Auth/forbidden error"
        );

        Ok(())
    }

    // --- FR-017 cross-workspace describe guard (T019) ---

    #[tokio::test]
    #[serial]
    async fn test_describe_forbidden_for_account_outside_scoped_workspace() -> CoreResult<()> {
        let config = Config::test_config();
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // account has no membership in the caller's scoped workspace
                .with_all::<MembershipRow>(vec![membership_row(
                    Uuid::new_v4(),
                    account_id,
                    Uuid::new_v4(),
                )]),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = scoped_ctx(ws_id, &["account:describe"])?;

        let err = svc
            .describe(
                &mut ctx,
                AccountDescribeParams {
                    email: None,
                    id: Some(account_id),
                },
            )
            .await;

        assert!(
            matches!(err, Err(CoreError::Auth(_))),
            "describing an account outside the caller's scoped workspace must fail with the generic forbidden error"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_describe_allowed_for_account_in_scoped_workspace() -> CoreResult<()> {
        let config = Config::test_config();
        let ws_id = Uuid::new_v4();
        let account_id = Uuid::new_v4();

        let dbx = Arc::new(
            MockDbx::new()
                // get_account_by_id_or_email -> account.get
                .with_optional::<AccountRow>(Some(account_row(account_id)))
                // account holds a membership in the caller's scoped workspace
                .with_all::<MembershipRow>(vec![membership_row(Uuid::new_v4(), account_id, ws_id)]),
        );
        let sm = Arc::new(StoreManager::new(dbx));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        let svc = svc_reg.account.clone();

        let mut ctx = scoped_ctx(ws_id, &["account:describe"])?;

        let acc = svc
            .describe(
                &mut ctx,
                AccountDescribeParams {
                    email: None,
                    id: Some(account_id),
                },
            )
            .await?;

        assert_eq!(acc.id, account_id);

        Ok(())
    }
}
