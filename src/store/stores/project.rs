use std::sync::Arc;

use serde_json::json;

use crate::store::{
    ctx::StoreCtx,
    dbx::PgDbx,
    entities::{
        id::DbId,
        project::{ProjectFilter, ProjectForCreate, ProjectForUpdate, ProjectIden, ProjectRow},
    },
    error::StoreResult,
    queries::meta::{ContainsFilterQueryMeta, MutateQueryMeta, ReadQueryMeta},
    traits::{
        crud::List,
        dbx::DbExecutor,
        meta::{ContainsFilterStore, MutateStore, ReadStore, Store},
    },
};

/// The struct for our Project store, holding the database connection wrapper.
pub struct ProjectStore<D: DbExecutor> {
    dbx: Arc<D>,
}

impl<D: DbExecutor> ProjectStore<D> {
    // Added generic
    /// Creates a new `ProjectStore`.
    pub fn new(dbx: Arc<D>) -> Self {
        // Use generic
        Self { dbx }
    }

    pub async fn get_by_code(
        &self,
        ctx: &StoreCtx,
        code: &str,
        workspace_id: &DbId,
    ) -> StoreResult<Option<ProjectRow>> {
        let filter: ProjectFilter = json!({
            "code": code.to_string(),
            "workspace_id": workspace_id.to_string()
        })
        .try_into()?;

        Ok(self.list(ctx, Some(filter), None).await?.into_iter().next())
    }
}

// region:    --- Base Trait Implementations
// -----------------------------------------------------------------------------
// By implementing these meta traits, ProjectStore implicitly gains all of the
// CRUD, Batch, and Query capabilities from the blanket implementations.

impl<D: DbExecutor> Store for ProjectStore<D> {
    type Iden = ProjectIden;
    type Row = ProjectRow;

    fn dbx(&self) -> impl DbExecutor {
        self.dbx.clone()
    }
}

impl<D: DbExecutor> ReadStore for ProjectStore<D> {
    type FilterStoreParams = ProjectFilter;

    fn read_meta(&self) -> ReadQueryMeta<Self::Iden> {
        ReadQueryMeta {
            table: ProjectIden::Table,
            pk: ProjectIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> MutateStore for ProjectStore<D> {
    type CreateStoreParams = ProjectForCreate;
    type UpdateStoreParams = ProjectForUpdate;

    fn mutate_meta(&self) -> MutateQueryMeta<Self::Iden> {
        MutateQueryMeta {
            table: ProjectIden::Table,
            pk: ProjectIden::Id,
            has_audit: true,
        }
    }
}

impl<D: DbExecutor> ContainsFilterStore for ProjectStore<D> {
    fn contains_tags_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ProjectIden::Table,
            col: ProjectIden::Tags,
            has_audit: true,
        }
    }

    fn contains_json_meta(&self) -> ContainsFilterQueryMeta<Self::Iden> {
        ContainsFilterQueryMeta {
            table: ProjectIden::Table,
            col: ProjectIden::Meta,
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
    use crate::{
        cache::redis::RedisChx,
        dev::init::init_test,
        store::{
            ctx::StoreCtx,
            entities::{project::ProjectForCreate, workspace::WorkspaceForCreate},
            error::StoreError,
            traits::{contains::FilterByContains, crud::*},
        },
    };
    use anyhow::Result;
    use serde_json::json;
    use serial_test::serial;
    use uuid::Uuid;

    /// Helper function to seed the necessary Workspace for a Project.
    async fn seed_prerequisite(
        ctx: &StoreCtx,
        app: &crate::app::AppState<PgDbx, RedisChx>,
    ) -> Result<Uuid> {
        let workspace = app
            .sm
            .workspace
            .create(ctx, WorkspaceForCreate::default())
            .await?;
        Ok(workspace.id.into())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_get_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = ProjectStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let data = ProjectForCreate {
            workspace_id,
            name: "test-project-create".to_string(),
            ..Default::default()
        };

        // -- Execute
        let created_project = store.create(&ctx, data).await?;
        let fetched_project = store.get(&ctx, &created_project.id).await?;

        // -- Assert
        assert_eq!(created_project.name, "test-project-create");
        assert_eq!(created_project.workspace_id, workspace_id);
        assert_eq!(fetched_project.id, created_project.id);
        assert_eq!(fetched_project.name, created_project.name);

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_update_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = ProjectStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let created_project = store
            .create(
                &ctx,
                ProjectForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        let update_data = ProjectForUpdate {
            name: Some("updated-project-name".to_string()),
            ..Default::default()
        };

        // -- Execute
        let updated_project = store.update(&ctx, &created_project.id, update_data).await?;
        let fetched_project = store.get(&ctx, &created_project.id).await?;

        // -- Assert
        assert_eq!(updated_project.name, "updated-project-name");
        assert_eq!(fetched_project.name, "updated-project-name");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = ProjectStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let created_project = store
            .create(
                &ctx,
                ProjectForCreate {
                    workspace_id,
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute
        let deleted_project = store.delete(&ctx, &created_project.id).await?;
        let get_result = store.get(&ctx, &created_project.id).await;

        // -- Assert
        assert_eq!(deleted_project.id, created_project.id);
        assert!(
            matches!(get_result, Err(StoreError::EntityNotFound { .. })),
            "Getting the project after deletion should fail"
        );

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_list_with_filter_ok() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = ProjectStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        let projects_to_create = vec![
            ProjectForCreate {
                workspace_id,
                name: "list-proj-a".to_string(),
                ..Default::default()
            },
            ProjectForCreate {
                workspace_id,
                name: "list-proj-b".to_string(),
                ..Default::default()
            },
        ];
        store.create_many(&ctx, projects_to_create).await?;

        // -- Execute
        let filter: ProjectFilter = json!({ "name": "list-proj-b" }).try_into()?;
        let projects = store.list(&ctx, Some(filter), None).await?;

        // -- Assert
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "list-proj-b");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_filter_by_contains_tags() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = ProjectStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        // -- Create test data with different tags
        store
            .create(
                &ctx,
                ProjectForCreate {
                    workspace_id,
                    name: "tags-proj-a".into(),
                    tags: vec!["frontend".into(), "critical".into()],
                    ..Default::default()
                },
            )
            .await?;
        store
            .create(
                &ctx,
                ProjectForCreate {
                    workspace_id,
                    name: "tags-proj-b".into(),
                    tags: vec!["backend".into(), "api".into()],
                    ..Default::default()
                },
            )
            .await?;

        // -- Execute & Assert
        let frontend_projects = store
            .filter_by_tags_contain(&ctx, vec!["frontend".into()], None)
            .await?;
        assert_eq!(
            frontend_projects.len(),
            1,
            "Should find 1 project with 'frontend' tag"
        );
        assert_eq!(frontend_projects[0].name, "tags-proj-a");

        let api_projects = store
            .filter_by_tags_contain(&ctx, vec!["api".into()], None)
            .await?;
        assert_eq!(
            api_projects.len(),
            1,
            "Should find 1 project with 'api' tag"
        );
        assert_eq!(api_projects[0].name, "tags-proj-b");

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_create_multiple_projects_unique_defaults() -> Result<()> {
        // -- Setup
        let app = init_test().await;
        let dbx = app.sm.dbx().clone();
        let store = ProjectStore::new(dbx);
        let ctx = StoreCtx::bootstrap();
        let workspace_id = seed_prerequisite(&ctx, &app).await?;

        // -- Execute: create 3 projects using Default::default() with same workspace
        let proj1 = store
            .create(&ctx, ProjectForCreate { workspace_id, ..Default::default() })
            .await?;
        let proj2 = store
            .create(&ctx, ProjectForCreate { workspace_id, ..Default::default() })
            .await?;
        let proj3 = store
            .create(&ctx, ProjectForCreate { workspace_id, ..Default::default() })
            .await?;

        // -- Assert: all 3 projects persisted with unique names/codes and valid IDs
        assert_ne!(proj1.id, proj2.id);
        assert_ne!(proj2.id, proj3.id);
        assert_ne!(proj1.id, proj3.id);
        assert_ne!(proj1.name, proj2.name, "Default project names should be unique");
        assert_ne!(proj2.name, proj3.name, "Default project names should be unique");
        assert_ne!(proj1.name, proj3.name, "Default project names should be unique");
        assert_ne!(proj1.code, proj2.code, "Default project codes should be unique");
        assert_ne!(proj2.code, proj3.code, "Default project codes should be unique");
        assert_ne!(proj1.code, proj3.code, "Default project codes should be unique");
        assert_eq!(proj1.workspace_id, workspace_id);
        assert_eq!(proj2.workspace_id, workspace_id);
        assert_eq!(proj3.workspace_id, workspace_id);

        // Verify all 3 are retrievable
        let fetched1 = store.get(&ctx, &proj1.id).await?;
        let fetched2 = store.get(&ctx, &proj2.id).await?;
        let fetched3 = store.get(&ctx, &proj3.id).await?;
        assert_eq!(fetched1.name, proj1.name);
        assert_eq!(fetched2.name, proj2.name);
        assert_eq!(fetched3.name, proj3.name);

        Ok(())
    }
}
// -----------------------------------------------------------------------------
// endregion: --- Tests
