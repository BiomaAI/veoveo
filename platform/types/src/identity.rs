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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
#[serde(deny_unknown_fields)]
pub enum AccessSubject {
    Principal(PrincipalId),
    Group(GroupId),
}

impl<'de> Deserialize<'de> for AccessSubject {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Map fields reject unknown keys explicitly; sequences retain the
        // adjacent enum's identifier visitor and its binary admission profile.
        #[derive(Deserialize)]
        #[serde(rename = "AccessSubject", deny_unknown_fields)]
        struct SubjectWire {
            kind: SubjectKind,
            id: String,
        }
        #[derive(Deserialize)]
        #[serde(rename = "AccessSubject", rename_all = "snake_case")]
        enum SubjectKind {
            Principal,
            Group,
        }

        #[derive(Deserialize)]
        #[serde(
            rename = "AccessSubject",
            rename_all = "snake_case",
            expecting = "adjacently tagged enum AccessSubject",
            tag = "kind",
            content = "id",
            deny_unknown_fields
        )]
        enum SubjectSequence {
            Principal(PrincipalId),
            Group(GroupId),
        }
        struct SubjectVisitor;
        impl<'de> serde::de::Visitor<'de> for SubjectVisitor {
            type Value = AccessSubject;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("adjacently tagged enum AccessSubject")
            }

            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let subject =
                    SubjectWire::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                match subject.kind {
                    SubjectKind::Principal => {
                        PrincipalId::parse(subject.id).map(AccessSubject::Principal)
                    }
                    SubjectKind::Group => GroupId::parse(subject.id).map(AccessSubject::Group),
                }
                .map_err(serde::de::Error::custom)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                sequence: A,
            ) -> Result<Self::Value, A::Error> {
                SubjectSequence::deserialize(serde::de::value::SeqAccessDeserializer::new(sequence))
                    .map(|subject| match subject {
                        SubjectSequence::Principal(id) => AccessSubject::Principal(id),
                        SubjectSequence::Group(id) => AccessSubject::Group(id),
                    })
            }
        }
        deserializer.deserialize_struct("AccessSubject", &["kind", "id"], SubjectVisitor)
    }
}

#[doc = "Expected token issuer identifier."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct TokenIssuer(String);
#[doc = "AccessSubject claim from an authenticated access token or identity assertion."]
#[veoveo_types::id(text(crate::identifier_syntax::ClaimTextProfile))]
pub struct TokenSubject(String);
