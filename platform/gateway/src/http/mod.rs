//! Shared HTTP admission and owned optional-module lifetimes.
mod context;
mod lifecycle;
pub mod native_mcp;
mod registration;
pub use context::*;
pub use lifecycle::*;
pub use registration::*;

pub mod auth_support;
mod authentication;
pub use authentication::{
    authenticate_profile, load_resource_authorization_jwks, profile_from_request,
    profile_from_route,
};

pub mod action_audit;
pub mod stream_limits;
#[cfg(test)]
mod tests;
