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

    /// Creates a system-level store context with a real user ID for audit
    /// trails. Unlike `bootstrap()` (nil UUIDs), this carries a traceable
    /// identity so `created_by`/`updated_by` columns reflect the actual
    /// system account.
    pub fn system(user_id: Uuid, ws_id: Uuid) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_ctx_new() {
        // -- Setup
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();

        // -- Execute
        let ctx = StoreCtx::new(user_id, ws_id);

        // -- Assert
        assert_eq!(ctx.user_id, user_id);
        assert_eq!(ctx.ws_id, ws_id);
        assert_eq!(ctx.workspace_scope(), None);
    }

    #[test]
    fn test_store_ctx_system() {
        // -- Setup
        let user_id = Uuid::new_v4();
        let ws_id = Uuid::new_v4();

        // -- Execute
        let ctx = StoreCtx::system(user_id, ws_id);

        // -- Assert
        assert_eq!(ctx.user_id, user_id);
        assert_eq!(ctx.ws_id, ws_id);
        assert_eq!(ctx.workspace_scope(), None);
    }

    #[test]
    fn test_store_ctx_bootstrap() {
        // -- Execute
        let ctx = StoreCtx::bootstrap();

        // -- Assert
        assert_eq!(ctx.user_id, Uuid::nil());
        assert_eq!(ctx.ws_id, Uuid::nil());
        assert_eq!(ctx.workspace_scope(), None);
    }

    #[test]
    fn test_workspace_scope_set_roundtrip() {
        // -- Setup
        let mut ctx = StoreCtx::bootstrap();
        let scope = Uuid::new_v4();

        // -- Execute
        ctx.set_workspace_scope(Some(scope));

        // -- Assert
        assert_eq!(ctx.workspace_scope(), Some(scope));

        // -- Execute
        ctx.set_workspace_scope(None);

        // -- Assert
        assert_eq!(ctx.workspace_scope(), None);
    }
}
