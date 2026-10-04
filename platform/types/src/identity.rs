use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id, validate_token_text},
};

#[doc = "Policy data label such as `cui`, `itar`, `pii`, or an IdP-provided clearance label."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_token_text)]
pub struct DataLabelId(String);

#[doc = "Stable authenticated user or service-principal identity."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct PrincipalId(String);

#[doc = "Tenant, organization, or customer boundary identifier."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct TenantId(String);

#[doc = "Tenant-local boundary that governs related work and every output it produces."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_path_id)]
pub struct WorkContextId(String);

#[doc = "Auditable identity of authority delegated by an initiator to another actor."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct DelegationId(String);

#[doc = "Identity-provider group identifier used by gateway policy."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct GroupId(String);

#[doc = "Identity-provider role identifier used by gateway policy."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct RoleId(String);

#[doc = "Immutable policy version identifier emitted with decisions and audit records."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_token_text)]
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
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct TokenIssuer(String);
#[doc = "AccessSubject claim from an authenticated access token or identity assertion."]
#[derive(
    veoveo_types::Id,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct TokenSubject(String);
