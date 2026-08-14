pub const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");
pub const SQL_DIR: &str = "sql";

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_sql_dir_constant() {
        assert_eq!(SQL_DIR, "sql");
    }

    #[test]
    fn test_project_root_is_non_empty_existing_dir() {
        assert!(!PROJECT_ROOT.is_empty());
        assert!(
            Path::new(PROJECT_ROOT).is_dir(),
            "PROJECT_ROOT should point at the crate manifest directory"
        );
    }
}
