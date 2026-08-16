//! Policy evaluation engine.
//!
//! `PolicyEngine` is a stub for now — it establishes the delegation boundary in
//! [`crate::core::services::validator::AuthValidator`] and will be expanded to
//! implement policy evaluation in a future feature.

/// Placeholder for future policy evaluation.
///
/// Held by [`crate::core::services::validator::AuthValidator`] to establish the
/// delegation boundary; it has no behavior yet.
#[derive(Clone, Debug, Default)]
pub struct PolicyEngine;
