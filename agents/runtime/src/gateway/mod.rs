//! Agents-owned registration, template ceilings and current-token policy.
mod identity;
pub mod installation;
mod templates;
pub use identity::{AdmittedManagedOAuthClient, ManagedOAuthClientResolver};
pub use templates::{ManagedTemplateCatalog, runtime_template_revision};
#[cfg(test)]
mod tests;

pub mod capabilities;
pub mod http;
