//! Protocol-independent identity, names, and resource addresses shared by Veoveo components.
//! Domain vocabularies and authorization decisions belong to their owning contracts.
//!
//! Identities with the same wire spelling remain different types:
//! ```compile_fail
//! use veoveo_types::{PrincipalId, TenantId};
//! fn assign_actor(_: PrincipalId) {}
//! assign_actor(TenantId::new("operations").unwrap());
//! ```
//! Subject variants require the matching identity:
//! ```compile_fail
//! use veoveo_types::{AccessSubject, GroupId};
//! let subject = AccessSubject::Principal(GroupId::new("operators").unwrap());
//! ```
//! Delegated provenance requires the identity of the delegation:
//! ```compile_fail
//! use veoveo_types::{InvocationProvenance, PrincipalId};
//! let provenance = InvocationProvenance::Delegated {
//!     initiator: PrincipalId::new("operator").unwrap(),
//! };
//! ```

mod digest;
mod error;
pub mod identifier_syntax;
mod identity;
mod names;
mod provenance;
mod resource;
mod resource_components;
mod resource_template;
mod scopes;
mod task;

pub use digest::{Sha256Digest, Sha256DigestError};
pub use error::IdentifierError;
pub use names::{ResourceScheme, ScopeDefinition, ScopeName};
pub use resource::{ResourceAddress, ResourceUri, TaskResourceAddress};
pub use resource_components::{
    ResourceUriBuilder, ResourceUriError, ResourceUriParts, UriAuthority, UriSegment,
};
pub use resource_template::{ResourceTemplateError, ResourceTemplateUri};

pub use identity::{
    AccessSubject, DataLabelId, DelegationId, GroupId, PolicyVersion, PrincipalId, RoleId,
    TenantId, WorkContextId,
};
pub use provenance::{InvocationMode, InvocationProvenance};
pub use task::TaskId;
