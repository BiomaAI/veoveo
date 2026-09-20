//! Current managed registration and installation template ceilings.
mod identity;
mod templates;
pub use identity::EffectiveOAuthClient;
#[cfg(test)]
mod tests;
pub use templates::{ManagedTemplateCatalog, runtime_template_revision};
