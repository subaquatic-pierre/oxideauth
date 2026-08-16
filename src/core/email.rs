use crate::core::error::{CoreError, CoreResult};

/// Normalize an email address: trim surrounding whitespace and lowercase it.
pub fn normalize_email(input: &str) -> String {
    input.trim().to_lowercase()
}

/// Validate that `input` is a non-empty, well-formed email address and return the
/// normalized form. Minimal check: exactly one `@`, non-empty local and domain
/// parts, and a `.` in the domain.
pub fn validate_email(input: &str) -> CoreResult<String> {
    let normalized = normalize_email(input);
    if normalized.is_empty() {
        return Err(CoreError::InvalidParams("email required".to_string()));
    }
    if normalized.matches('@').count() != 1 {
        return Err(CoreError::InvalidParams("invalid email".to_string()));
    }
    let at = normalized.find('@').unwrap();
    let local = &normalized[..at];
    let domain = &normalized[at + 1..];
    if local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(CoreError::InvalidParams("invalid email".to_string()));
    }
    Ok(normalized)
}
