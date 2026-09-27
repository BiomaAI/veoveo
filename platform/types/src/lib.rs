//! Protocol-independent names and resource addresses shared by Veoveo components.
//! Domain vocabularies and authorization decisions belong to their owning contracts.

mod digest;
mod error;
mod names;
mod resource;
mod resource_components;
mod scopes;

pub use digest::{Sha256Digest, Sha256DigestError};
pub use error::IdentifierError;
pub use names::{ResourceScheme, ScopeDefinition, ScopeName};
pub use resource::{ResourceAddress, ResourceUri};
pub use resource_components::{ResourceUriBuilder, ResourceUriError, ResourceUriParts, UriSegment};
