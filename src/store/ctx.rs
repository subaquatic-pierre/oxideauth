use uuid::Uuid;

#[derive(Debug)]
pub struct StoreCtx {
    pub user_id: Uuid,
    pub ws_id: Uuid,
    pub workspace_scope: Option<Uuid>,
}

impl StoreCtx {
    pub fn new(user_id: Uuid, ws_id: Uuid) -> Self {
        Self {
            user_id,
            ws_id,
            workspace_scope: None,
        }
    }

    /// Creates a bootstrap context with nil UUIDs — no pre-existing DB data required.
    ///
    /// Used during seeding/initialization before any accounts or workspaces exist.
    /// All queries run with `workspace_scope: None` (no row-level filtering).
    pub fn bootstrap() -> Self {
        Self {
            user_id: Uuid::nil(),
            ws_id: Uuid::nil(),
            workspace_scope: None,
        }
    }

    pub fn workspace_scope(&self) -> Option<Uuid> {
        self.workspace_scope
    }

    pub fn set_workspace_scope(&mut self, ws: Option<Uuid>) {
        self.workspace_scope = ws
    }
}
