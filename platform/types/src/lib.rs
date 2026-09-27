//! Protocol-independent names and resource addresses shared by Veoveo components.
//! Domain vocabularies and authorization decisions belong to their owning contracts.

mod error;
mod names;
mod resource;

pub use error::IdentifierError;
pub use names::{ResourceScheme, ScopeDefinition, ScopeName};
pub use resource::{ResourceAddress, ResourceUri};
