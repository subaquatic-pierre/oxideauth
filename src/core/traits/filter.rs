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

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal filter struct used to exercise the trait contracts
    /// (they must be implemented on a concrete type to be testable).
    struct TestFilter {
        workspace_id: Option<OpValString>,
        account_id: Option<OpValString>,
    }

    impl OpValWorkspaceId for TestFilter {
        fn get_workspace_id_opval(&self) -> Option<&OpValString> {
            self.workspace_id.as_ref()
        }
    }

    impl OpValAccountId for TestFilter {
        fn get_account_id_opval(&self) -> Option<&OpValString> {
            self.account_id.as_ref()
        }
    }

    #[test]
    fn test_as_eq_string_returns_some_for_eq() {
        let op_val = OpValString::Eq("some-string".to_string());
        assert_eq!(op_val.as_eq_string(), Some("some-string"));
    }

    #[test]
    fn test_as_eq_string_returns_none_for_non_eq_variants() {
        let non_eq_ops = [
            OpValString::Not("x".to_string()),
            OpValString::Contains("x".to_string()),
            OpValString::StartsWith("x".to_string()),
            OpValString::Lt("x".to_string()),
            OpValString::Empty(true),
            OpValString::Null(false),
        ];
        for op in non_eq_ops {
            assert_eq!(op.as_eq_string(), None, "expected None for {op:?}");
        }
    }

    #[test]
    fn test_get_workspace_id_opval_returns_some_when_present() {
        let filter = TestFilter {
            workspace_id: Some(OpValString::Eq("ws-id".to_string())),
            account_id: None,
        };

        let op_val = filter.get_workspace_id_opval();
        assert!(matches!(op_val, Some(OpValString::Eq(s)) if s == "ws-id"));
        assert_eq!(op_val.and_then(|v| v.as_eq_string()), Some("ws-id"));
    }

    #[test]
    fn test_get_workspace_id_opval_returns_none_when_absent() {
        let filter = TestFilter {
            workspace_id: None,
            account_id: None,
        };
        assert!(filter.get_workspace_id_opval().is_none());
    }

    #[test]
    fn test_get_account_id_opval_returns_some_when_present() {
        let filter = TestFilter {
            workspace_id: None,
            account_id: Some(OpValString::Eq("acc-id".to_string())),
        };

        let op_val = filter.get_account_id_opval();
        assert!(matches!(op_val, Some(OpValString::Eq(s)) if s == "acc-id"));
        assert_eq!(op_val.and_then(|v| v.as_eq_string()), Some("acc-id"));
    }

    #[test]
    fn test_get_account_id_opval_returns_none_when_absent() {
        let filter = TestFilter {
            workspace_id: None,
            account_id: None,
        };
        assert!(filter.get_account_id_opval().is_none());
    }
}
