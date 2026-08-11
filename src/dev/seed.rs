use std::sync::Arc;

use uuid::Uuid;

use crate::{
    app::AppState,
    cache::{redis::RedisChx, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::CoreResult,
        models::{
            account::AccountCreateParams,
            credential::{CredentialCreateParams, CredentialMeta},
            membership::{MembershipCreateParams, MembershipMeta},
            workspace::{WorkspaceConfig, WorkspaceCreateParams, WorkspaceDescribeParams},
        },
        services::registry::ServiceRegistry,
        traits::service::{CoreModelCreateService, CoreModelDescribeService},
    },
    store::{
        crud::Update,
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            credential::{CredentialConfig, CredentialKind, CredentialProvider, CredentialStatus},
            id::DbId,
            membership::{MembershipScope, MembershipStatus},
            workspace::{WorkspaceForUpdate, WorkspaceMeta},
        },
        stores::workspace::SYSTEM_CONST,
        traits::dbx::DbExecutor,
    },
    utils::crypt::hash_password,
};

pub async fn seed_all<D: DbExecutor, C: CacheExecutor>(
    app: &AppState<PgDbx, RedisChx>,
) -> CoreResult<()> {
    let svc_reg = app.svc_reg.clone();
    let ctx_factory = app.ctx_factory.clone();
    let mut ctx = CoreCtx::bootstrap()?;

    seed_workspaces(&mut ctx, svc_reg.clone()).await?;
    seed_users(&mut ctx, svc_reg.clone(), &app.config).await?;
    seed_memberships(&mut ctx, svc_reg.clone(), &app.config).await?;

    ctx_factory.init_from_seed(&svc_reg.sm).await?;
    Ok(())
}

pub async fn seed_workspaces<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
) -> CoreResult<()> {
    // System workspace
    svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: "System Workspace".to_string(),
                slug: SYSTEM_CONST.system_ws_slug.to_string(),
                owner: Uuid::nil(),
                description: Some("System workspace for root operations".to_string()),
                config: WorkspaceConfig::default(),
                tags: vec!["system".to_string()],
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    // Default workspace
    svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: "Default Workspace".to_string(),
                slug: "default".to_string(),
                owner: Uuid::nil(),
                description: Some("Default workspace for general access".to_string()),
                config: WorkspaceConfig::default(),
                tags: vec!["system".to_string()],
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    Ok(())
}

pub async fn seed_users<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
    config: &Config,
) -> CoreResult<()> {
    let system_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some(SYSTEM_CONST.system_ws_slug.to_string()),
                id: None,
            },
        )
        .await?;
    let ws_id = system_ws.id;

    // System account (no password credential — internal use only)
    let system = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: SYSTEM_CONST.system_acc_email.to_string(),
                password: String::new(),
                name: SYSTEM_CONST.system_acc_name.to_string(),
                workspace_id: ws_id,
                description: Some("System account for internal operations".to_string()),
                tags: Some(vec!["system".to_string()]),
                ..Default::default()
            },
        )
        .await?;

    // Owner account (from config/env)
    let owner = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: config.owner_email.clone(),
                password: config.owner_password.clone(),
                name: config.owner_name.clone(),
                workspace_id: ws_id,
                description: Some("Workspace owner account".to_string()),
                tags: Some(vec!["system".to_string()]),
                ..Default::default()
            },
        )
        .await?;

    // Owner credential
    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: owner.id,
                workspace_id: ws_id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password(&config.owner_password)?),
                provider_id: None,
                email: None,
                config: CredentialConfig::default(),
                last_used_at: None,
                tags: vec!["system".to_string()],
                meta: CredentialMeta {
                    schema_version: "1".to_string(),
                },
            },
        )
        .await?;

    // Update workspace owners via the store directly (since WorkspaceUpdateParams doesn't have owner field)
    let store_ctx: StoreCtx = (&*ctx).into();

    let system_ws_db_id = DbId(system_ws.id);
    svc_reg
        .sm
        .workspace
        .update(
            &store_ctx,
            &system_ws_db_id,
            WorkspaceForUpdate {
                name: None,
                slug: None,
                owner: Some(system.id),
                description: None,
                config: None,
                tags: None,
                meta: None,
            },
        )
        .await?;

    let default_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some("default".to_string()),
                id: None,
            },
        )
        .await?;
    let default_ws_db_id = DbId(default_ws.id);
    svc_reg
        .sm
        .workspace
        .update(
            &store_ctx,
            &default_ws_db_id,
            WorkspaceForUpdate {
                name: None,
                slug: None,
                owner: Some(owner.id),
                description: None,
                config: None,
                tags: None,
                meta: None,
            },
        )
        .await?;

    tracing::info!("Seed users created: system, owner");
    Ok(())
}

pub async fn seed_memberships<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
    config: &Config,
) -> CoreResult<()> {
    let system_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some(SYSTEM_CONST.system_ws_slug.to_string()),
                id: None,
            },
        )
        .await?;
    let default_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some("default".to_string()),
                id: None,
            },
        )
        .await?;

    let store_ctx: StoreCtx = (&*ctx).into();

    // Look up admin role in each workspace
    let system_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, "Workspace Admin", DbId(system_ws.id))
        .await?
        .expect("Workspace Admin role not found in system workspace");
    let default_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, "Workspace Admin", DbId(default_ws.id))
        .await?
        .expect("Workspace Admin role not found in default workspace");

    // Look up accounts by email
    let system = svc_reg
        .sm
        .account
        .get_by_email(&store_ctx, SYSTEM_CONST.system_acc_email)
        .await?
        .expect("System account not found");
    let owner = svc_reg
        .sm
        .account
        .get_by_email(&store_ctx, &config.owner_email)
        .await?
        .expect("Owner account not found");

    // Build memberships: system → system workspace admin, owner → system + default admin
    let memberships: Vec<MembershipCreateParams> = vec![
        MembershipCreateParams {
            account_id: system.id.into(),
            workspace_id: system_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![system_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        MembershipCreateParams {
            account_id: owner.id.into(),
            workspace_id: system_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![system_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        MembershipCreateParams {
            account_id: owner.id.into(),
            workspace_id: default_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![default_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
    ];

    for params in memberships {
        ctx.extend_perms(&["membership:create"])?;
        svc_reg.membership.create(ctx, params).await?;
    }

    tracing::info!("Seed memberships created");
    Ok(())
}

/// Seeds test data (workspace, project, accounts).
/// This is optional — callers can skip it for production-like environments.
pub async fn seed_test_data<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
) -> CoreResult<()> {
    // Create a test workspace
    let test_ws = svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: "Test Workspace".to_string(),
                slug: "test".to_string(),
                owner: Uuid::nil(),
                description: Some("Test workspace for development".to_string()),
                config: WorkspaceConfig::default(),
                tags: vec!["test".to_string()],
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    let ws_id = test_ws.id;

    // Create a test account
    let test_account = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "test@example.com".to_string(),
                password: "testpass".to_string(),
                name: "Test Account".to_string(),
                workspace_id: ws_id,
                description: Some("General test account".to_string()),
                ..Default::default()
            },
        )
        .await?;

    // Test credential
    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: test_account.id,
                workspace_id: ws_id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password("testpass")?),
                provider_id: None,
                email: None,
                config: CredentialConfig::default(),
                last_used_at: None,
                tags: vec!["test".to_string()],
                meta: CredentialMeta {
                    schema_version: "1".to_string(),
                },
            },
        )
        .await?;

    tracing::info!("Seed test data created");
    Ok(())
}
