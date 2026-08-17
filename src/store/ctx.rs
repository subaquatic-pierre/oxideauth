use uuid::Uuid;

#[derive(Debug)]
pub struct StoreCtx {
    pub user_id: Uuid,
    pub ws_id: Option<Uuid>,
}

impl StoreCtx {
    pub fn new(user_id: Uuid, ws_id: Uuid) -> Self {
        Self {
            user_id,
            ws_id: Some(ws_id),
        }
    }

    /// Creates a system-level store context with a real user ID for audit
    /// trails. Unlike `bootstrap()` (nil UUIDs), this carries a traceable
    /// identity so `created_by`/`updated_by` columns reflect the actual
    /// system account.
    pub fn system(user_id: Uuid, ws_id: Uuid) -> Self {
        Self {
            user_id,
            ws_id: Some(ws_id),
        }
    }

    /// Creates a bootstrap context with nil UUIDs — no pre-existing DB data required.
    ///
    /// Used during seeding/initialization before any accounts or workspaces exist.
    /// All queries run with `workspace_scope: None` (no row-level filtering).
    pub fn bootstrap() -> Self {
        Self {
            user_id: Uuid::nil(),
            ws_id: None,
        }
    }

    pub fn workspace_scope(&self) -> Option<Uuid> {
        self.ws_id
    }

    pub fn set_workspace_scope(&mut self, ws: Option<Uuid>) {
        self.ws_id = ws
    }
}

