use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[doc = "Policy data label such as `cui`, `itar`, `pii`, or an IdP-provided clearance label."]
#[veoveo_types::id(text(crate::identifier_syntax::TokenTextProfile))]
pub struct DataLabelId(String);

#[doc = "Stable authenticated user or service-principal identity."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct PrincipalId(String);

#[doc = "Tenant, organization, or customer boundary identifier."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct TenantId(String);

#[doc = "Tenant-local boundary that governs related work and every output it produces."]
#[veoveo_types::id(text(crate::identifier_syntax::PathIdProfile))]
pub struct WorkContextId(String);

#[doc = "Auditable identity of authority delegated by an initiator to another actor."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct DelegationId(String);

#[doc = "Identity-provider group identifier used by gateway policy."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct GroupId(String);

#[doc = "Identity-provider role identifier used by gateway policy."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct RoleId(String);

#[doc = "Immutable policy version identifier emitted with decisions and audit records."]
#[veoveo_types::id(text(crate::identifier_syntax::TokenTextProfile))]
pub struct PolicyVersion(String);

/// A principal or group that can own governed data or receive access.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum AccessSubject {
    Principal(PrincipalId),
    Group(GroupId),
}

#[doc = "Expected token issuer identifier."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct TokenIssuer(String);
#[doc = "AccessSubject claim from an authenticated access token or identity assertion."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct TokenSubject(String);
