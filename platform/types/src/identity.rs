use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id, validate_token_text},
    names::name,
};

name!(
    DataLabelId,
    validate_token_text,
    "Policy data label such as `cui`, `itar`, `pii`, or an IdP-provided clearance label."
);

name!(
    PrincipalId,
    validate_claim_text,
    "Stable authenticated user or service-principal identity."
);

name!(
    TenantId,
    validate_claim_text,
    "Tenant, organization, or customer boundary identifier."
);

name!(
    WorkContextId,
    validate_path_id,
    "Tenant-local boundary that governs related work and every output it produces."
);

name!(
    DelegationId,
    validate_claim_text,
    "Auditable identity of authority delegated by an initiator to another actor."
);

name!(
    GroupId,
    validate_claim_text,
    "Identity-provider group identifier used by gateway policy."
);

name!(
    RoleId,
    validate_claim_text,
    "Identity-provider role identifier used by gateway policy."
);

name!(
    PolicyVersion,
    validate_token_text,
    "Immutable policy version identifier emitted with decisions and audit records."
);

/// A principal or group that can own governed data or receive access.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum AccessSubject {
    Principal(PrincipalId),
    Group(GroupId),
}

name!(
    TokenIssuer,
    validate_claim_text,
    "Expected token issuer identifier."
);
name!(
    TokenSubject,
    validate_claim_text,
    "AccessSubject claim from an authenticated access token or identity assertion."
);
