use std::collections::HashSet;

use serde_json::{from_value, json};
use serial_test::serial;
use uuid::Uuid;

use oxideauth::{
    dev::init::init_test,
    store::{
        ctx::StoreCtx,
        entities::{
            account::{AccountFilter, AccountForCreate, AccountIden, AccountRow},
            id::DbId,
            membership::{
                MembershipForCreate, MembershipMeta, MembershipScope, MembershipStatus,
            },
            permission::{PermissionForCreate, PermissionRow},
            project::{ProjectConfig, ProjectForCreate, ProjectMeta},
            role::{RoleFilter, RoleForCreate, RoleIden, RoleRow},
            workspace::WorkspaceForCreate,
        },
        error::StoreResult,
        queries::list::{list_containing_many, list_in_namespace_by_join_table},
        queries::meta::{ListContainingManyQueryMeta, ListInNamespaceQueryMeta},
        stores::{account::AccountStore, permission::PermissionStore, role::RoleStore},
        traits::{
            crud::{Create, Get},
            join::LinkManyToMany,
            meta::MutateStore,
        },
    },
};

// -----------------------------------------------------------------------------
// list_in_namespace_by_join_table
// -----------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_pass() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    let mut acc_ids: HashSet<Uuid> = HashSet::new();
    for i in 0..3 {
        let mut a = AccountForCreate::default();
        a.email = format!("ns-list-pass-{i}@example.com");
        a.name = "NS_LIST_PASS".to_string();
        let acc = app.sm.account.create(&ctx, a).await?;
        acc_ids.insert(acc.id.into());

        let membership = MembershipForCreate {
            account_id: acc.id.into(),
            workspace_id: ws_id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            profile_id: None,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        };
        app.sm.membership.create(&ctx, membership).await?;
    }

    // Namespace falls back to ctx.ws_id when no workspace_scope is set.
    let ns_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&ns_ctx, None, None, None)
        .await?;

    // Assert
    assert_eq!(rows.len(), 3, "all accounts linked via membership must be listed");
    let returned: HashSet<Uuid> = rows.iter().map(|r| r.id.into()).collect();
    assert_eq!(returned, acc_ids);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_scoped() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    let mut acc_ids: HashSet<Uuid> = HashSet::new();
    for i in 0..2 {
        let mut a = AccountForCreate::default();
        a.email = format!("ns-list-scoped-{i}@example.com");
        a.name = "NS_LIST_SCOPED".to_string();
        let acc = app.sm.account.create(&ctx, a).await?;
        acc_ids.insert(acc.id.into());

        let membership = MembershipForCreate {
            account_id: acc.id.into(),
            workspace_id: ws_id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            profile_id: None,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        };
        app.sm.membership.create(&ctx, membership).await?;
    }

    let mut scoped_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);
    scoped_ctx.set_workspace_scope(Some(ws_id));

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&scoped_ctx, None, None, None)
        .await?;

    // Assert
    assert_eq!(rows.len(), 2);
    let returned: HashSet<Uuid> = rows.iter().map(|r| r.id.into()).collect();
    assert_eq!(returned, acc_ids);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_with_filter() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    // 2 accounts matching, 1 not matching
    for i in 0..3 {
        let mut a = AccountForCreate::default();
        a.email = format!("ns-list-filter-{i}@example.com");
        a.name = if i < 2 {
            "NS_LIST_FILTER_MATCH".to_string()
        } else {
            "NS_LIST_FILTER_OTHER".to_string()
        };
        let acc = app.sm.account.create(&ctx, a).await?;

        let membership = MembershipForCreate {
            account_id: acc.id.into(),
            workspace_id: ws_id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            profile_id: None,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        };
        app.sm.membership.create(&ctx, membership).await?;
    }

    let ns_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);
    let filter: AccountFilter = from_value(json!({"name": {"$contains": "NS_LIST_FILTER_MATCH"}})).unwrap();

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&ns_ctx, None, Some(filter), None)
        .await?;

    // Assert
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r.name == "NS_LIST_FILTER_MATCH"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_with_tags() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    // 2 accounts tagged "ns-tag", 1 tagged differently
    for i in 0..3 {
        let mut a = AccountForCreate::default();
        a.email = format!("ns-list-tags-{i}@example.com");
        a.name = "NS_LIST_TAGS".to_string();
        a.tags = if i < 2 {
            vec!["ns-tag".to_string(), "common".to_string()]
        } else {
            vec!["other-tag".to_string()]
        };
        let acc = app.sm.account.create(&ctx, a).await?;

        let membership = MembershipForCreate {
            account_id: acc.id.into(),
            workspace_id: ws_id,
            scope: MembershipScope::Workspace,
            status: MembershipStatus::Active,
            project_id: None,
            profile_id: None,
            tags: vec![],
            meta: MembershipMeta {
                schema_version: "1".to_string(),
            },
        };
        app.sm.membership.create(&ctx, membership).await?;
    }

    let ns_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&ns_ctx, Some(vec!["ns-tag".to_string()]), None, None)
        .await?;

    // Assert
    assert_eq!(rows.len(), 2, "only accounts containing the 'ns-tag' tag must be listed");
    assert!(rows.iter().all(|r| r.tags.contains(&"ns-tag".to_string())));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_distinct() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    // A project in the same workspace (FK prerequisite for a project-scoped membership).
    let project = ProjectForCreate {
        workspace_id: ws_id,
        name: "ns-list-distinct-project".to_string(),
        code: Some("NS_LIST_DISTINCT".to_string()),
        description: None,
        owner: Uuid::nil().into(),
        config: ProjectConfig::default(),
        tags: vec![],
        meta: ProjectMeta::default(),
    };
    let proj = app.sm.project.create(&ctx, project).await?;
    let proj_id: Uuid = proj.id.into();

    // One account linked to the workspace through TWO membership rows:
    // 1) workspace-scoped, 2) project-scoped.
    let mut a = AccountForCreate::default();
    a.email = "ns-list-distinct@example.com".to_string();
    a.name = "NS_LIST_DISTINCT".to_string();
    let acc = app.sm.account.create(&ctx, a).await?;
    let acc_id: Uuid = acc.id.into();

    let ws_membership = MembershipForCreate {
        account_id: acc_id,
        workspace_id: ws_id,
        scope: MembershipScope::Workspace,
        status: MembershipStatus::Active,
        project_id: None,
        profile_id: None,
        tags: vec![],
        meta: MembershipMeta {
            schema_version: "1".to_string(),
        },
    };
    app.sm.membership.create(&ctx, ws_membership).await?;

    let proj_membership = MembershipForCreate {
        account_id: acc_id,
        workspace_id: ws_id,
        scope: MembershipScope::Project,
        status: MembershipStatus::Active,
        project_id: Some(proj_id),
        profile_id: None,
        tags: vec![],
        meta: MembershipMeta {
            schema_version: "1".to_string(),
        },
    };
    app.sm.membership.create(&ctx, proj_membership).await?;

    let ns_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&ns_ctx, None, None, None)
        .await?;

    // Assert: DISTINCT must collapse the duplicate account row
    assert_eq!(
        rows.len(),
        1,
        "DISTINCT must prevent duplicate rows when an account has several memberships in the namespace"
    );
    assert_eq!(rows[0].id, acc.id);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_empty() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // Workspace with no memberships at all
    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    let ns_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&ns_ctx, None, None, None)
        .await?;

    // Assert
    assert!(rows.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_other_namespace() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws_a = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_b = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_a_id: Uuid = ws_a.id.into();
    let ws_b_id: Uuid = ws_b.id.into();
    assert_ne!(ws_a_id, ws_b_id);

    // Account linked only to ws_b
    let mut a = AccountForCreate::default();
    a.email = "ns-list-other@example.com".to_string();
    a.name = "NS_LIST_OTHER".to_string();
    let acc = app.sm.account.create(&ctx, a).await?;

    let membership = MembershipForCreate {
        account_id: acc.id.into(),
        workspace_id: ws_b_id,
        scope: MembershipScope::Workspace,
        status: MembershipStatus::Active,
        project_id: None,
        profile_id: None,
        tags: vec![],
        meta: MembershipMeta {
            schema_version: "1".to_string(),
        },
    };
    app.sm.membership.create(&ctx, membership).await?;

    let ns_ctx_a = StoreCtx::new(Uuid::new_v4(), ws_a_id);

    // Act
    let rows: Vec<AccountRow> = acc_store
        .list_in_namespace_by_join_table(&ns_ctx_a, None, None, None)
        .await?;

    // Assert: the account is in ws_b, not ws_a
    assert!(rows.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_in_namespace_by_join_table_query_fn_direct() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    let mut a = AccountForCreate::default();
    a.email = "ns-list-direct@example.com".to_string();
    a.name = "NS_LIST_DIRECT".to_string();
    let acc = app.sm.account.create(&ctx, a).await?;

    let membership = MembershipForCreate {
        account_id: acc.id.into(),
        workspace_id: ws_id,
        scope: MembershipScope::Workspace,
        status: MembershipStatus::Active,
        project_id: None,
        profile_id: None,
        tags: vec![],
        meta: MembershipMeta {
            schema_version: "1".to_string(),
        },
    };
    app.sm.membership.create(&ctx, membership).await?;

    let ns_ctx = StoreCtx::new(Uuid::new_v4(), ws_id);

    let meta = ListInNamespaceQueryMeta {
        table: AccountIden::Table,
        pk: AccountIden::Id,
        join_table: AccountIden::Membership,
        join_fk: AccountIden::AccountId,
        has_audit: true,
    };

    // Act
    let rows: Vec<AccountRow> =
        list_in_namespace_by_join_table(&ns_ctx, &dbx, None, None::<AccountFilter>, None, &meta).await?;

    // Assert
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, acc.id);

    Ok(())
}

// -----------------------------------------------------------------------------
// list_containing_many
// -----------------------------------------------------------------------------

/// Creates a role and a permission in `ws_id` and links them via `role_store`.
async fn seed_role_with_permissions(
    ctx: &StoreCtx,
    role_store: &RoleStore<oxideauth::store::dbx::PgDbx>,
    perm_store: &PermissionStore<oxideauth::store::dbx::PgDbx>,
    ws_id: Uuid,
    role_name: &str,
    perm_names: &[&str],
) -> StoreResult<(RoleRow, Vec<PermissionRow>)> {
    let role = role_store
        .create(
            ctx,
            RoleForCreate {
                workspace_id: ws_id,
                name: role_name.to_string(),
                ..Default::default()
            },
        )
        .await?;

    let mut perms = vec![];
    let mut perm_ids = vec![];
    for name in perm_names {
        let perm = perm_store
            .create(
                ctx,
                PermissionForCreate {
                    workspace_id: ws_id,
                    name: name.to_string(),
                    ..Default::default()
                },
            )
            .await?;
        perm_ids.push(perm.id);
        perms.push(perm);
    }

    role_store
        .set_many_to_many_links(ctx, &role.id, perm_ids)
        .await?;

    Ok((role, perms))
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_pass() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    // Create the permissions once and share them between the two roles
    // (the permission table enforces a (workspace_id, name) unique constraint).
    let p1 = perm_store
        .create(
            &ctx,
            PermissionForCreate {
                workspace_id: ws_id,
                name: "LCM_PERM_1".to_string(),
                ..Default::default()
            },
        )
        .await?;
    let p2 = perm_store
        .create(
            &ctx,
            PermissionForCreate {
                workspace_id: ws_id,
                name: "LCM_PERM_2".to_string(),
                ..Default::default()
            },
        )
        .await?;

    // role_a -> [p1, p2], role_b -> [p1]
    let role_a = role_store
        .create(
            &ctx,
            RoleForCreate {
                workspace_id: ws_id,
                name: "LCM_ROLE_A".to_string(),
                ..Default::default()
            },
        )
        .await?;
    let role_b = role_store
        .create(
            &ctx,
            RoleForCreate {
                workspace_id: ws_id,
                name: "LCM_ROLE_B".to_string(),
                ..Default::default()
            },
        )
        .await?;
    role_store
        .set_many_to_many_links(&ctx, &role_a.id, vec![p1.id, p2.id])
        .await?;
    role_store
        .set_many_to_many_links(&ctx, &role_b.id, vec![p1.id])
        .await?;

    // Act: roles containing ALL of [p1, p2]
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(&ctx, vec![p1.id, p2.id], None, None, None)
        .await?;

    // Assert
    assert_eq!(rows.len(), 1, "only role_a contains both permissions");
    assert_eq!(rows[0].id, role_a.id);

    // Act: roles containing [p1]
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(&ctx, vec![p1.id], None, None, None)
        .await?;

    // Assert
    assert_eq!(rows.len(), 2, "both roles contain p1");
    let ids: HashSet<DbId> = rows.iter().map(|r| r.id).collect();
    assert!(ids.contains(&role_a.id));
    assert!(ids.contains(&role_b.id));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_empty_ids() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    seed_role_with_permissions(&ctx, &role_store, &perm_store, ws_id, "LCM_EMPTY", &["LCM_EMPTY_P1"])
        .await?;

    // Act: an empty "must contain all" set matches nothing
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(&ctx, vec![], None, None, None)
        .await?;

    // Assert
    assert!(rows.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_no_match() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    seed_role_with_permissions(&ctx, &role_store, &perm_store, ws_id, "LCM_NO_MATCH", &["LCM_NO_MATCH_P1"])
        .await?;

    // Act: request an unlinked permission id
    let random_id: DbId = Uuid::new_v4().into();
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(&ctx, vec![random_id], None, None, None)
        .await?;

    // Assert
    assert!(rows.is_empty(), "no role should contain the unlinked permission");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_partial_match_excluded() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    // role has only p1
    let (role, perms) = seed_role_with_permissions(
        &ctx,
        &role_store,
        &perm_store,
        ws_id,
        "LCM_PARTIAL",
        &["LCM_PARTIAL_P1"],
    )
    .await?;
    let p1 = perms[0].id;
    let p2 = perm_store
        .create(
            &ctx,
            PermissionForCreate {
                workspace_id: ws_id,
                name: "LCM_PARTIAL_P2".to_string(),
                ..Default::default()
            },
        )
        .await?;

    // Act: request [p1, p2] but role only has p1
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(&ctx, vec![p1, p2.id], None, None, None)
        .await?;

    // Assert
    assert!(rows.is_empty(), "a role missing one requested permission must be excluded");
    assert!(rows.is_empty() || rows.iter().all(|r| r.id != role.id));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_scoped() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws_a = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_b = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_a_id: Uuid = ws_a.id.into();
    let ws_b_id: Uuid = ws_b.id.into();
    assert_ne!(ws_a_id, ws_b_id);

    // role_a (ws_a) -> [perm_a], role_b (ws_b) -> [perm_b]
    let (role_a, perms_a) = seed_role_with_permissions(
        &ctx,
        &role_store,
        &perm_store,
        ws_a_id,
        "LCM_WS_A_ROLE",
        &["LCM_WS_A_PERM"],
    )
    .await?;
    let (role_b, _) = seed_role_with_permissions(
        &ctx,
        &role_store,
        &perm_store,
        ws_b_id,
        "LCM_WS_B_ROLE",
        &["LCM_WS_B_PERM"],
    )
    .await?;
    let perm_a = perms_a[0].id;
    assert_ne!(role_a.id, role_b.id);

    let mut scoped_ctx = StoreCtx::new(Uuid::new_v4(), ws_a_id);
    scoped_ctx.set_workspace_scope(Some(ws_a_id));

    // Act
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(&scoped_ctx, vec![perm_a], None, None, None)
        .await?;

    // Assert: only the ws_a role is returned
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, role_a.id);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_with_tags() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    // role tagged "lcm-tag" contains p1; untagged role also contains p1
    let p1 = perm_store
        .create(
            &ctx,
            PermissionForCreate {
                workspace_id: ws_id,
                name: "LCM_TAGS_P1".to_string(),
                ..Default::default()
            },
        )
        .await?;

    let role_tagged = role_store
        .create(
            &ctx,
            RoleForCreate {
                workspace_id: ws_id,
                name: "LCM_TAGS_ROLE_A".to_string(),
                tags: vec!["lcm-tag".to_string()],
                ..Default::default()
            },
        )
        .await?;
    role_store
        .set_many_to_many_links(&ctx, &role_tagged.id, vec![p1.id])
        .await?;

    let role_plain = role_store
        .create(
            &ctx,
            RoleForCreate {
                workspace_id: ws_id,
                name: "LCM_TAGS_ROLE_B".to_string(),
                ..Default::default()
            },
        )
        .await?;
    role_store
        .set_many_to_many_links(&ctx, &role_plain.id, vec![p1.id])
        .await?;

    // Act
    let rows: Vec<RoleRow> = role_store
        .list_containing_permissions(
            &ctx,
            vec![p1.id],
            Some(vec!["lcm-tag".to_string()]),
            None,
            None,
        )
        .await?;

    // Assert: only the tagged role matches the tag containment
    assert_eq!(rows.len(), 1, "tags must narrow the result set");
    assert_eq!(rows[0].id, role_tagged.id);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_containing_many_query_fn_direct() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let role_store = RoleStore::new(dbx.clone());
    let perm_store = PermissionStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let ws = app
        .sm
        .workspace
        .create(&ctx, WorkspaceForCreate::default())
        .await?;
    let ws_id: Uuid = ws.id.into();

    let (role, perms) = seed_role_with_permissions(
        &ctx,
        &role_store,
        &perm_store,
        ws_id,
        "LCM_DIRECT_ROLE",
        &["LCM_DIRECT_PERM"],
    )
    .await?;
    let p1 = perms[0].id;

    let meta = ListContainingManyQueryMeta {
        table: RoleIden::Table,
        pk: RoleIden::Id,
        join_table: RoleIden::RolePermission,
        join_fk: RoleIden::RoleId,
        join_many_fk: RoleIden::PermissionId,
        has_audit: true,
    };

    // Act
    let rows: Vec<RoleRow> =
        list_containing_many(&ctx, &dbx, vec![p1], None, None::<RoleFilter>, None, &meta).await?;

    // Assert
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, role.id);

    Ok(())
}
