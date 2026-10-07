//! Protocol-independent identity, names, and resource addresses shared by Veoveo components.
//! Domain vocabularies and authorization decisions belong to their owning contracts.
//!
//! Identities with the same wire spelling remain different types:
//! ```compile_fail
//! use veoveo_types::{PrincipalId, TenantId};
//! fn assign_actor(_: PrincipalId) {}
//! assign_actor(TenantId::parse("operations").unwrap());
//! ```
//! Subject variants require the matching identity:
//! ```compile_fail
//! use veoveo_types::{AccessSubject, GroupId};
//! let subject = AccessSubject::Principal(GroupId::parse("operators").unwrap());
//! ```
//! Delegated provenance requires the identity of the delegation:
//! ```compile_fail
//! use veoveo_types::{InvocationProvenance, PrincipalId};
//! let provenance = InvocationProvenance::Delegated {
//!     initiator: PrincipalId::parse("operator").unwrap(),
//! };
//! ```

extern crate self as veoveo_types;
pub use id::Identity;
pub use veoveo_macros::{Vocabulary, embedded_document, id, resource_address};
pub mod naming;
pub use naming::{
    NAMING_PROFILE_KEY, NamingAuthority, NamingDeclaration, NamingLabel, NamingProfile,
    NamingProfileError, NamingRole, NamingSchemaContext, ScalarGrammar, ScalarNaming,
    dictionary_schema, dictionary_schema_with_key, naming_profile, scalar_schema,
    with_naming_profile,
};
mod id_profile;
pub use id_profile::*;
mod resource_profile;
pub use resource_profile::{ResourceProfile, ResourceProfileSpec, ResourceSchema};
pub use vocabulary::{Vocabulary, is_scope_token, scope_vocabulary_schema};

mod actions;
pub use actions::{ActionHandle, ActionKey, ActionName, ActionRegistry, ActionRegistryBuilder};
mod access_grant;
mod checked;
mod chrono_timestamp;
pub use chrono_timestamp::ChronoUtcTimestampSchema;
mod extensions;
pub use extensions::{
    AdmittedExtensions, ExtensionError, ExtensionKey, ExtensionName, ExtensionRegistry,
    ExtensionRegistryBuilder, UniqueJsonValue,
};
mod cursor;
pub use checked::{Check, Checked};
pub use cursor::{CursorCodec, OpaqueCursor, StatelessCursorCodec};
mod authority;
mod digest;
mod error;
mod https_url;
mod id;
pub mod identifier_syntax;
mod identity;
mod names;
mod platform_names;
mod provenance;
mod resource;
mod resource_components;
mod resource_route;
mod resource_selector;
mod resource_template;
pub mod sha256_hex;
mod task;
mod task_type;
mod vocabulary;

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
pub use resource_route::{
    IdentityResourceCodec, ResourceComponentEncoding, ResourceEncodedPattern, ResourceFieldCodec,
    ResourcePatternContext, ResourcePatternSpelling, ResourceRoute, ResourceRouteError,
    ResourceRouteMatch, ResourceRoutePolicy, ResourceTailCodec, RouteBinding, RoutePath,
    RouteQuery,
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
