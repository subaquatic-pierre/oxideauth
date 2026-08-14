use modql::filter::{IntoFilterNodes, OpValsString};
use uuid::Uuid;

use crate::{
    core::{
        error::CoreResult,
        models::list::{RequestFilterParams, RequestListOptions},
        traits::{
            filter::{OpValIsString, OpValWorkspaceId},
            params::ValidateParams,
        },
    },
    store::utils::ListOptionsValidator,
};

pub trait RequestListParams<F: IntoFilterNodes + Clone + OpValWorkspaceId> {
    fn filter(&self) -> Option<RequestFilterParams<F>>;
    fn options(&self) -> Option<RequestListOptions>;

    fn list_options(&self) -> RequestListOptions {
        let options = self.options().unwrap_or_else(ListOptionsValidator::default);
        options
    }

    fn validate_filter_tags(&self) -> CoreResult<RequestFilterParams<F>> {
        let filter = self.filter();
        let params = match filter {
            Some(filter) => filter.validate()?,
            None => RequestFilterParams::new(None, None),
        };

        Ok(params)
    }

    fn workspace_id(&self) -> Option<Uuid> {
        self.filter()
            .as_ref()
            .and_then(|filter| filter.fields.as_ref())
            .and_then(|fields| fields.get_workspace_id_opval()) // Use the new trait method
            // Only proceed if the operator is OpValString::Eq and contains a string
            .and_then(|op_val_string| op_val_string.as_eq_string()) // Assuming OpValString::as_eq_string() exists
            // Attempt to parse the resulting string as a Uuid
            .and_then(|val_str| Uuid::try_parse(val_str).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::project::ProjectFilter;
    use modql::filter::{ListOptions, OpValString, OpValsString};
    use uuid::Uuid;

    /// A minimal params struct used to exercise the `RequestListParams` default
    /// implementations. `ProjectFilter` already implements `OpValWorkspaceId`
    /// and `IntoFilterNodes`, satisfying the trait bounds.
    struct TestListParams {
        filter: Option<RequestFilterParams<ProjectFilter>>,
        options: Option<RequestListOptions>,
    }

    impl RequestListParams<ProjectFilter> for TestListParams {
        fn filter(&self) -> Option<RequestFilterParams<ProjectFilter>> {
            self.filter.clone()
        }

        fn options(&self) -> Option<RequestListOptions> {
            self.options.clone()
        }
    }

    fn filter_with_workspace_id(op_val: OpValString) -> RequestFilterParams<ProjectFilter> {
        let fields = ProjectFilter {
            workspace_id: Some(OpValsString(vec![op_val])),
            ..Default::default()
        };
        RequestFilterParams::new(None, Some(fields))
    }

    #[test]
    fn test_workspace_id_parses_eq_uuid() {
        let ws_uuid = Uuid::new_v4();
        let params = TestListParams {
            filter: Some(filter_with_workspace_id(OpValString::Eq(ws_uuid.to_string()))),
            options: None,
        };
        assert_eq!(params.workspace_id(), Some(ws_uuid));
    }

    #[test]
    fn test_workspace_id_none_when_no_filter() {
        let params = TestListParams {
            filter: None,
            options: None,
        };
        assert_eq!(params.workspace_id(), None);
    }

    #[test]
    fn test_workspace_id_none_when_no_workspace_field() {
        let params = TestListParams {
            filter: Some(RequestFilterParams::new(None, Some(ProjectFilter::default()))),
            options: None,
        };
        assert_eq!(params.workspace_id(), None);
    }

    #[test]
    fn test_workspace_id_none_for_non_eq_opval() {
        let params = TestListParams {
            filter: Some(filter_with_workspace_id(OpValString::Contains("abc".to_string()))),
            options: None,
        };
        assert_eq!(params.workspace_id(), None);
    }

    #[test]
    fn test_workspace_id_none_for_invalid_uuid_string() {
        let params = TestListParams {
            filter: Some(filter_with_workspace_id(OpValString::Eq("not-a-uuid".to_string()))),
            options: None,
        };
        assert_eq!(params.workspace_id(), None);
    }

    #[test]
    fn test_list_options_returns_provided_options() {
        let options = ListOptions {
            limit: Some(10),
            offset: Some(2),
            order_bys: None,
        };
        let params = TestListParams {
            filter: None,
            options: Some(options.clone()),
        };

        let res = params.list_options();
        assert_eq!(res.limit, options.limit);
        assert_eq!(res.offset, options.offset);
    }

    #[test]
    fn test_list_options_defaults_when_none() {
        let params = TestListParams {
            filter: None,
            options: None,
        };

        let res = params.list_options();
        let expected = ListOptionsValidator::default();
        assert_eq!(res.limit, expected.limit);
        assert_eq!(res.offset, expected.offset);
        assert_eq!(res.order_bys.is_none(), expected.order_bys.is_none());
    }

    #[test]
    fn test_validate_filter_tags_preserves_filter_fields() {
        let ws_uuid = Uuid::new_v4();
        let filter = filter_with_workspace_id(OpValString::Eq(ws_uuid.to_string()));
        let params = TestListParams {
            filter: Some(filter.clone()),
            options: None,
        };

        let res = params.validate_filter_tags().unwrap();
        assert!(res.fields.is_some());
        assert!(res.tags.is_none());
    }

    #[test]
    fn test_validate_filter_tags_without_filter_returns_empty() {
        let params = TestListParams {
            filter: None,
            options: None,
        };

        let res = params.validate_filter_tags().unwrap();
        assert!(res.fields.is_none());
        assert!(res.tags.is_none());
    }
}
