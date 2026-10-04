//! Agents-owned registration, template ceilings and current-token policy.
mod identity;
mod templates;
pub use identity::{AdmittedManagedOAuthClient, ManagedOAuthClientResolver};
pub use templates::{ManagedTemplateCatalog, runtime_template_revision};
#[cfg(test)]
#[path = "../../../../testing/fixtures/store.rs"]
mod test_store;
#[cfg(test)]
mod tests;
