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
