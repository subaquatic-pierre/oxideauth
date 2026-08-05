pub trait HasActiveFilter {
    /// Returns true if any field in the struct is Some(T).
    fn has_active_filter(&self) -> bool;
}
