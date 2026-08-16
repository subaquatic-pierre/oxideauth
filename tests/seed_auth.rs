use anyhow::Result;
use serial_test::serial;

use oxideauth::{
    core::{
        ctx::CoreCtx,
        models::workspace::WorkspaceDescribeParams,
        traits::service::CoreModelDescribeService,
    },
    dev::init::init_test,
    store::{
        ctx::StoreCtx,
        entities::id::DbId,
        join::GetManyToMany,
        stores::workspace::SYSTEM_CONST,
    },
};

/// Verifies the seeded `WorkspaceAdmin` role carries the single `*:*` wildcard
/// permission (FR-020) in the system namespace.
#[tokio::test]
#[serial]
async fn test_seeded_admin_role_has_global_wildcard() -> Result<()> {
    let app = init_test().await;
    let svc_reg = app.svc_reg.clone();

    let mut ctx = CoreCtx::bootstrap()?;

    let system_ws = svc_reg
        .workspace
        .describe(
            &mut ctx,
            WorkspaceDescribeParams {
                slug: Some(SYSTEM_CONST.system_ws_slug.to_string()),
                ..Default::default()
            },
        )
        .await?;

    let mut store_ctx: StoreCtx = (&ctx).into();
    store_ctx.set_workspace_scope(Some(system_ws.id));

    let admin_role = svc_reg
        .sm
        .role
        .get_by_name_opt(
            &store_ctx,
            SYSTEM_CONST.workspace_admin_role,
            DbId(system_ws.id),
        )
        .await?
        .expect("WorkspaceAdmin role not found in system workspace");

    let role_with_perms = svc_reg
        .sm
        .role
        .get_many_to_many(&store_ctx, &admin_role.id)
        .await?;

    let names: Vec<String> = role_with_perms
        .permissions
        .iter()
        .map(|p| p.name.clone())
        .collect();

    assert_eq!(
        names,
        vec!["*:*".to_string()],
        "system WorkspaceAdmin role must carry only the *:* wildcard permission"
    );

    Ok(())
}
