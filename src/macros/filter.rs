use crate::store::filter::HasActiveFilter;

#[macro_export]
macro_rules! impl_has_active_filter {
    // Matches a struct name followed by a list of field names
    ($struct_name:ident, $( $field:ident ),*) => {
        impl $crate::store::traits::filter::HasActiveFilter for $struct_name {
            fn has_active_filter(&self) -> bool {
                // Generates an expression that ORs the is_some() check for every field
                $( self.$field.is_some() )||*
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::store::traits::filter::HasActiveFilter;

    struct MacroTestFilter {
        name: Option<String>,
        email: Option<String>,
        count: Option<i32>,
    }

    crate::impl_has_active_filter!(MacroTestFilter, name, email, count);

    #[test]
    fn test_has_active_filter_false_when_all_fields_none() {
        let filter = MacroTestFilter {
            name: None,
            email: None,
            count: None,
        };
        assert!(!filter.has_active_filter());
    }

    #[test]
    fn test_has_active_filter_true_when_single_field_set() {
        let filter = MacroTestFilter {
            name: Some("alice".to_string()),
            email: None,
            count: None,
        };
        assert!(filter.has_active_filter());

        let filter = MacroTestFilter {
            name: None,
            email: Some("alice@example.com".to_string()),
            count: None,
        };
        assert!(filter.has_active_filter());
    }

    #[test]
    fn test_has_active_filter_true_when_multiple_fields_set() {
        let filter = MacroTestFilter {
            name: Some("alice".to_string()),
            email: Some("alice@example.com".to_string()),
            count: Some(3),
        };
        assert!(filter.has_active_filter());
    }
}
