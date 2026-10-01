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

mod access_grant;
mod authority;
mod digest;
mod error;
mod https_url;
pub mod identifier_syntax;
mod identity;
mod names;
mod platform_names;
mod provenance;
mod resource;
mod resource_components;
mod resource_selector;
mod resource_template;
mod scopes;
pub mod sha256_hex;
mod task;
mod task_type;

pub use access_grant::AccessGrant;
pub use authority::{
    AccessLevel, InvocationAuthority, WorkContextGrant, WorkContextMembershipLevel,
    WorkContextOutputPolicy,
};
pub use digest::{Sha256Digest, Sha256DigestError};
pub use error::IdentifierError;
pub use https_url::{HttpsUrl, HttpsUrlError};
pub use names::{ResourceScheme, ScopeDefinition, ScopeName};
pub use resource::{ResourceAddress, ResourceUri, TaskResourceAddress};
pub use resource_components::{
    ResourceUriBuilder, ResourceUriError, ResourceUriParts, UriAuthority, UriSegment,
};
pub use resource_selector::{
    ResourceSelection, ResourceSelector, ResourceUriPrefix, ResourceUriTemplate,
};
pub use resource_template::{ResourceTemplateError, ResourceTemplateUri};

pub use identity::{
    AccessSubject, DataLabelId, DelegationId, GroupId, PolicyVersion, PrincipalId, RoleId,
    TenantId, TokenIssuer, TokenSubject, WorkContextId,
};
pub use provenance::{InvocationMode, InvocationProvenance};
pub use task::TaskId;
#[doc(hidden)]
pub use task_type::assert_task_type_names as __assert_task_type_names;
pub use task_type::{TaskTypeDefinition, TaskTypeName};

pub use platform_names::{
    CanonicalTaskId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, OAuthClientId,
    PromptName, ServerSlug,
};

mod agent_names;
pub use agent_names::{
    AgentDefinitionId, AgentIdentifierError, AgentManagedInstanceId, AgentModelId, AgentTemplateId,
};
mod authentication;
pub use authentication::{AuthMethod, AuthOutcome, AuthReasonCode};
