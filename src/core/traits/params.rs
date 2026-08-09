use uuid::Uuid;

use crate::core::error::{CoreError, CoreResult};

/// Conversion from a web request DTO into a core service parameter struct.
///
/// Unlike `From`, this trait explicitly injects the resolved workspace ID
/// so that the DTO does not need to carry a `workspace_id` field. The
/// workspace is resolved by the middleware (from the token or header) and
/// passed downstream.
pub trait IntoParams<P> {
    fn into_params(self, workspace_id: Uuid) -> CoreResult<P>;
}

/// Validate a core parameter struct before it is handed to the service layer.
pub trait ValidateParams
where
    Self: Sized,
{
    fn validate(self) -> CoreResult<Self>;
}
