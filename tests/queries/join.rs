use anyhow::Result;
use env_logger::filter;
use serde_json::{from_value, json};
use serial_test::serial;
use uuid::Uuid;

use oxideauth::{
    dev::init::init_test,
    store::{
        ctx::StoreCtx,
        entities::{
            account::{
                AccountFilter, AccountForCreate, AccountIden, AccountRow,
                AccountWithCredentials,
            },
            credential::{
                CredentialFilter, CredentialForCreate, CredentialIden, CredentialKind,
            },
            permission::{PermissionFilter, PermissionForCreate, PermissionIden},
            role::{RoleFilter, RoleForCreate, RoleIden, RoleWithPermissions},
            workspace::WorkspaceForCreate,
        },
        error::StoreError,
        queries::crud::create,
        stores::{
            account::AccountStore, credential::CredentialStore, permission::PermissionStore,
            role::RoleStore,
        },
        traits::{
            crud::{Create, Get, List},
            join::{GetManyToMany, GetOneToMany},
            meta::ReadStore,
        },
    },
};

use oxideauth::store::error::StoreResult;
use oxideauth::store::queries::join::*;
use oxideauth::store::queries::meta::{ManyToManyQueryMeta, OneToManyQueryMeta};

#[tokio::test]
#[serial]
async fn test_get_joined() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = CredentialStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // FK prerequisites: credentials reference a real workspace and account.
    let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
    let ws_id: Uuid = ws.id.into();
    let acct = app.sm.account.create(&ctx, AccountForCreate::default()).await?;
    let acct_id: Uuid = acct.id.into();

    let filter: CredentialFilter = json!({"account_id": acct_id.to_string()}).try_into()?;

    let existing_cred = store.list(&ctx, Some(filter), None).await?;

    let c = |i| {
        let mut cred = CredentialForCreate::default();
        cred.account_id = acct_id;
        cred.workspace_id = ws_id;
        cred.provider_id = Some("TEST".to_string());
        // Multiple credentials per (workspace, account) require a non-password kind
        // (the active-password unique index allows only one).
        cred.kind = CredentialKind::ApiKey;

        cred
    };

    let create_count = 2;

    for i in 0..create_count {
        let cred = c(i);
        store.create(&ctx, cred).await?;
    }

    let meta = OneToManyQueryMeta {
        single_table: AccountIden::Table,
        many_table: AccountIden::Credential,
        single_pk: AccountIden::Id,
        many_pk: AccountIden::Id,
        many_fk: AccountIden::AccountId,
        agg_alias: AccountIden::Credentials,
        has_audit: true,
    };

    let res: AccountWithCredentials = get_one_to_many(&ctx, &dbx, &acct_id, &meta).await?;

    let all_cred = res.credentials;

    assert_eq!(all_cred.len(), existing_cred.len() + create_count);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_joined() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = CredentialStore::new(dbx.clone());
    let acc_store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // FK prerequisite: credentials reference a real workspace.
    let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
    let ws_id: Uuid = ws.id.into();

    let c = |i| {
        let mut acc = AccountForCreate::default();
        acc.email = format!("test{i}{i}@LIST_JOIN");
        acc.description = Some("TEST DESCRIPTION".to_string());
        acc
    };

    let mut acc = vec![];

    let acc_count = 2;

    for i in 0..acc_count {
        let n = c(i);

        acc.push(acc_store.create(&ctx, n).await?);
    }

    let c = |acc_id| {
        let mut cred = CredentialForCreate::default();
        cred.account_id = acc_id;
        cred.workspace_id = ws_id;
        // Multiple credentials per (workspace, account) require a non-password kind
        // (the active-password unique index allows only one).
        cred.kind = CredentialKind::ApiKey;
        cred
    };

    let cred_count = 3;

    for ac in acc {
        for i in 0..cred_count {
            let cred = c(ac.id.into());
            store.create(&ctx, cred).await?;
        }
    }

    let meta = OneToManyQueryMeta {
        single_table: AccountIden::Table,
        many_table: AccountIden::Credential,
        single_pk: AccountIden::Id,
        many_pk: AccountIden::Id,
        many_fk: AccountIden::AccountId,
        agg_alias: AccountIden::Credentials,
        has_audit: true,
    };

    // let filter: AccountFilter = json!({"description":"TEST DESCRIPTION"}).try_into()?;
    let filter: AccountFilter = json!({"email": { "$contains" : "LIST_JOIN"}}).try_into()?;

    let res: Vec<AccountWithCredentials> =
        list_one_to_many::<_, AccountFilter, _>(&ctx, &dbx, Some(filter), None, &meta).await?;

    let mut total_count = 0;
    res.iter()
        .for_each(|el| total_count += el.credentials.len());

    assert_eq!(total_count, acc_count * cred_count);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_many_to_many() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let perm_store = PermissionStore::new(dbx.clone());
    let role_store = RoleStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // FK prerequisite: roles and permissions reference a real workspace.
    let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
    let ws_id: Uuid = ws.id.into();

    let c_perm = |i| {
        let mut perm = PermissionForCreate::default();
        perm.workspace_id = ws_id;
        perm.name = format!("PERMISSION_GET_MANY_TEST_i_i_i_i{i}_iii_ii_iii___{i}__{i}");
        perm
    };

    let c_role = |i| {
        let mut role = RoleForCreate::default();
        role.workspace_id = ws_id;
        role.name = format!("ROLE_GET_MANY_TEST_i_i_i_i{i}_iii_ii_iii___{i}__{i}");
        role
    };

    let mut roles = vec![];
    let mut perms = vec![];

    let role_count = 2;
    let perm_count = 2;

    for i in 0..role_count {
        let n = c_role(i);

        roles.push(role_store.create(&ctx, n).await?);
    }

    for i in 0..perm_count {
        let n = c_perm(i);

        perms.push(perm_store.create(&ctx, n).await?);
    }

    let perm_filter: PermissionFilter =
        json!({"name":{"$contains":"GET_MANY_TEST_i_i_i_i"}}).try_into()?;
    let role_filter: RoleFilter =
        json!({"name":{"$contains":"GET_MANY_TEST_i_i_i_i"}}).try_into()?;

    let filtered_perms = perm_store.list(&ctx, Some(perm_filter), None).await?;
    let filtered_roles = role_store.list(&ctx, Some(role_filter), None).await?;

    assert_eq!(role_count, filtered_roles.len());
    assert_eq!(perm_count, filtered_perms.len());

    let role = filtered_roles
        .into_iter()
        .next()
        .take()
        .expect("should be at least on role returns in filter");

    // link all perms

    let mutate_meta = ManyToManyQueryMeta {
        single_table: RoleIden::Table,
        many_table: RoleIden::Permission,
        join_table: RoleIden::RolePermission,
        single_pk: RoleIden::Id,
        many_pk: RoleIden::PermissionPk,
        many_fk: RoleIden::PermissionId,
        join_fk: RoleIden::RoleId,
        agg_alias: RoleIden::Permissions,
        has_audit: true,
    };

    let perm_ids = filtered_perms.iter().map(|el| el.id.clone()).collect();

    let _ =
        set_many_to_many_links(&ctx, &dbx, &role.id.clone(), perm_ids, &mutate_meta).await?;

    let meta = ManyToManyQueryMeta {
        single_table: RoleIden::Table,
        many_table: RoleIden::Permission,
        join_table: RoleIden::RolePermission,
        single_pk: RoleIden::Id,
        many_pk: RoleIden::PermissionPk,
        many_fk: RoleIden::PermissionId,
        join_fk: RoleIden::RoleId,
        agg_alias: RoleIden::Permissions,
        has_audit: true,
    };

    let joined_role: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id.clone(), &meta).await?;

    assert_eq!(perm_count, joined_role.permissions.len());

    let _ = set_many_to_many_links(&ctx, &dbx, &role.id.clone(), vec![], &mutate_meta).await?;

    let joined_role: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id.clone(), &meta).await?;
    assert!(joined_role.permissions.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_list_many_to_many() -> StoreResult<()> {
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let perm_store = PermissionStore::new(dbx.clone());
    let role_store = RoleStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // FK prerequisite: roles and permissions reference a real workspace.
    let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
    let ws_id: Uuid = ws.id.into();

    let c_perm = |i, name: String| {
        let mut perm = PermissionForCreate::default();
        perm.workspace_id = ws_id;
        perm.name = format!("PERMISSION_GET_MANY_TEST_{i}_{name}");
        perm
    };

    let c_role = |i| {
        let mut role = RoleForCreate::default();
        role.workspace_id = ws_id;
        role.name = format!("ROLE_GET_MANY_TEST_{i}");
        role
    };

    let mut roles = vec![];

    let role_count = 2;
    let perm_count = 2;

    for i in 0..role_count {
        let n = c_role(i);

        roles.push(role_store.create(&ctx, n).await?);
    }

    let mutate_meta = ManyToManyQueryMeta {
        single_table: RoleIden::Table,
        many_table: RoleIden::Permission,
        join_table: RoleIden::RolePermission,
        single_pk: RoleIden::Id,
        many_pk: RoleIden::PermissionPk,
        many_fk: RoleIden::PermissionId,
        join_fk: RoleIden::RoleId,
        agg_alias: RoleIden::Permissions,
        has_audit: true,
    };

    for role in roles {
        let mut perms = vec![];

        // create new permissions for each role
        for i in 0..perm_count {
            let n = c_perm(i, role.name.clone());

            perms.push(perm_store.create(&ctx, n).await?);
        }

        // attach perms to role
        let perm_ids = perms.iter().map(|el| el.id.clone()).collect();

        let _ = set_many_to_many_links(&ctx, &dbx, &role.id.clone(), perm_ids, &mutate_meta)
            .await?;
    }

    let meta = ManyToManyQueryMeta {
        single_table: RoleIden::Table,
        many_table: RoleIden::Permission,
        join_table: RoleIden::RolePermission,
        single_pk: RoleIden::Id,
        many_pk: RoleIden::PermissionPk,
        many_fk: RoleIden::PermissionId,
        join_fk: RoleIden::RoleId,
        agg_alias: RoleIden::Permissions,
        has_audit: true,
    };

    let role_filter: RoleFilter = json!({"name":{"$contains":"MANY_TEST"}}).try_into()?;

    let filtered_roles = list_many_to_many::<RoleWithPermissions, RoleFilter, _>(
        &ctx,
        &dbx,
        None,
        Some(role_filter),
        None,
        &meta,
    )
    .await?;

    let mut total_perm_count = 0;

    filtered_roles
        .iter()
        .for_each(|el| total_perm_count += el.permissions.len());

    assert_eq!(role_count * perm_count, total_perm_count);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_attach_detach_many_to_many() -> StoreResult<()> {
    // -- Setup
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let perm_store = PermissionStore::new(dbx.clone());
    let role_store = RoleStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    // FK prerequisite: roles and permissions reference a real workspace.
    let ws = app.sm.workspace.create(&ctx, WorkspaceForCreate::default()).await?;
    let ws_id: Uuid = ws.id.into();

    // -- Create test entities
    let role = role_store
        .create(
            &ctx,
            RoleForCreate {
                workspace_id: ws_id,
                name: "ROLE_FOR_ATTACH_DETACH".to_string(),
                ..Default::default()
            },
        )
        .await?;

    let perm1 = perm_store
        .create(
            &ctx,
            PermissionForCreate {
                workspace_id: ws_id,
                name: "PERMISSION_1_FOR_ATTACH_DETACH".to_string(),
                ..Default::default()
            },
        )
        .await?;

    let perm2 = perm_store
        .create(
            &ctx,
            PermissionForCreate {
                workspace_id: ws_id,
                name: "PERMISSION_2_FOR_ATTACH_DETACH".to_string(),
                ..Default::default()
            },
        )
        .await?;

    // -- Define metadata for read and mutate operations
    let mutate_meta = ManyToManyQueryMeta {
        single_table: RoleIden::Table,
        many_table: RoleIden::Permission,
        join_table: RoleIden::RolePermission,
        single_pk: RoleIden::Id,
        many_pk: RoleIden::PermissionPk,
        many_fk: RoleIden::PermissionId,
        join_fk: RoleIden::RoleId,
        agg_alias: RoleIden::Permissions,
        has_audit: true,
    };

    let read_meta = ManyToManyQueryMeta {
        single_table: RoleIden::Table,
        many_table: RoleIden::Permission,
        join_table: RoleIden::RolePermission,
        single_pk: RoleIden::Id,
        many_pk: RoleIden::PermissionPk,
        many_fk: RoleIden::PermissionId,
        join_fk: RoleIden::RoleId,
        agg_alias: RoleIden::Permissions,
        has_audit: true,
    };

    // -- Initial State Verification
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert!(
        role_with_perms.permissions.is_empty(),
        "Initially, the role should have no permissions."
    );

    // -- Test Attach
    // Attach the first permission
    attach_link(&ctx, &dbx, &role.id, &perm1.id, &mutate_meta).await?;
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert_eq!(
        role_with_perms.permissions.len(),
        1,
        "Should have one permission after attaching."
    );
    assert_eq!(
        role_with_perms.permissions[0].id, perm1.id,
        "The correct permission should be attached."
    );

    // Attach the same permission again to test idempotency
    attach_link(&ctx, &dbx, &role.id, &perm1.id, &mutate_meta).await?;
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert_eq!(
        role_with_perms.permissions.len(),
        1,
        "Attaching an existing link should be idempotent."
    );

    // Attach the second permission
    attach_link(&ctx, &dbx, &role.id, &perm2.id, &mutate_meta).await?;
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert_eq!(
        role_with_perms.permissions.len(),
        2,
        "Should have two permissions after attaching the second one."
    );

    // -- Test Detach
    // Detach the first permission
    detach_link(&ctx, &dbx, &role.id, &perm1.id, &mutate_meta).await?;
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert_eq!(
        role_with_perms.permissions.len(),
        1,
        "Should have one permission remaining after detaching the first."
    );
    assert_eq!(
        role_with_perms.permissions[0].id, perm2.id,
        "The remaining permission should be the second one."
    );

    // Detach a non-existent link (perm1 again)
    detach_link(&ctx, &dbx, &role.id, &perm1.id, &mutate_meta).await?;
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert_eq!(
        role_with_perms.permissions.len(),
        1,
        "Detaching a non-existent link should not change anything."
    );

    // Detach the second permission
    detach_link(&ctx, &dbx, &role.id, &perm2.id, &mutate_meta).await?;
    let role_with_perms: RoleWithPermissions =
        get_many_to_many(&ctx, &dbx, &role.id, &read_meta).await?;
    assert!(
        role_with_perms.permissions.is_empty(),
        "Should have no permissions after detaching the last one."
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_one_to_many_opt_not_found() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = AccountStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let missing_id = Uuid::new_v4().into();

    // Act & Assert: the _opt variant returns Ok(None) without an error
    let res = store.get_one_to_many_opt(&ctx, &missing_id).await?;
    assert!(res.is_none(), "expected Ok(None) for a missing parent");

    // The non-opt variant maps the miss to EntityNotFound
    let err = store.get_one_to_many(&ctx, &missing_id).await;
    assert!(matches!(err, Err(StoreError::EntityNotFound { .. })));

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_get_many_to_many_opt_not_found() -> StoreResult<()> {
    // Arrange
    let app = init_test().await;
    let dbx = app.sm.dbx().clone();
    let store = RoleStore::new(dbx.clone());
    let ctx = StoreCtx::bootstrap();

    let missing_id = Uuid::new_v4().into();

    // Act & Assert: the _opt variant returns Ok(None) without an error
    let res = store.get_many_to_many_opt(&ctx, &missing_id).await?;
    assert!(res.is_none(), "expected Ok(None) for a missing parent");

    // The non-opt variant maps the miss to EntityNotFound
    let err = store.get_many_to_many(&ctx, &missing_id).await;
    assert!(matches!(err, Err(StoreError::EntityNotFound { .. })));

    Ok(())
}
