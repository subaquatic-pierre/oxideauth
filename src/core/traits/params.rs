use crate::core::error::CoreResult;

/// Conversion from a web request DTO into a core service parameter struct.
///
/// The workspace is no longer injected here: workspace-scoped DTOs carry an
/// optional `workspace_id` field that is mapped straight through. Resolution
/// and match-validation of that (optional) workspace happen later in
/// [`crate::core::services::validator::AuthValidator::validate_workspace`].
pub trait IntoParams<P> {
    fn into_params(self) -> CoreResult<P>;
}

/// Validate a core parameter struct before it is handed to the service layer.
pub trait ValidateParams
where
    Self: Sized,
{
    fn validate(self) -> CoreResult<Self>;
}
