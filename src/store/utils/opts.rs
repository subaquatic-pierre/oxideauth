use modql::filter::ListOptions;
use sea_query::Iden;
use serde::Deserialize;

use crate::store::entities::audit::AuditIden;
use crate::store::error::{StoreError, StoreResult};
use crate::store::traits::meta::Store;

/// Default number of rows to return in a list query.
pub const LIST_LIMIT_DEFAULT: i64 = 100;

/// Hard cap on the maximum number of rows a client can request.
pub const LIST_LIMIT_MAX: i64 = 500;

/// Utility struct for validating and normalizing `ListOptions`.
///
/// Ensures client-provided list options (limit, offset, order_bys)
/// are safe and consistent with store defaults (like audit ordering).
pub struct ListOptionsValidator;

impl ListOptionsValidator {
    /// Validate and normalize list options.
    ///
    /// - If `opts` is `Some`, enforces a maximum limit.
    ///   * If `limit` > `LIST_LIMIT_MAX`, returns an error.
    ///   * If no `limit` is provided, defaults to `LIST_LIMIT_MAX`.
    ///
    /// - If `opts` is `None`, falls back to defaults:
    ///   * If the store has audit fields (`created_at`), sorts by `created_at DESC`
    ///     with a default limit of `LIST_LIMIT_DEFAULT`.
    ///   * Otherwise uses the bare default (limit only, no order).
    pub fn validate_list_opts(
        opts: Option<ListOptions>,
        has_audit_fields: bool,
    ) -> StoreResult<ListOptions> {
        let opts = match opts {
            Some(mut opts) => {
                // If caller provided a limit, check it's within the allowed max
                if let Some(limit) = opts.limit {
                    Self::validate_limit(limit)?
                } else {
                    // If no limit provided, enforce the max limit by default
                    opts.limit = Some(LIST_LIMIT_MAX);
                }
                opts
            }
            None => {
                // No options provided → use sensible defaults
                if has_audit_fields {
                    // If table has audit fields, sort by created_at DESC (newest first)
                    Self::with_order_by_created_at()
                } else {
                    // Else rely on Postgres default row order
                    Self::default()
                }
            }
        };

        Ok(opts.to_owned())
    }

    /// Build a default `ListOptions` that orders by `created_at DESC`.
    ///
    /// Uses a *soft default* of `LIST_LIMIT_DEFAULT` rows.
    pub fn with_order_by_created_at() -> ListOptions {
        ListOptions {
            limit: Some(LIST_LIMIT_DEFAULT),
            offset: None,
            // `!` prefix = descending order in modql syntax
            order_bys: Some(format!("!{}", AuditIden::CreatedAt.to_string()).into()),
        }
    }

    pub fn validate_limit(limit: i64) -> StoreResult<()> {
        if limit > LIST_LIMIT_MAX {
            return Err(StoreError::ListLimitExceeded {
                max: LIST_LIMIT_MAX,
                actual: limit,
            });
        }

        Ok(())
    }

    /// Build a default `ListOptions` with only a row limit, no ordering.
    pub fn default() -> ListOptions {
        ListOptions {
            limit: Some(LIST_LIMIT_DEFAULT),
            offset: None,
            order_bys: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Deref;

    use super::*;
    use modql::filter::{ListOptions, OrderBys};
    use serial_test::serial;

    // Convert OrderBys into a single string for assertions.
    // `OrderBys` derefs to a Vec<String>, so we can join on commas.
    fn ob_str(ob: &Option<OrderBys>) -> Option<String> {
        if let Some(ob) = ob {
            let mut tokens = vec![];
            for token in ob {
                tokens.push(token.to_string());
            }
            return Some(tokens.join(","));
        }

        None
    }

    #[tokio::test]
    #[serial]
    async fn test_none_opts_with_audit_fields_adds_created_at_desc_and_default_limit() {
        let res =
            ListOptionsValidator::validate_list_opts(None, /*has_audit_fields*/ true).unwrap();
        assert_eq!(res.limit, Some(LIST_LIMIT_DEFAULT));
        // Expect "!created_at" (descending on created_at)
        assert_eq!(ob_str(&res.order_bys).as_deref(), Some("created_at DESC"));
        // Offset should remain None by default
        assert_eq!(res.offset, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_none_opts_without_audit_fields_uses_default_no_order() {
        let res =
            ListOptionsValidator::validate_list_opts(None, /*has_audit_fields*/ false).unwrap();
        assert_eq!(res.limit, Some(LIST_LIMIT_DEFAULT));
        assert!(res.order_bys.is_none());
        assert_eq!(res.offset, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_some_opts_within_max_limit_is_accepted_and_preserved() {
        let opts = ListOptions {
            limit: Some(250),
            offset: Some(20),
            order_bys: None,
        };
        let res = ListOptionsValidator::validate_list_opts(
            Some(opts.clone()),
            /*has_audit_fields*/ true,
        )
        .unwrap();
        // Provided limit is kept as-is (<= max)
        assert_eq!(res.limit, Some(250));
        // Provided offset is preserved
        assert_eq!(res.offset, Some(20));
        // Since caller provided opts, we do not auto-add order_bys
        assert!(res.order_bys.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_some_opts_without_limit_defaults_to_max_limit() {
        let opts = ListOptions {
            limit: None,
            offset: Some(5),
            order_bys: None,
        };
        let res =
            ListOptionsValidator::validate_list_opts(Some(opts), /*has_audit_fields*/ true)
                .unwrap();
        assert_eq!(res.limit, Some(LIST_LIMIT_MAX));
        assert_eq!(res.offset, Some(5));
        assert!(res.order_bys.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_some_opts_over_max_limit_errors() {
        let opts = ListOptions {
            limit: Some(LIST_LIMIT_MAX + 1),
            offset: None,
            order_bys: None,
        };
        let err =
            ListOptionsValidator::validate_list_opts(Some(opts), /*has_audit_fields*/ false)
                .unwrap_err();
        match err {
            StoreError::ListLimitExceeded { max, actual } => {
                assert_eq!(max, LIST_LIMIT_MAX);
                assert_eq!(actual, LIST_LIMIT_MAX + 1);
            }
            e => panic!("unexpected error variant: {:?}", e),
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_provided_order_bys_is_preserved_and_not_overwritten() {
        // Caller explicitly wants ascending by created_at (no '!' prefix)
        let provided = Some("created_at".to_owned().into());
        let opts = ListOptions {
            limit: Some(42),
            offset: None,
            order_bys: provided.clone(),
        };
        let res =
            ListOptionsValidator::validate_list_opts(Some(opts), /*has_audit_fields*/ true)
                .unwrap();
        assert_eq!(res.limit, Some(42));
        // Our validator should not override caller's order_bys
        assert_eq!(ob_str(&res.order_bys).as_deref(), Some("created_at ASC"));
    }

    #[tokio::test]
    #[serial]
    async fn test_with_order_by_created_at_helper_builds_expected_default() {
        let res = ListOptionsValidator::with_order_by_created_at();
        assert_eq!(res.limit, Some(LIST_LIMIT_DEFAULT));
        assert_eq!(ob_str(&res.order_bys).as_deref(), Some("created_at DESC"));
        assert_eq!(res.offset, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_default_helper_builds_expected_default() {
        let res = ListOptionsValidator::default();
        assert_eq!(res.limit, Some(LIST_LIMIT_DEFAULT));
        assert!(res.order_bys.is_none());
        assert_eq!(res.offset, None);
    }
}
