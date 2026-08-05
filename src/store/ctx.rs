use std::str::FromStr;

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

    pub fn new_root() -> Self {
        let root_user_id: Uuid = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let root_workspace_id: Uuid =
            Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();

        Self {
            user_id: root_user_id,
            ws_id: root_workspace_id,
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
