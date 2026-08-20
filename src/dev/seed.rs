use uuid::Uuid;

use crate::cache::entities::workspace::WorkspaceCache;
use crate::{
    app::AppState,
    cache::{redis::RedisChx, traits::CacheExecutor},
    config::Config,
    core::{
        ctx::CoreCtx,
        error::{CoreError, CoreResult},
        models::{
            account::AccountCreateParams,
            credential::{CredentialCreateParams, CredentialMeta},
            membership::{MembershipCreateParams, MembershipMeta},
            profile::ProfileCreateParams,
            workspace::{WorkspaceConfig, WorkspaceCreateParams, WorkspaceUpdateParams},
        },
        services::registry::ServiceRegistry,
        traits::service::{CoreModelCreateService, CoreModelUpdateService},
    },
    store::{
        ctx::StoreCtx,
        dbx::PgDbx,
        entities::{
            credential::{CredentialConfig, CredentialKind, CredentialProvider, CredentialStatus},
            id::DbId,
            membership::{MembershipScope, MembershipStatus},
            workspace::WorkspaceMeta,
        },
        stores::workspace::SYSTEM_CONST,
        traits::dbx::DbExecutor,
    },
    utils::crypt::hash_password,
};

// --- Small id-returning helpers -------------------------------------------------

async fn create_workspace<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    name: &str,
    slug: &str,
    description: &str,
    tags: Vec<&str>,
    config: WorkspaceConfig,
) -> CoreResult<Uuid> {
    let ws = svc_reg
        .workspace
        .create(
            ctx,
            WorkspaceCreateParams {
                name: name.to_string(),
                slug: slug.to_string(),
                owner: None,
                description: Some(description.to_string()),
                config,
                tags: tags.into_iter().map(str::to_string).collect(),
                meta: WorkspaceMeta::default(),
            },
        )
        .await?;

    Ok(ws.id)
}

async fn create_account<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    email: &str,
    name: &str,
    description: &str,
    tags: Vec<&str>,
    enabled: bool,
    verified: bool,
) -> CoreResult<Uuid> {
    let acc = svc_reg
        .account
        .create(
            ctx,
            AccountCreateParams {
                email: email.to_string(),
                name: name.to_string(),
                description: Some(description.to_string()),
                tags: Some(tags.into_iter().map(str::to_string).collect()),
                enabled,
                verified,
                ..Default::default()
            },
        )
        .await?;

    Ok(acc.id)
}

async fn workspace_role_id<D: DbExecutor, C: CacheExecutor>(
    ctx: &CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    ws_id: Uuid,
    role_name: &str,
) -> CoreResult<Uuid> {
    let mut store_ctx: StoreCtx = ctx.into();
    store_ctx.set_workspace_scope(Some(ws_id));

    let role = svc_reg
        .sm
        .role
        .get_by_name_opt(&store_ctx, role_name, DbId(ws_id))
        .await?
        .ok_or_else(|| CoreError::NotFound(format!("{role_name} role not found in {ws_id}")))?;

    Ok(role.id.into())
}

async fn create_membership<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    account_id: Uuid,
    email: &str,
    ws_id: Uuid,
    role_id: Uuid,
    tags: Vec<&str>,
) -> CoreResult<Uuid> {
    ctx.set_scoped_ws(WorkspaceCache::new_keyed(ws_id));
    ctx.escalate_perms(&["membership:create"])?;
    ctx.escalate_perms(&["profile:create"])?;

    let profile = svc_reg
        .profile
        .create(
            ctx,
            ProfileCreateParams {
                account_id,
                workspace_id: Some(ws_id),
                email: email.to_string(),
                name: email.to_string(),
                description: None,
                display_name: None,
                job_title: None,
                timezone: None,
                avatar_url: None,
                tags: vec![],
                meta: Default::default(),
            },
        )
        .await?;

    let membership = svc_reg
        .membership
        .create(
            ctx,
            MembershipCreateParams {
                account_id: account_id,
                workspace_id: Some(ws_id),
                profile_id: profile.id,
                scope: MembershipScope::Workspace,
                status: MembershipStatus::Active,
                project_id: None,
                role_ids: vec![role_id],
                policy_ids: vec![],
                tags: tags.into_iter().map(str::to_string).collect(),
                meta: MembershipMeta {
                    schema_version: "1".to_string(),
                },
            },
        )
        .await?;

    Ok(membership.id)
}

async fn set_workspace_owner<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    ws_id: Uuid,
    account_id: Uuid,
) -> CoreResult<()> {
    svc_reg
        .workspace
        .update(
            ctx,
            WorkspaceUpdateParams {
                id: Some(ws_id),
                owner: Some(account_id),
                ..Default::default()
            },
        )
        .await?;

    Ok(())
}

async fn create_password_credential<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    account_id: Uuid,
    ws_id: Uuid,
    membership_id: Uuid,
    secret: &str,
    tags: Vec<&str>,
) -> CoreResult<Uuid> {
    let cred = svc_reg
        .credential
        .create(
            ctx,
            CredentialCreateParams {
                account_id,
                workspace_id: Some(ws_id),
                membership_id,
                kind: CredentialKind::Password,
                provider: CredentialProvider::Local,
                status: CredentialStatus::Active,
                secret: Some(hash_password(secret)?),
                expires_at: None,
                provider_id: None,
                email: None,
                config: CredentialConfig::default(),
                last_used_at: None,
                tags: tags.into_iter().map(str::to_string).collect(),
                meta: CredentialMeta::default(),
            },
        )
        .await?;

    Ok(cred.id)
}

// --- System / default pipeline --------------------------------------------------

pub async fn seed_all<D: DbExecutor, C: CacheExecutor>(
    app: &AppState<PgDbx, RedisChx>,
) -> CoreResult<()> {
    let svc_reg = app.svc_reg.clone();
    let ctx_factory = app.svc_reg.ctx_factory.clone();
    let mut ctx = CoreCtx::bootstrap()?;

    let (system_ws, default_ws) = seed_workspaces(&mut ctx, &svc_reg).await?;
    let (system_acc, owner_acc) = seed_users(&mut ctx, &svc_reg, &app.config).await?;
    let (owner_system_mem, owner_default_mem) = seed_memberships(
        &mut ctx,
        &svc_reg,
        &app.config,
        system_ws,
        default_ws,
        system_acc,
        owner_acc,
    )
    .await?;
    seed_owners_and_credentials(
        &mut ctx,
        &svc_reg,
        &app.config,
        system_ws,
        default_ws,
        system_acc,
        owner_acc,
        owner_system_mem,
        owner_default_mem,
    )
    .await?;

    ctx_factory.init_from_seed(&svc_reg.sm).await?;
    Ok(())
}

async fn seed_workspaces<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
) -> CoreResult<(Uuid, Uuid)> {
    let system_ws = create_workspace(
        ctx,
        svc_reg,
        "System Workspace",
        SYSTEM_CONST.system_ws_slug,
        "System workspace for root operations",
        vec!["system"],
        WorkspaceConfig::default(),
    )
    .await?;

    let default_ws = create_workspace(
        ctx,
        svc_reg,
        "Default Workspace",
        SYSTEM_CONST.default_ws_slug,
        "Default workspace for general access",
        vec!["system"],
        WorkspaceConfig::default(),
    )
    .await?;

    Ok((system_ws, default_ws))
}

async fn seed_users<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    config: &Config,
) -> CoreResult<(Uuid, Uuid)> {
    let system_acc = create_account(
        ctx,
        svc_reg,
        SYSTEM_CONST.system_acc_email,
        SYSTEM_CONST.system_acc_name,
        "System account for internal operations",
        vec!["system"],
        true,
        true,
    )
    .await?;

    let owner_acc = create_account(
        ctx,
        svc_reg,
        &config.owner_email,
        &config.owner_name,
        "Workspace owner account",
        vec!["system"],
        true,
        true,
    )
    .await?;

    tracing::info!("Seed users created: system, owner");
    Ok((system_acc, owner_acc))
}

async fn seed_memberships<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    config: &Config,
    system_ws: Uuid,
    default_ws: Uuid,
    system_acc: Uuid,
    owner_acc: Uuid,
) -> CoreResult<(Uuid, Uuid)> {
    let system_admin =
        workspace_role_id(ctx, svc_reg, system_ws, SYSTEM_CONST.workspace_admin_role).await?;
    let default_admin =
        workspace_role_id(ctx, svc_reg, default_ws, SYSTEM_CONST.workspace_admin_role).await?;

    // system account → system workspace admin
    create_membership(
        ctx,
        svc_reg,
        system_acc,
        SYSTEM_CONST.system_acc_email,
        system_ws,
        system_admin,
        vec!["system"],
    )
    .await?;

    // owner → system workspace admin
    let owner_system_mem = create_membership(
        ctx,
        svc_reg,
        owner_acc,
        &config.owner_email,
        system_ws,
        system_admin,
        vec!["system"],
    )
    .await?;

    // owner → default workspace admin
    let owner_default_mem = create_membership(
        ctx,
        svc_reg,
        owner_acc,
        &config.owner_email,
        default_ws,
        default_admin,
        vec!["system"],
    )
    .await?;

    tracing::info!("Seed memberships created");
    Ok((owner_system_mem, owner_default_mem))
}

async fn seed_owners_and_credentials<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
    config: &Config,
    system_ws: Uuid,
    default_ws: Uuid,
    system_acc: Uuid,
    owner_acc: Uuid,
    owner_system_mem: Uuid,
    owner_default_mem: Uuid,
) -> CoreResult<()> {
    set_workspace_owner(ctx, svc_reg, system_ws, system_acc).await?;
    set_workspace_owner(ctx, svc_reg, default_ws, owner_acc).await?;

    create_password_credential(
        ctx,
        svc_reg,
        owner_acc,
        system_ws,
        owner_system_mem,
        &config.owner_password,
        vec!["system"],
    )
    .await?;

    create_password_credential(
        ctx,
        svc_reg,
        owner_acc,
        default_ws,
        owner_default_mem,
        &config.owner_password,
        vec!["system"],
    )
    .await?;

    tracing::info!("Seed workspace owners and owner credentials set");
    Ok(())
}

// --- Test data ------------------------------------------------------------------

/// Seeds test data (workspace, accounts, memberships, owners, credentials).
/// This is optional — callers can skip it for production-like environments.
pub async fn seed_test_data<D: DbExecutor, C: CacheExecutor>(
    ctx: &mut CoreCtx,
    svc_reg: &ServiceRegistry<D, C>,
) -> CoreResult<()> {
    // Workspaces
    let test_ws = create_workspace(
        ctx,
        svc_reg,
        "Test Workspace",
        "test",
        "Private test workspace for development",
        vec!["test"],
        WorkspaceConfig::default(),
    )
    .await?;

    let public_ws = create_workspace(
        ctx,
        svc_reg,
        "Public Test Workspace",
        "public-test",
        "Public test workspace — anyone can browse",
        vec!["test", "public"],
        WorkspaceConfig {
            public: true,
            ..Default::default()
        },
    )
    .await?;

    let private_ws = create_workspace(
        ctx,
        svc_reg,
        "Private Test Workspace",
        "private-test",
        "Private test workspace — members only",
        vec!["test", "private"],
        WorkspaceConfig {
            public: false,
            ..Default::default()
        },
    )
    .await?;

    // Accounts
    let test_account = create_account(
        ctx,
        svc_reg,
        "test@example.com",
        "Test Account",
        "General test account",
        vec![],
        false,
        false,
    )
    .await?;

    let public_admin = create_account(
        ctx,
        svc_reg,
        "public-admin@example.com",
        "Public Admin",
        "Admin of the public test workspace",
        vec![],
        false,
        false,
    )
    .await?;

    let private_admin = create_account(
        ctx,
        svc_reg,
        "private-admin@example.com",
        "Private Admin",
        "Admin of the private test workspace",
        vec![],
        false,
        false,
    )
    .await?;

    let member = create_account(
        ctx,
        svc_reg,
        "member@example.com",
        "Workspace Member",
        "Cross-workspace member account",
        vec![],
        false,
        false,
    )
    .await?;

    // Roles
    let test_admin_role =
        workspace_role_id(ctx, svc_reg, test_ws, SYSTEM_CONST.workspace_admin_role).await?;
    let public_admin_role =
        workspace_role_id(ctx, svc_reg, public_ws, SYSTEM_CONST.workspace_admin_role).await?;
    let private_admin_role =
        workspace_role_id(ctx, svc_reg, private_ws, SYSTEM_CONST.workspace_admin_role).await?;
    let test_viewer_role =
        workspace_role_id(ctx, svc_reg, test_ws, SYSTEM_CONST.workspace_viewer_role).await?;
    let public_viewer_role =
        workspace_role_id(ctx, svc_reg, public_ws, SYSTEM_CONST.workspace_viewer_role).await?;
    let private_viewer_role =
        workspace_role_id(ctx, svc_reg, private_ws, SYSTEM_CONST.workspace_viewer_role).await?;

    // Memberships
    let test_admin_mem = create_membership(
        ctx,
        svc_reg,
        test_account,
        "test@example.com",
        test_ws,
        test_admin_role,
        vec!["test"],
    )
    .await?;
    let public_admin_mem = create_membership(
        ctx,
        svc_reg,
        public_admin,
        "public-admin@example.com",
        public_ws,
        public_admin_role,
        vec!["test"],
    )
    .await?;
    let private_admin_mem = create_membership(
        ctx,
        svc_reg,
        private_admin,
        "private-admin@example.com",
        private_ws,
        private_admin_role,
        vec!["test"],
    )
    .await?;
    let member_test_mem = create_membership(
        ctx,
        svc_reg,
        member,
        "member@example.com",
        test_ws,
        test_viewer_role,
        vec!["test"],
    )
    .await?;
    create_membership(
        ctx,
        svc_reg,
        member,
        "member@example.com",
        public_ws,
        public_viewer_role,
        vec!["test"],
    )
    .await?;
    create_membership(
        ctx,
        svc_reg,
        member,
        "member@example.com",
        private_ws,
        private_viewer_role,
        vec!["test"],
    )
    .await?;

    // Owners
    set_workspace_owner(ctx, svc_reg, test_ws, test_account).await?;
    set_workspace_owner(ctx, svc_reg, public_ws, public_admin).await?;
    set_workspace_owner(ctx, svc_reg, private_ws, private_admin).await?;

    // Credentials (linked to memberships)
    create_password_credential(
        ctx,
        svc_reg,
        test_account,
        test_ws,
        test_admin_mem,
        "testpass",
        vec!["test"],
    )
    .await?;
    create_password_credential(
        ctx,
        svc_reg,
        public_admin,
        public_ws,
        public_admin_mem,
        "adminpass",
        vec!["test"],
    )
    .await?;
    create_password_credential(
        ctx,
        svc_reg,
        private_admin,
        private_ws,
        private_admin_mem,
        "adminpass",
        vec!["test"],
    )
    .await?;
    create_password_credential(
        ctx,
        svc_reg,
        member,
        test_ws,
        member_test_mem,
        "memberpass",
        vec!["test"],
    )
    .await?;

    tracing::info!(
        "Seed test data created: 3 workspaces (test, public-test, private-test), \
         4 accounts (test, public-admin, private-admin, member), \
         4 credentials, 6 memberships"
    );
    Ok(())
}
