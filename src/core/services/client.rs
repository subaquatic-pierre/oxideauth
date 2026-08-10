use std::{str::FromStr, sync::Arc};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            client::{
                Client, ClientCreateParams, ClientDeleteParams, ClientDescribeParams,
                ClientListParams, ClientUpdateParams,
            },
            list::ListResponse,
            permission::{PermissionEngine, PermissionRule},
            token::{TokenClaims, TokenType},
        },
        services::{permission::CANONICAL_PERMISSIONS, workspace::WorkspaceService},
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
    ws_svc: WorkspaceService<D>,
    cm: Arc<CacheManager<C>>,
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelService<D> for ClientService<D, C> {
    type CoreModel = Client;
    type ServiceStore = ClientStore<D>;

    fn store(&self) -> &Self::ServiceStore {
        &self.sm.client
    }

    fn ws_svc(&self) -> &WorkspaceService<D> {
        &self.ws_svc
    }
}

impl<D: DbExecutor, C: CacheExecutor> ClientService<D, C> {
    pub fn new(
        sm: Arc<StoreManager<D>>,
        ws_svc: WorkspaceService<D>,
        cm: Arc<CacheManager<C>>,
    ) -> Self {
        Self { sm, ws_svc, cm }
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
        workspace_id: Uuid,
        client_secret: &str,
        user_token: &str,
        required_permissions: &[String],
    ) -> CoreResult<bool> {
        // --- 1. Hash the provided client secret ---
        let secret_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(client_secret.as_bytes());
            format!("{:x}", hasher.finalize())
        };

        // --- 2. Validate the calling Client has permission & scope the store ---
        let (store_ctx, _workspace) = self
            .scope_and_validate_ctx(ctx, workspace_id, &[CANONICAL_PERMISSIONS.client.validate])
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
            user_token,
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
        if Uuid::from_str(claims.ws())
            .map(|ws| ws != workspace_id)
            .unwrap_or(true)
        {
            return Ok(false);
        }

        // --- 6. Build PermissionEngine from the user's cached auth scope ---
        let mem_id = claims
            .mem_id()
            .map_err(|_| CoreError::Auth("invalid token".into()))?;
        let Some(scope) = self.cm.auth.fetch_auth_scope(&mem_id).await? else {
            return Ok(false);
        };
        let checker = match PermissionEngine::from_string_vec(scope.permissions) {
            Ok(checker) => checker,
            Err(_) => return Ok(false),
        };

        // --- 7. Check required permissions against the user's grants ---
        if !required_permissions.is_empty() {
            let required = match PermissionRule::perms_from_string_slice(required_permissions) {
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
        id: Uuid,
        workspace_id: Uuid,
    ) -> CoreResult<ClientSecret> {
        // Validate permissions
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(
                ctx,
                workspace_id,
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

        let client = Client::from_row_with_workspace(row, workspace);
        Ok(ClientSecret {
            client,
            plaintext_secret: sh.plaintext,
        })
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelCreateService<D> for ClientService<D, C> {
    type CreateParams = ClientCreateParams;
    const CREATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.create;

    async fn create(
        &self,
        ctx: &mut CoreCtx,
        params: Self::CreateParams,
    ) -> CoreResult<Self::CoreModel> {
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // Generate a random client secret (48-char alphanumeric string using rand)
        // and store only its SHA-256 hash. The plaintext secret is returned to the
        // caller exactly once, at creation time.
        let sh = self.generate_secret();

        // Build store params from core params + secret_hash
        let for_create = ClientForCreate::from((params, sh.sha256_hash));

        let row = self.store().create(&store_ctx, for_create).await?;
        let client = Client::from_row_with_workspace(row, workspace);

        // TODO: The plaintext secret should be exposed to the caller exactly once.
        // `Client` does not currently carry a `secret` field; this will be addressed
        // by either adding a `secret` field to `Client` (returned only on create) or
        // by returning a `ClientCreated` wrapper containing both the client and the
        // plaintext secret.
        let _ = sh.plaintext;

        Ok(client)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelListService<D> for ClientService<D, C> {
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

        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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
            .map(|row| Client::from_row_with_workspace(row, workspace.clone()))
            .collect();

        Ok(ListResponse::new(clients, total, list_options))
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDescribeService<D> for ClientService<D, C> {
    type DescribeParams = ClientDescribeParams;
    const DESCRIBE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.describe;

    async fn describe(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DescribeParams,
    ) -> CoreResult<Self::CoreModel> {
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
            .await?;

        let row = self.store().get(&store_ctx, &params.id.into()).await?;
        let client = Client::from_row_with_workspace(row, workspace);

        Ok(client)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelUpdateService<D> for ClientService<D, C> {
    type UpdateParams = ClientUpdateParams;
    const UPDATE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.update;

    async fn update(
        &self,
        ctx: &mut CoreCtx,
        params: Self::UpdateParams,
    ) -> CoreResult<Self::CoreModel> {
        let (store_ctx, workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
            .await?;

        let id = params.id;
        let for_update = ClientForUpdate::from(params);
        let row = self
            .store()
            .update(&store_ctx, &id.into(), for_update)
            .await?;
        let client = Client::from_row_with_workspace(row, workspace);

        Ok(client)
    }
}

impl<D: DbExecutor, C: CacheExecutor> CoreModelDeleteService<D> for ClientService<D, C> {
    type DeleteParams = ClientDeleteParams;
    const DELETE_PERMISSION: &'static str = CANONICAL_PERMISSIONS.client.delete;

    async fn delete(
        &self,
        ctx: &mut CoreCtx,
        params: Self::DeleteParams,
    ) -> CoreResult<Self::CoreModel> {
        let (store_ctx, _workspace) = self
            .scope_and_validate_ctx(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
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
