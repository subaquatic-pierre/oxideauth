use std::sync::Arc;

use uuid::Uuid;

use crate::{
    cache::{manager::CacheManager, traits::CacheExecutor},
    core::{
        ctx::CoreCtx,
        error::CoreResult,
        models::{
            client::{
                Client, ClientCreateParams, ClientDeleteParams, ClientDescribeParams,
                ClientListParams, ClientUpdateParams,
            },
            list::ListResponse,
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
        entities::client::{ClientForCreate, ClientForUpdate},
        manager::StoreManager,
        stores::client::ClientStore,
        traits::{contains::FilterByContains, crud::*, dbx::DbExecutor},
    },
    utils::time::{format_time, now_utc},
};

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
            workspace_id: Some(workspace_id),
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::CREATE_PERMISSION])
            .await?;

        // Build store params from core params.
        let for_create = params.into_store_params();

        let row = self.store().create(&store_ctx, for_create).await?;
        let client = Client::from(row);

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
            .scope_and_validate(ctx, params.workspace_id, &[Self::LIST_PERMISSION])
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

        let clients: Vec<Client> = rows.into_iter().map(|row| Client::from(row)).collect();

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
            .scope_and_validate(ctx, params.workspace_id, &[Self::DESCRIBE_PERMISSION])
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::UPDATE_PERMISSION])
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
            .scope_and_validate(ctx, params.workspace_id, &[Self::DELETE_PERMISSION])
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

