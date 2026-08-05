use modql::filter::{OpValString, OpValsString};

pub trait OpValIsString {
    fn as_eq_string(&self) -> Option<&str>;
}

pub trait OpValWorkspaceId {
    /// Must return a reference to the Option<OpValString> for workspace_id.
    fn get_workspace_id_opval(&self) -> Option<&OpValString>;
}

pub trait OpValAccountId {
    /// Must return a reference to the Option<OpValString> for account_id.
    fn get_account_id_opval(&self) -> Option<&OpValString>;
}
