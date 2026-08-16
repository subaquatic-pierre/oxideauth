use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        profile::{ProfileFilter, ProfileForCreate, ProfileForUpdate, ProfileIden, ProfileRow},
    },
    error::StoreResult,
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    traits::{
        crud::List,
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Profile store, holding the database connection wrapper.
pub struct ProfileStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> ProfileStore<D> {
    /// Creates a new `ProfileStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        Self { dbx }
    }

    /// Finds the single profile for an account within a workspace (if any).
    pub async fn find_by_account_workspace(
        &self,
        ctx: &StoreCtx,
        account_id: Uuid,
        workspace_id: Uuid,
    ) -> StoreResult<Option<ProfileRow>> {
        let filter: ProfileFilter = json!({
            "account_id": account_id.to_string(),
            "workspace_id": workspace_id.to_string(),
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, ProfileStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for ProfileStore<D> {
    type Iden = ProfileIden;
    type Row = ProfileRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for ProfileStore<D> {
    type FilterStoreParams = ProfileFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: ProfileIden::Table,
            pk: ProfileIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for ProfileStore<D> {
    type CreateStoreParams = ProfileForCreate;
    type UpdateStoreParams = ProfileForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: ProfileIden::Table,
            pk: ProfileIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for ProfileStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ProfileIden::Table,
            col: ProfileIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ProfileIden::Table,
            col: ProfileIden::Meta,
            has_audit: true,
        }
    }
}

// -----------------------------------------------------------------------------
// endregion: --- Base Trait Implementations

// region:    --- Tests
// -----------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{
        ctx::StoreCtx,
        dbx::MockDbx,
        entities::{
            audit::AuditFields,
            profile::ProfileMeta,
        },
        error::StoreError,
        traits::{contains::FilterByContains, crud::*},
    };
    use anyhow::Result;
    use serde_json::json;
    use uuid::Uuid;

    /// Helper to build a `ProfileRow` with default-ish values for the mock.
    fn profile_row() -> ProfileRow {
        ProfileRow {
            id: DbId::from(Uuid::new_v4()),
            account_id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: String::new(),
            description: None,
            display_name: None,
            job_title: None,
            timezone: None,
            avatar_url: None,
            version: 0,
            tags: vec![],
            meta: ProfileMeta::default(),
            audit: AuditFields::default(),
        }
    }

    #[tokio::test]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let profile_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<ProfileRow>(ProfileRow {
                    id: profile_id,
                    account_id,
                    workspace_id,
                    name: "test-profile-create".into(),
                    ..profile_row()
                })
                .with_optional::<ProfileRow>(Some(ProfileRow {
                    id: profile_id,
                    name: "test-profile-create".into(),
                    ..profile_row()
                })),
        );
        let store = ProfileStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        let data = ProfileForCreate {
            account_id,
            workspace_id,
            name: "test-profile-create".to_string(),
            ..Default::default()
        };

        // -- Execute
        let created_profile = store.create(&ctx, data).await?;
        let fetched_profile = store.get(&ctx, &created_profile.id).await?;

        // -- Assert
        assert_eq!(created_profile.name, "test-profile-create");
        assert_eq!(created_profile.workspace_id, workspace_id);
        assert_eq!(fetched_profile.id, created_profile.id);
        assert_eq!(fetched_profile.name, created_profile.name);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_one::<ProfileRow>(profile_row())
                .with_optional::<ProfileRow>(Some(ProfileRow {
                    name: "updated-profile-name".into(),
                    ..profile_row()
                }))
                .with_optional::<ProfileRow>(Some(ProfileRow {
                    name: "updated-profile-name".into(),
                    ..profile_row()
                })),
        );
        let store = ProfileStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let created_profile = store.create(&ctx, ProfileForCreate::default()).await?;
        let updated_profile = store
            .update(
                &ctx,
                &created_profile.id,
                ProfileForUpdate {
                    name: Some("updated-profile-name".to_string()),
                    ..Default::default()
                },
            )
            .await?;
        let fetched_profile = store.get(&ctx, &created_profile.id).await?;

        // -- Assert
        assert_eq!(updated_profile.name, "updated-profile-name");
        assert_eq!(fetched_profile.name, "updated-profile-name");

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let profile_id = DbId::from(Uuid::new_v4());
        let dbx = Arc::new(
            MockDbx::new()
                .with_optional::<ProfileRow>(Some(ProfileRow {
                    id: profile_id,
                    ..profile_row()
                }))
                .with_optional::<ProfileRow>(None),
        );
        let store = ProfileStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let deleted_profile = store.delete(&ctx, &profile_id).await?;
        let get_result = store.get(&ctx, &profile_id).await;

        // -- Assert
        assert_eq!(deleted_profile.id, profile_id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the profile after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<ProfileRow>(vec![profile_row(), profile_row()])
                .with_all::<ProfileRow>(vec![ProfileRow {
                    name: "list-profil-b".into(),
                    ..profile_row()
                }]),
        );
        let store = ProfileStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        store
            .create_many(
                &ctx,
                vec![ProfileForCreate::default(), ProfileForCreate::default()],
            )
            .await?;

        let filter: ProfileFilter = json!({ "name": "list-profil-b" }).try_into()?;
        let profiles = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "list-profil-b");

        Ok(())
    }

    #[tokio::test]
    async fn test_find_by_account_workspace_ok() -> Result<()> {
        // -- Setup
        let account_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let dbx = Arc::new(
            MockDbx::new().with_all::<ProfileRow>(vec![ProfileRow {
                account_id,
                workspace_id,
                ..profile_row()
            }]),
        );
        let store = ProfileStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute
        let found = store
            .find_by_account_workspace(&ctx, account_id, workspace_id)
            .await?;

        // -- Assert
        assert!(found.is_some());
        assert_eq!(found.unwrap().account_id, account_id);

        Ok(())
    }

    #[tokio::test]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let dbx = Arc::new(
            MockDbx::new()
                .with_all::<ProfileRow>(vec![ProfileRow {
                    name: "tags-profil-a".into(),
                    tags: vec!["frontend".into(), "critical".into()],
                    ..profile_row()
                }])
                .with_all::<ProfileRow>(vec![ProfileRow {
                    name: "tags-profil-b".into(),
                    tags: vec!["backend".into(), "api".into()],
                    ..profile_row()
                }]),
        );
        let store = ProfileStore::new(dbx);
        let ctx = StoreCtx::bootstrap();

        // -- Execute & Assert
        let frontend_profiles = store
            .filter_by_tags_contain(&ctx, vec!["frontend".into()], None)
            .await?;
        assert_eq!(
            frontend_profiles.len(),
            1,
            "Should find 1 profile with 'frontend' tag"
        );
        assert_eq!(frontend_profiles[0].name, "tags-profil-a");

        let api_profiles = store
            .filter_by_tags_contain(&ctx, vec!["api".into()], None)
            .await?;
        assert_eq!(
            api_profiles.len(),
            1,
            "Should find 1 profile with 'api' tag"
        );
        assert_eq!(api_profiles[0].name, "tags-profil-b");

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
