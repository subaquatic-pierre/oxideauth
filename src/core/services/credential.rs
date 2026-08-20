use uuid::Uuid;

use crate::{
    cache::{
        entities::{auth::AuthCache, client_auth::ClientAuthCache},
        manager::CacheManager,
        traits::CacheExecutor,
    },
    core::models::{
        permission::PermissionRule,
        workspace::{Workspace, WorkspaceDescribeParams},
    },
    store::{
        ctx::StoreCtx,
        entities::credential::{
            CredentialForCreate, CredentialForUpdate, CredentialKind, CredentialRow,
            CredentialStatus,
        },
        error::StoreError,
        manager::StoreManager,
        stores::credential::CredentialStore,
        traits::{crud::*, dbx::DbExecutor},
    },
};
use crate::{
    core::email::normalize_email,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::{Account, AccountDescribeParams},
            credential::{
                Credential, CredentialCreateParams, CredentialDeleteParams,
                CredentialDescribeParams, CredentialListParams, CredentialUpdateParams,
            },
            list::ListResponse,
        },
        services::{
            account::AccountService, permission::CANONICAL_PERMISSIONS, validator::AuthValidator,
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
    store::contains::FilterByContains,
    utils::{crypt::verify_password, time::now_utc},
};
use std::{collections::HashMap, sync::Arc};

pub struct CredentialService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    acc_svc: Arc<AccountService<D, C>>,
    validator: Arc<AuthValidator>,
    cm: Arc<CacheManager<C>>,
}

impl<D: DbExecutor, C: CacheExecutor> CredentialService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        acc_svc: Arc<AccountService<D, C>>,
        validator: Arc<AuthValidator>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self {
            sm,
            ws_svc,
            acc_svc,
            validator,
            cm,
        }
    }

    /// Authenticates a credential-based (API-key) client.
    ///
    /// This is a public, unauthenticated operation: the caller presents a
    /// `credential_id` + `secret` pair and receives the resolved client auth
    /// scope. All failures collapse to `CoreError::Auth("invalid credentials")`
    /// to avoid leaking which part of the chain failed.
    ///
    /// On success the resolved [`ClientAuthCache`] is written to the cache with
    /// a TTL derived from the credential's `expires_at` (if any), so subsequent
    /// authentication requests can be served from cache without a DB hit.
    pub async fn authenticate(
        &self,
        credential_id: Uuid,
        secret: &str,
    ) -> CoreResult<ClientAuthCache> {
        // 1. Fetch the credential row by id using an unscoped store context —
        //    client credentials are identified globally by their id, not by a
        //    caller-supplied workspace scope.
        let store_ctx = StoreCtx::bootstrap();
        let cred = match self.store().get(&store_ctx, &credential_id.into()).await {
            Ok(row) => row,
            Err(StoreError::EntityNotFound { .. }) => {
                return Err(CoreError::Auth("invalid credentials".to_string()));
            }
            Err(e) => return Err(e.into()),
        };

        // 2. Verify the secret (argon2). A missing stored secret is treated as
        //    an authentication failure (never authenticates).
        let stored_secret = cred.secret.as_deref().unwrap_or("");
        if !verify_password(stored_secret, secret)? {
            return Err(CoreError::Auth("invalid credentials".to_string()));
        }

        // 3. Only active credentials can authenticate.
        if cred.status != CredentialStatus::Active {
            return Err(CoreError::Auth("invalid credentials".to_string()));
        }

        // 4. Reject expired credentials.
        if let Some(t) = cred.expires_at {
            if t <= now_utc() {
                return Err(CoreError::Auth("invalid credentials".to_string()));
            }
        }

        // 5. Resolve the membership -> roles -> permissions graph.
        let mem_id: Uuid = cred.membership_id.into();
        let auth =
            AuthCache::build_from_db(self.sm.clone(), mem_id, cred.account_id.into(), None).await?;

        // 6. Build the client auth cache entry.
        let entry = ClientAuthCache {
            credential_id,
            membership_id: mem_id,
            account_id: auth.acc_id,
            workspace_id: auth.auth_scope.workspace_id,
            roles: auth.auth_scope.roles,
            permissions: auth.auth_scope.permissions,
            expires_at: cred.expires_at,
            status: cred.status,
        };

        // 7. Cache with a TTL derived from the credential expiry (if any).
        let ttl = cred.expires_at.map(|e| {
            let s = (e - now_utc()).whole_seconds();
            if s > 0 { s } else { 0 }
        });
        self.cm.client_auth.write(&entry, ttl).await?;

        Ok(entry)
    }

    async fn get_account(
        &self,
        ctx: &mut CoreCtx,
        id: Uuid,
        _workspace_id: Uuid,
    ) -> CoreResult<Account> {
        ctx.escalate_perms(&["account:describe"])?;

        let account = self
            .acc_svc
            .describe(
                ctx,
                AccountDescribeParams {
                    email: None,
                    id: Some(id),
                },
            )
            .await?;
        Ok(account)
    }
}

// --- Base Model Service ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for CredentialService<D, C> {
    type CoreModel = Credential;
    type ServiceStore = CredentialStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.credential
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

// --- Create ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for CredentialService<D, C> {
    type CreateParams = CredentialCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        mut params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        // OAuth identity data is persisted in canonical form at the service
        // boundary.  The provider subject is opaque and must not be
        // transformed; an absent subject cannot represent an OAuth identity.
        if params.kind == CredentialKind::OAuth {
            if params
                .provider_id
                .as_deref()
                .is_none_or(|provider_id| provider_id.is_empty())
            {
                return Err(CoreError::InvalidParams(
                    "OAuth credential provider_id required".to_string(),
                ));
            }
            params.email = params.email.map(|email| normalize_email(&email));
        }

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        let for_create: CredentialForCreate = params.clone().into();

        let row = match store.create(&store_ctx, for_create).await {
            Ok(row) => row,
            Err(StoreError::ConstraintViolation) => {
                return Err(CoreError::AlreadyExists(
                    "a credential of this kind already exists for this account in this workspace"
                        .to_string(),
                ));
            }
            Err(e) => return Err(e.into()),
        };

        Ok(row.into())
    }
}

// --- Describe (with Hydration) ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for CredentialService<D, C> {
    type DescribeParams = CredentialDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let row = store.get(&store_ctx, &params.id.into()).await?;

        Ok(row.into())
    }
}

// --- List ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for CredentialService<D, C> {
    type ListParams = CredentialListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
            .await?;
        // validate params
        let list_options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;

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

        let mut credentials: Vec<Credential> = data.into_iter().map(|v| v.into()).collect();

        Ok(ListResponse::new(credentials, total, list_options))
    }
}

// --- Update ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for CredentialService<D, C> {
    type UpdateParams = CredentialUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        mut params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        // Read before invalidating caches or issuing an UPDATE.  This keeps a
        // rejected identity mutation side-effect free and protects every
        // supported credential update path (all writes go through this
        // service).  Metadata and lifecycle fields remain independently
        // updateable below.
        let existing = store.get(&store_ctx, &params.id.into()).await?;
        if existing.kind == CredentialKind::OAuth {
            if params
                .kind
                .as_ref()
                .is_some_and(|kind| kind != &existing.kind)
                || params
                    .provider
                    .as_ref()
                    .is_some_and(|provider| provider != &existing.provider)
                || params
                    .new_provider_id
                    .as_ref()
                    .is_some_and(|provider_id| Some(provider_id) != existing.provider_id.as_ref())
            {
                return Err(CoreError::InvalidParams(
                    "OAuth identity fields are immutable; create a new credential to replace the identity"
                        .to_string(),
                ));
            }

            // OAuth email is stored normalized, so an equivalent case or
            // surrounding-whitespace representation is not a mutation.
            params.new_email = params.new_email.map(|email| normalize_email(&email));
            if params
                .new_email
                .as_ref()
                .is_some_and(|email| Some(email) != existing.email.as_ref())
            {
                return Err(CoreError::InvalidParams(
                    "OAuth identity fields are immutable; create a new credential to replace the identity"
                        .to_string(),
                ));
            }
        }

        // Invalidate any cached client auth for this credential BEFORE the store
        // mutation, so no concurrent request can read a stale grant.
        self.cm.client_auth.invalidate(params.id).await?;

        let for_update: CredentialForUpdate = params.clone().into();

        store
            .update(&store_ctx, &params.id.into(), for_update)
            .await?;

        // Also invalidate after a successful mutation, to be safe.
        self.cm.client_auth.invalidate(params.id).await?;

        self.describe(
            ctx,
            CredentialDescribeParams {
                id: params.id,
                account_id: params.account_id,
                workspace_id: params.workspace_id,
                provider_id: None,
                email: None,
            },
        )
        .await
    }
}

// --- Delete ---
impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for CredentialService<D, C> {
    type DeleteParams = CredentialDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.credential.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store = self.store();
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
            .await?;

        let to_delete = self
            .describe(
                ctx,
                CredentialDescribeParams {
                    id: params.id,
                    account_id: params.account_id,
                    workspace_id: params.workspace_id,
                    provider_id: None,
                    email: None,
                },
            )
            .await?;

        // Invalidate any cached client auth for this credential BEFORE the store
        // mutation, so no concurrent request can read a stale grant.
        self.cm.client_auth.invalidate(params.id).await?;

        let res = store.delete(&store_ctx, &params.id.into()).await?;

        // Also invalidate after a successful mutation, to be safe.
        self.cm.client_auth.invalidate(params.id).await?;

        Ok(to_delete)
    }
}
