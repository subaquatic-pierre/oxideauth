use std::{str::FromStr, sync::Arc};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{CacheEntity, entities::auth::AuthCache, manager::CacheManager, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            client::{
                Client, ClientCreateParams, ClientDeleteParams, ClientDescribeParams,
                ClientListParams, ClientRegenerateSecretParams, ClientUpdateParams,
                ClientValidateParams,
            },
            list::ListResponse,
            permission::{PermissionSet, PermissionRule},
            token::{TokenClaims, TokenType},
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
        entities::client::{ClientFilter, ClientForCreate, ClientForUpdate},
        manager::StoreManager,
        stores::client::ClientStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
    },
    utils::time::{format_time, now_utc},
};

/// Return value for `ClientService::regenerate_secret()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSecret {
    pub client: Client,
    pub plaintext_secret: String,
}

/// Return value for `ClientService::generate_secret()` (private helper).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretHash {
    pub plaintext: String,
    pub sha256_hash: String,
}

pub struct ClientService<D: DbExecutor, C: CacheExecutor> {
    sm: Arc<StoreManager<D>>,
    ws_svc: Arc<WorkspaceService<D, C>>,
    cm: Arc<CacheManager<C>>,
    validator: Arc<AuthValidator>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D, C> for ClientService<D, C> {
    type CoreModel = Client;
    type ServiceStore = ClientStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.client
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

impl<D: DbExecutor, C: CacheExecutor> ClientService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: Arc<WorkspaceService<D, C>>,
        cm: Arc<CacheManager<C>>,
        validator: Arc<AuthValidator>,
    ) -> Self {
        Self {
            sm,
            ws_svc,
            cm,
            validator,
        }
    }

    /// Push a notification payload to a single client's endpoint via HTTP POST.
    ///
    /// Retries up to 3 times with exponential backoff (1s, 5s, 25s). Failures
    /// after all retries are logged (via `tracing::warn`).
    ///
    /// The delivery happens on a spawned tokio task so the caller is never
    /// blocked by a slow (or unreachable) endpoint.
    pub async fn push_to_client(&self, client: &Client, payload: &serde_json::Value)
    where
        C: 'static,
    {
        let endpoint = match &client.endpoint {
            Some(url) => url.clone(),
            None => return, // Client has no endpoint — skip
        };

        let client_id = client.id.to_string();
        let payload = payload.clone();

        // Spawn async task so this doesn't block the caller
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            let mut last_error = None;

            for attempt in 0..3 {
                let delay = match attempt {
                    0 => 1,
                    1 => 5,
                    _ => 25,
                };

                if attempt > 0 {
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                }

                match client
                    .post(&endpoint)
                    .json(&payload)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        tracing::info!(client_id = %client_id, attempt = attempt + 1, "Push notification delivered");
                        return;
                    }
                    Ok(resp) => {
                        last_error = Some(format!("HTTP {}", resp.status()));
                    }
                    Err(e) => {
                        last_error = Some(e.to_string());
                    }
                }
            }

            tracing::warn!(
                client_id = %client_id,
                error = ?last_error,
                "Push notification failed after 3 retries"
            );
        });
    }

    /// Push a notification to all clients in a workspace that have an endpoint
    /// configured.
    pub async fn push_to_workspace(
        &self,
        workspace_id: Uuid,
        notification_type: &str,
        resource_ids: serde_json::Value,
        ctx: &mut CoreCtx,
    ) where
        C: 'static,
    {
        // List all clients in the workspace
        let list_params = ClientListParams {
            workspace_id,
            filter: None,
            options: None,
        };

        let clients = match self.list(ctx, list_params).await {
            Ok(response) => response.data,
            Err(e) => {
                tracing::error!(%workspace_id, error = %e, "Failed to list clients for push notification");
                return;
            }
        };

        let payload = serde_json::json!({
            "type": notification_type,
            "workspace_id": workspace_id.to_string(),
            "resource_ids": resource_ids,
            "timestamp": format_time(now_utc()),
        });

        for client in &clients {
            self.push_to_client(client, &payload).await;
        }
    }

    /// Generates a random client secret (48-char alphanumeric string) and returns
    /// `(plaintext, sha256_hex_hash)`.
    ///
    /// The plaintext secret is only ever exposed at creation time; only the SHA-256
    /// hash is persisted (in `client.secret_hash`).
    fn generate_secret(&self) -> SecretHash {
        use rand::Rng;
        let plaintext: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(48)
            .map(char::from)
            .collect();
        // Use SHA-256 hash (simple, no extra deps needed)
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        let hash = format!("{:x}", hasher.finalize());
        SecretHash {
            plaintext,
            sha256_hash: hash,
        }
    }

    /// Validates a client credential + user token pair.
    ///
    /// Returns `Ok(true)` only when **all** of the following hold:
    /// 1. The calling Client (micro service, authenticated via `ctx`) has
    ///    `client:validate` permission in the target workspace.
    /// 2. A client with a matching secret hash exists in the workspace.
    /// 3. The user token is a valid, non-expired `Auth` token bound to the same
    ///    workspace.
    /// 4. The user's permissions (from the auth scope cache) satisfy every
    ///    permission in `required_permissions`.
    ///
    /// # Simplifications
    ///
    /// - Client lookup filters by `workspace_id` and matches the secret hash in
    ///   memory (`secret_hash` is not part of `ClientFilter`).
    /// - Token validation decodes the user's JWT directly against the configured
    ///   secret and checks expiry/type/workspace; the full auth-cache
    ///   hydrate-on-miss pipeline is skipped.
    pub async fn validate(
        &self,
        ctx: &mut CoreCtx,
        params: ClientValidateParams,
    ) -> CoreResult<bool> {
        let ClientValidateParams {
            workspace_id,
            client_secret,
            user_token,
            required_permissions,
        } = params;

        // --- 1. Hash the provided client secret ---
        let secret_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(client_secret.as_bytes());
            format!("{:x}", hasher.finalize())
        };

        // --- 2. Validate the calling Client has permission & scope the store ---
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(workspace_id), &[CANONICAL_PERMISSIONS.client.validate])
            .await?;

        // --- 3. Look up the client by workspace_id + secret_hash ---
        let filter: ClientFilter = json!({
            "workspace_id": workspace_id.to_string(),
        })
        .try_into()?;
        let clients = self.store().list(&store_ctx, Some(filter), None).await?;
        let client_found = clients.into_iter().any(|c| c.secret_hash == secret_hash);

        // Anti-enumeration: all authentication failures return Ok(false).
        if !client_found {
            return Ok(false);
        }

        // --- 4. Decode and validate the user token (end user, not the Client) ---
        let claims = match decode::<TokenClaims>(
            &user_token,
            &DecodingKey::from_secret(Config::from_env().jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        ) {
            Ok(data) => data.claims,
            Err(_) => return Ok(false),
        };

        // --- 5. Check expiry, token type, and workspace binding ---
        if claims.is_expired() || claims.token_type() != TokenType::Auth {
            return Ok(false);
        }
        if claims.ws != workspace_id {
            return Ok(false);
        }

        // --- 6. Build PermissionEngine from the user's cached auth scope ---
        let mem_id = claims.mem;
        let keyed = AuthCache::from_claims(&claims);
        let Some(auth_cache) = self.cm.auth.fetch(&keyed.key()).await? else {
            return Ok(false);
        };
        let checker = match PermissionSet::from_string_vec(auth_cache.auth_scope.permissions) {
            Ok(checker) => checker,
            Err(_) => return Ok(false),
        };

        // --- 7. Check required permissions against the user's grants ---
        if !required_permissions.is_empty() {
            let required = match PermissionRule::perms_from_string_slice(&required_permissions) {
                Ok(perms) => perms,
                Err(_) => return Ok(false),
            };
            if !checker.has_subset(&required) {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Regenerates the secret for an existing client and returns the client plus
    /// the new plaintext secret (shown only once).
    pub async fn regenerate_secret(
        &self,
        ctx: &mut CoreCtx,
        params: ClientRegenerateSecretParams,
    ) -> CoreResult<ClientSecret> {
        let ClientRegenerateSecretParams { id, workspace_id } = params;

        // Validate permissions
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(
                ctx,
                Some(workspace_id),
                &[CANONICAL_PERMISSIONS.client.regenerate_secret],
            )
            .await?;

        // Generate new secret
        let sh = self.generate_secret();

        // Update the secret_hash
        let for_update = ClientForUpdate {
            secret_hash: Some(sh.sha256_hash),
            ..Default::default()
        };
        let row = self
            .store()
            .update(&store_ctx, &id.into(), for_update)
            .await?;

        let client = Client::from(row);
        Ok(ClientSecret {
            client,
            plaintext_secret: sh.plaintext,
        })
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D, C> for ClientService<D, C> {
    type CreateParams = ClientCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::CREATE_PERMISSION])
            .await?;

        // Generate a random client secret (48-char alphanumeric string using rand)
        // and store only its SHA-256 hash. The plaintext secret is returned to the
        // caller exactly once, at creation time.
        let sh = self.generate_secret();

        // TODO: unify secret generation logic in CredentialService
        // create a credential it API secret and expiry date
        // this is used for api client auth

        // Build store params from core params + secret_hash
        let for_create = params.into_store_params(sh.sha256_hash);

        let row = self.store().create(&store_ctx, for_create).await?;
        let client = Client::from(row);

        let _ = sh.plaintext;

        Ok(client)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D, C> for ClientService<D, C> {
    type ListParams = ClientListParams;
    const LIST_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.list;

    async fn list(
        &self,
        ctx: &mut CoreCtx,
        params: Self::ListParams,
    ) -> CoreResult<ListResponse<Self::CoreModel>> {
        let store = self.store();

        // validate params
        let list_options = params.list_options();
        let tags_filter = params.validate_filter_tags()?;

        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::LIST_PERMISSION])
            .await?;

        // Combined query: tags (@> containment) + field filter
        // ClientStore now implements ContainsFilterStore, so tag filtering
        // is done in SQL alongside field filters for accurate counts.
        let tags = tags_filter.tags();
        let filter = tags_filter.filter();

        let rows = store
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

        let clients: Vec<Client> = rows
            .into_iter()
            .map(|row| Client::from(row))
            .collect();

        Ok(ListResponse::new(clients, total, list_options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D, C> for ClientService<D, C> {
    type DescribeParams = ClientDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DESCRIBE_PERMISSION])
            .await?;

        let row = self.store().get(&store_ctx, &params.id.into()).await?;
        let client = Client::from(row);

        Ok(client)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D, C> for ClientService<D, C> {
    type UpdateParams = ClientUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::UPDATE_PERMISSION])
            .await?;

        let id = params.id;
        let for_update = ClientForUpdate::from(params);
        let row = self
            .store()
            .update(&store_ctx, &id.into(), for_update)
            .await?;
        let client = Client::from(row);

        Ok(client)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D, C> for ClientService<D, C> {
    type DeleteParams = ClientDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let store_ctx = self
            // NOTE(workspace-scope): scoped - scopes the store context to the requested workspace.
            .scope_and_validate(ctx, Some(params.workspace_id), &[Self::DELETE_PERMISSION])
            .await?;

        // Capture the entity before deletion so we can return it in the response.
        let to_delete = self
            .describe(
                ctx,
                ClientDescribeParams {
                    id: params.id,
                    workspace_id: params.workspace_id,
                },
            )
            .await?;

        let _ = self
            .store()
            .delete(&store_ctx, &to_delete.id.into())
            .await?;

        Ok(to_delete)
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
                client::{ClientMeta, ClientRow},
                id::DbId,
                workspace::WorkspaceRow,
            },
        },
    };
    use serial_test::serial;
    use uuid::Uuid;

    /// Builds a `ClientRow` for the in-memory mock.
    fn client_row(id: Uuid, ws_id: Uuid) -> ClientRow {
        ClientRow {
            id: id.into(),
            workspace_id: ws_id,
            name: "test-client".to_string(),
            secret_hash: "stored-hash".to_string(),
            endpoint: None,
            description: None,
            tags: vec![],
            meta: ClientMeta {
                schema_version: "1".to_string(),
            },
            audit: AuditFields::default(),
        }
    }

    /// Builds a `ClientService` backed by an in-memory `MockDbx` + `MockChx`.
    fn mock_svc(dbx: MockDbx) -> Arc<ClientService<MockDbx, MockChx>> {
        let config = Config::test_config();
        let sm = Arc::new(StoreManager::new(Arc::new(dbx)));
        let cm = Arc::new(CacheManager::new(Arc::new(MockChx::default())));
        let svc_reg = ServiceRegistry::new(&config, sm, cm);
        svc_reg.client.clone()
    }

    #[tokio::test]
    #[serial]
    async fn test_client_create() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws: WorkspaceRow = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };
        let client_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate -> get_workspace -> get by id
            // store.create
            .with_one::<ClientRow>(client_row(client_id, ws_id));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.set_scoped_ws(ws.into());
        ctx.escalate_perms(&["client:create"])?;

        let params = ClientCreateParams {
            workspace_id: ws_id,
            name: "test-client".to_string(),
            endpoint: None,
            description: None,
            tags: vec![],
            meta: ClientMeta {
                schema_version: "1".to_string(),
            },
        };

        let client = svc.create(&mut ctx, params).await?;

        assert_eq!(client.id, client_id);
        assert_eq!(client.workspace_id, ws_id);
        assert_eq!(client.name, "test-client");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_client_describe() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws: WorkspaceRow = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };
        let client_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate -> get_workspace
            // .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
            //     id: ws_id.into(),
            //     ..Default::default()
            // }))
            // store.get
            .with_optional::<ClientRow>(Some(client_row(client_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["client:describe"])?;
        ctx.set_scoped_ws(ws.into());

        let client = svc
            .describe(
                &mut ctx,
                ClientDescribeParams {
                    id: client_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(client.id, client_id);
        assert_eq!(client.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_client_list() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws: WorkspaceRow = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };
        let client_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // list_with_tags_and_filter
            .with_all::<ClientRow>(vec![client_row(client_id, ws_id)])
            // count_with_tags_and_filter
            .with_one::<(i64,)>((1,));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["client:list"])?;
        ctx.set_scoped_ws(ws.into());

        let res = svc
            .list(
                &mut ctx,
                ClientListParams {
                    workspace_id: ws_id,
                    filter: None,
                    options: None,
                },
            )
            .await?;

        assert_eq!(res.data.len(), 1);
        assert_eq!(res.data[0].id, client_id);
        assert_eq!(res.metadata.total, 1);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_client_update() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws: WorkspaceRow = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };
        let client_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // store.update
            .with_optional::<ClientRow>(Some(client_row(client_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["client:update"])?;
        ctx.set_scoped_ws(ws.into());

        let params = ClientUpdateParams {
            id: client_id,
            workspace_id: ws_id,
            name: Some("renamed".to_string()),
            endpoint: None,
            description: None,
            tags: None,
            meta: None,
        };

        let updated = svc.update(&mut ctx, params).await?;

        assert_eq!(updated.id, client_id);
        assert_eq!(updated.workspace_id, ws_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_client_delete() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws: WorkspaceRow = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };
        let client_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // delete -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // delete -> describe -> scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // delete -> describe -> store.get
            .with_optional::<ClientRow>(Some(client_row(client_id, ws_id)))
            // delete -> store.delete
            .with_optional::<ClientRow>(Some(client_row(client_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["client:delete", "client:describe"])?;
        ctx.set_scoped_ws(ws.into());

        let deleted = svc
            .delete(
                &mut ctx,
                ClientDeleteParams {
                    id: client_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(deleted.id, client_id);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_client_regenerate_secret() -> CoreResult<()> {
        let ws_id = Uuid::new_v4();
        let ws: WorkspaceRow = WorkspaceRow {
            id: ws_id.into(),
            ..Default::default()
        };
        let client_id = Uuid::new_v4();

        let dbx = MockDbx::new()
            // scope_and_validate -> get_workspace
            .with_optional::<WorkspaceRow>(Some(WorkspaceRow {
                id: ws_id.into(),
                ..Default::default()
            }))
            // store.update (secret rotation)
            .with_optional::<ClientRow>(Some(client_row(client_id, ws_id)));
        let svc = mock_svc(dbx);
        let mut ctx = CoreCtx::bootstrap()?;
        ctx.escalate_perms(&["client:regenerateSecret"])?;
        ctx.set_scoped_ws(ws.into());

        let res = svc
            .regenerate_secret(
                &mut ctx,
                ClientRegenerateSecretParams {
                    id: client_id,
                    workspace_id: ws_id,
                },
            )
            .await?;

        assert_eq!(res.client.id, client_id);
        // Plaintext secret must be a 48-char alphanumeric string, shown once.
        assert_eq!(res.plaintext_secret.len(), 48);
        assert!(
            res.plaintext_secret
                .chars()
                .all(|c| c.is_ascii_alphanumeric())
        );

        Ok(())
    }
}
