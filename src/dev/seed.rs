use std::sync::Arc;

use uuid::Uuid;

use crate::cache::entities::workspace::WorkspaceCache;
use crate::core::traits::service::CoreModelUpdateService;
use crate::{
    app::AppState,
    cache::{redis::RedisChx, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::CoreResult,
        models::{
            account::{AccountCreateParams, AccountDescribeParams},
            credential::{CredentialCreateParams, CredentialMeta},
            membership::{MembershipCreateParams, MembershipMeta},
            workspace::{
                WorkspaceConfig, WorkspaceCreateParams, WorkspaceDescribeParams,
                WorkspaceUpdateParams,
            },
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
    let ctx_factory = app.svc_reg.ctx_factory.clone();
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
                owner: None,
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
                owner: None,
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
                ..Default::default()
            },
        )
        .await?;

    // System account (no password credential — internal use only)
    let system_acc = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: SYSTEM_CONST.system_acc_email.to_string(),
                name: SYSTEM_CONST.system_acc_name.to_string(),
                description: Some("System account for internal operations".to_string()),
                tags: Some(vec!["system".to_string()]),
                ..Default::default()
            },
        )
        .await?;

    // Owner account (from config/env)
    let owner_acc = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: config.owner_email.clone(),
                name: config.owner_name.clone(),
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
                account_id: owner_acc.id,
                workspace_id: system_ws.id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password(&config.owner_password)?),
                provider_id: None,
                email: None,
                config: CredentialConfig::default(),
                last_used_at: None,
                tags: vec!["system".to_string()],
                meta: CredentialMeta::default(),
            },
        )
        .await?;

    // Update workspace owners via the store directly (since WorkspaceUpdateParams doesn't have owner field)
    svc_reg
        .workspace
        .update(
            ctx,
            WorkspaceUpdateParams {
                id: Some(system_ws.id),
                slug: None,
                name: None,
                owner: Some(system_ws.id),
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
                slug: Some(SYSTEM_CONST.default_ws_slug.to_string()),
                ..Default::default()
            },
        )
        .await?;
    svc_reg
        .workspace
        .update(
            ctx,
            WorkspaceUpdateParams {
                id: Some(default_ws.id),
                owner: Some(owner_acc.id),
                ..Default::default()
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
                ..Default::default()
            },
        )
        .await?;
    let default_ws = svc_reg
        .workspace
        .describe(
            ctx,
            WorkspaceDescribeParams {
                slug: Some(SYSTEM_CONST.default_ws_slug.to_string()),
                ..Default::default()
            },
        )
        .await?;

    // NOTE(workspace-scope): scoped — seed roles scoped to the system workspace.
    let mut store_ctx: StoreCtx = (&*ctx).into();

    // Look up admin role in each workspace (scoped to the expected workspace)
    store_ctx.set_workspace_scope(Some(system_ws.id));
    let system_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_admin_role,
            DbId(system_ws.id),
        )
        .await?
        .expect("Workspace Admin role not found in system workspace");

    store_ctx.set_workspace_scope(Some(default_ws.id));
    let default_admin = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_admin_role,
            DbId(default_ws.id),
        )
        .await?
        .expect("Workspace Admin role not found in default workspace");

    // Look up accounts by email
    let system = svc_reg
        .account
        .get_by_email(
            &ctx,
            &AccountDescribeParams {
                id: None,
                email: Some(SYSTEM_CONST.system_acc_email.to_string()),
            },
        )
        .await?
        .expect("System account not found");
    let owner = svc_reg
        .account
        .get_by_email(
            &ctx,
            &AccountDescribeParams {
                id: None,
                email: Some(config.owner_email.clone()),
            },
        )
        .await?
        .expect("Owner account not found");

    let system_ws_id = system_ws.id.clone();
    let default_ws_id = default_ws.id.clone();

    // Build memberships: system → system workspace admin, owner → system + default admin
    let memberships: Vec<(WorkspaceCache, MembershipCreateParams)> = vec![
        (
            system_ws.clone().into(),
            MembershipCreateParams {
                account_id: Some(system.id.into()),
                email: None,
                workspace_id: system_ws_id,
                scope: MembershipScope::Workspace,
                status: Some(MembershipStatus::Active),
                profile_id: None,
                project_id: None,
                role_ids: vec![system_admin.id.into()],
                policy_ids: vec![],
                tags: vec!["system".to_string()],
                meta: MembershipMeta {
                    schema_version: "1".to_string(),
                },
            },
        ),
        (
            system_ws.into(),
            MembershipCreateParams {
                account_id: Some(owner.id.into()),
                email: None,
                workspace_id: system_ws_id,
                scope: MembershipScope::Workspace,
                status: Some(MembershipStatus::Active),
                profile_id: None,
                project_id: None,
                role_ids: vec![system_admin.id.into()],
                policy_ids: vec![],
                tags: vec!["system".to_string()],
                meta: MembershipMeta {
                    schema_version: "1".to_string(),
                },
            },
        ),
        (
            default_ws.into(),
            MembershipCreateParams {
                account_id: Some(owner.id.into()),
                email: None,
                workspace_id: default_ws_id,
                scope: MembershipScope::Workspace,
                status: Some(MembershipStatus::Active),
                profile_id: None,
                project_id: None,
                role_ids: vec![default_admin.id.into()],
                policy_ids: vec![],
                tags: vec!["system".to_string()],
                meta: MembershipMeta {
                    schema_version: "1".to_string(),
                },
            },
        ),
    ];

    for (ws, params) in memberships {
        ctx.set_scoped_ws(ws);
        ctx.escalate_perms(&["membership:create"])?;
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
    // =========================================================================
    // WORKSPACE 1: Private "test" workspace
    // =========================================================================
    let test_ws = svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: "Test Workspace".to_string(),
                slug: "test".to_string(),
                owner: None,
                description: Some("Private test workspace for development".to_string()),
                config: WorkspaceConfig::default(),
                tags: vec!["test".to_string()],
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    // Test account — owner of test workspace
    let test_account = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "test@example.com".to_string(),
                name: "Test Account".to_string(),
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
                workspace_id: test_ws.id,
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

    // Set owner of test workspace
    svc_reg
        .workspace
        .update(
            ctx,
            WorkspaceUpdateParams {
                id: Some(test_ws.id),
                owner: Some(test_account.id),
                ..Default::default()
            },
        )
        .await?;

    // =========================================================================
    // WORKSPACE 2: Public "public-test" workspace
    // =========================================================================
    let public_ws = svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: "Public Test Workspace".to_string(),
                slug: "public-test".to_string(),
                owner: None,
                description: Some("Public test workspace — anyone can browse".to_string()),
                config: WorkspaceConfig {
                    public: true,
                    ..Default::default()
                },
                tags: vec!["test".to_string(), "public".to_string()],
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    // Admin for public workspace
    let public_admin = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "public-admin@example.com".to_string(),
                name: "Public Admin".to_string(),
                description: Some("Admin of the public test workspace".to_string()),
                ..Default::default()
            },
        )
        .await?;

    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: public_admin.id,
                workspace_id: public_ws.id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password("adminpass")?),
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

    // Set owner of public workspace
    svc_reg
        .workspace
        .update(
            ctx,
            WorkspaceUpdateParams {
                id: Some(public_ws.id),
                owner: Some(public_admin.id),
                ..Default::default()
            },
        )
        .await?;

    // =========================================================================
    // WORKSPACE 3: Private "private-test" workspace
    // =========================================================================
    let private_ws = svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: "Private Test Workspace".to_string(),
                slug: "private-test".to_string(),
                owner: None,
                description: Some("Private test workspace — members only".to_string()),
                config: WorkspaceConfig {
                    public: false,
                    ..Default::default()
                },
                tags: vec!["test".to_string(), "private".to_string()],
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    // Admin for private workspace
    let private_admin = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "private-admin@example.com".to_string(),
                name: "Private Admin".to_string(),
                description: Some("Admin of the private test workspace".to_string()),
                ..Default::default()
            },
        )
        .await?;

    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: private_admin.id,
                workspace_id: private_ws.id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password("adminpass")?),
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

    // Set owner of private workspace
    svc_reg
        .workspace
        .update(
            ctx,
            WorkspaceUpdateParams {
                id: Some(private_ws.id),
                owner: Some(private_admin.id),
                ..Default::default()
            },
        )
        .await?;

    // =========================================================================
    // EXTRA USER: Cross-workspace member
    // =========================================================================
    let member = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: "member@example.com".to_string(),
                name: "Workspace Member".to_string(),
                description: Some("Cross-workspace member account".to_string()),
                ..Default::default()
            },
        )
        .await?;

    // Credential for member (attached to test workspace)
    svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id: member.id,
                workspace_id: test_ws.id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password("memberpass")?),
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

    // =========================================================================
    // MEMBERSHIPS — owners as admins, member as viewer across workspaces
    // =========================================================================
    // NOTE(workspace-scope): scoped — seed test roles scoped per workspace.
    let mut store_ctx: StoreCtx = (&*ctx).into();

    store_ctx.set_workspace_scope(Some(test_ws.id));
    let test_admin_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_admin_role,
            DbId(test_ws.id),
        )
        .await?
        .expect("WorkspaceAdmin role not found in test workspace");

    store_ctx.set_workspace_scope(Some(public_ws.id));
    let public_admin_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_admin_role,
            DbId(public_ws.id),
        )
        .await?
        .expect("WorkspaceAdmin role not found in public test workspace");

    store_ctx.set_workspace_scope(Some(private_ws.id));
    let private_admin_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_admin_role,
            DbId(private_ws.id),
        )
        .await?
        .expect("WorkspaceAdmin role not found in private test workspace");

    store_ctx.set_workspace_scope(Some(test_ws.id));
    let test_viewer_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_viewer_role,
            DbId(test_ws.id),
        )
        .await?
        .expect("WorkspaceViewer role not found in test workspace");

    store_ctx.set_workspace_scope(Some(public_ws.id));
    let public_viewer_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_viewer_role,
            DbId(public_ws.id),
        )
        .await?
        .expect("WorkspaceViewer role not found in public test workspace");

    store_ctx.set_workspace_scope(Some(private_ws.id));
    let private_viewer_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_viewer_role,
            DbId(private_ws.id),
        )
        .await?
        .expect("WorkspaceViewer role not found in private test workspace");

    let memberships: Vec<MembershipCreateParams> = vec![
        // test_account as admin of test ws
        MembershipCreateParams {
            account_id: Some(test_account.id),
            email: None,
            workspace_id: test_ws.id,
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            role_ids: vec![test_admin_role.id.into()],
            policy_ids: vec![],
            tags: vec!["test".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // public_admin as admin of public ws
        MembershipCreateParams {
            account_id: Some(public_admin.id),
            email: None,
            workspace_id: public_ws.id,
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            role_ids: vec![public_admin_role.id.into()],
            policy_ids: vec![],
            tags: vec!["test".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // private_admin as admin of private ws
        MembershipCreateParams {
            account_id: Some(private_admin.id),
            email: None,
            workspace_id: private_ws.id,
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            role_ids: vec![private_admin_role.id.into()],
            policy_ids: vec![],
            tags: vec!["test".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // member as viewer of test ws
        MembershipCreateParams {
            account_id: Some(member.id),
            email: None,
            workspace_id: test_ws.id,
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            role_ids: vec![test_viewer_role.id.into()],
            policy_ids: vec![],
            tags: vec!["test".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // member as viewer of public ws
        MembershipCreateParams {
            account_id: Some(member.id),
            email: None,
            workspace_id: public_ws.id,
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            role_ids: vec![public_viewer_role.id.into()],
            policy_ids: vec![],
            tags: vec!["test".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
        // member as viewer of private ws
        MembershipCreateParams {
            account_id: Some(member.id),
            email: None,
            workspace_id: private_ws.id,
            scope: MembershipScope::Workspace,
            status: Some(MembershipStatus::Active),
            profile_id: None,
            project_id: None,
            role_ids: vec![private_viewer_role.id.into()],
            policy_ids: vec![],
            tags: vec!["test".to_string()],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        },
    ];

    for params in memberships {
        ctx.escalate_perms(&["membership:create"])?;
        svc_reg.membership.create(ctx, params).await?;
    }

    tracing::info!(
        "Seed test data created: 3 workspaces (test, public-test, private-test), \
         4 accounts (test, public-admin, private-admin, member), \
         4 credentials, 6 memberships"
    );
    Ok(())
}
