use std::{sync::Arc, vec};

use crate::{
    app::AppState,
    cache::{redis::RedisChx, traits::CacheExecutor},
    core::{
        ctx::{ContextFactory, CoreCtx},
        error::{CoreError, CoreResult},
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
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            credential::{CredentialKind, CredentialProvider, CredentialStatus},
            id::DbId,
            membership::{MembershipScope, MembershipStatus},
            workspace::WorkspaceMeta,
        },
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
    seed_users(&mut ctx, svc_reg.clone()).await?;
    seed_memberships(&mut ctx, svc_reg.clone()).await?;

    // Cache the real UUIDs for all future context construction
    ctx_factory.init_from_seed(&svc_reg.sm).await?;

    Ok(())
}

pub async fn seed_workspaces<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
) -> CoreResult<()> {
    // create global workspace
    let ws = WorkspaceCreateParams {
        name: "Global Workspace".to_string(),
        slug: "global".to_string(),
        description: Some("Global Workspace used for root operations".to_string()),
        config: WorkspaceConfig::default(),
        tags: vec!["system".to_string()],
        meta: WorkspaceMeta::default(),
    };
    svc_reg.workspace.create(ctx, ws).await?;

    // registry workspace
    let ws = WorkspaceCreateParams {
        name: "Registry Workspace".to_string(),
        slug: "registry".to_string(),
        description: Some("Regisrty Workspace used for auditing".to_string()),
        config: WorkspaceConfig::default(),
        tags: vec!["system".to_string()],
        meta: WorkspaceMeta::default(),
    };
    svc_reg.workspace.create(ctx, ws).await?;

    // default workspace
    let ws = WorkspaceCreateParams {
        name: "Default Workspace".to_string(),
        slug: "default".to_string(),
        description: Some("Default Workspace used for general permission access".to_string()),
        config: WorkspaceConfig::default(),
        tags: vec!["system".to_string()],
        meta: WorkspaceMeta::default(),
    };
    svc_reg.workspace.create(ctx, ws).await?;

    Ok(())
}

pub async fn seed_users<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
) -> CoreResult<()> {
    // 1. Look up the global workspace (created by seed_workspaces)
    let global_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some("global".to_string()),
                id: None,
            },
        )
        .await?;
    let ws_id = global_ws.id;

    // 2. Root account
    let root = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "root@system.local".to_string(),
                password: "rootpass".to_string(),
                name: "Root Account".to_string(),
                workspace_id: ws_id,
                description: Some("Root system account with full privileges".to_string()),
                tags: Some(vec!["system".to_string()]),
                ..Default::default()
            },
        )
        .await?;

    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: root.id,
                workspace_id: ws_id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password("rootpass")?),
                provider_id: None,
                email: None,
                last_used_at: None,
                tags: vec!["system".to_string()],
                meta: CredentialMeta {
                    schema_version: "1".to_string(),
                },
            },
        )
        .await?;

    // 3. Owner account
    let owner = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "owner@system.local".to_string(),
                password: "ownerpass".to_string(),
                name: "Owner Account".to_string(),
                workspace_id: ws_id,
                description: Some("Workspace owner account".to_string()),
                tags: Some(vec!["system".to_string()]),
                ..Default::default()
            },
        )
        .await?;

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
                secret: Some(hash_password("ownerpass")?),
                provider_id: None,
                email: None,
                last_used_at: None,
                tags: vec!["system".to_string()],
                meta: CredentialMeta {
                    schema_version: "1".to_string(),
                },
            },
        )
        .await?;

    // 4. Test account
    let test = svc_reg
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

    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: test.id,
                workspace_id: ws_id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password("testpass")?),
                provider_id: None,
                email: None,
                last_used_at: None,
                tags: vec!["system".to_string()],
                meta: CredentialMeta {
                    schema_version: "1".to_string(),
                },
            },
        )
        .await?;

    tracing::info!("Seed users created: root, owner, test");
    Ok(())
}

pub async fn seed_memberships<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: Arc<ServiceRegistry<D, C>>,
) -> CoreResult<()> {
    // --- Look up workspaces by slug ---
    let global_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some("global".to_string()),
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
    let registry_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some("registry".to_string()),
                id: None,
            },
        )
        .await?;
    let store_ctx = ctx.into();
    // --- Look up roles by name in each workspace ---
    let global_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, "Workspace Admin", DbId(global_ws.id))
        .await?
        .expect("Workspace Admin role not found in global workspace");
    let default_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, "Workspace Admin", DbId(default_ws.id))
        .await?
        .expect("Workspace Admin role not found in default workspace");
    let default_viewer = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, "Workspace Viewer", DbId(default_ws.id))
        .await?
        .expect("Workspace Viewer role not found in default workspace");
    let registry_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, "Workspace Admin", DbId(registry_ws.id))
        .await?
        .expect("Workspace Admin role not found in registry workspace");

    // --- Look up accounts by email ---
    let root = svc_reg
        .sm
        .account
        .get_by_email(&store_ctx, "root@system.local")
        .await?
        .expect("Root account not found");
    let owner = svc_reg
        .sm
        .account
        .get_by_email(&store_ctx, "owner@system.local")
        .await?
        .expect("Owner account not found");
    let test = svc_reg
        .sm
        .account
        .get_by_email(&store_ctx, "test@example.com")
        .await?
        .expect("Test account not found");

    // --- Build membership params ---
    let memberships: Vec<MembershipCreateParams> = vec![
        // root → global (admin)
        MembershipCreateParams {
            account_id: root.id.into(),
            workspace_id: global_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![global_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // owner → global (admin)
        MembershipCreateParams {
            account_id: owner.id.into(),
            workspace_id: global_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![global_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // root → registry (admin)
        MembershipCreateParams {
            account_id: root.id.into(),
            workspace_id: registry_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![registry_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // owner → registry (admin)
        MembershipCreateParams {
            account_id: owner.id.into(),
            workspace_id: registry_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![registry_admin.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // owner → default (admin)
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
        // test → default (viewer)
        MembershipCreateParams {
            account_id: test.id.into(),
            workspace_id: default_ws.id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            role_ids: vec![default_viewer.id.into()],
            tags: vec!["system".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
    ];

    // --- Create all memberships ---
    for params in memberships {
        ctx.extend_perms(&["membership:create"])?;
        svc_reg.membership.create(ctx, params).await?;
    }

    tracing::info!("Seed memberships created");
    Ok(())
}
