//! Private Artifact ledger identities and addresses shared by service adapters.
use crate::ArtifactShareLinkId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{ResourceUri, ResourceUriBuilder, UriSegment};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactLedgerIdError(&'static str);
impl fmt::Display for ArtifactLedgerIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ArtifactLedgerIdError {}

macro_rules! artifact_uuid_id {
    ($name:ident, $label:literal) => {
        #[derive(
            Clone,
            Copy,
            Debug,
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
        pub struct $name(uuid::Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }

            pub fn parse(value: impl AsRef<str>) -> Result<Self, ArtifactLedgerIdError> {
                let value = uuid::Uuid::parse_str(value.as_ref()).map_err(|_| {
                    ArtifactLedgerIdError(concat!($label, " must be an RFC UUIDv7"))
                })?;
                if value.get_version_num() != 7 || value.get_variant() != uuid::Variant::RFC4122 {
                    return Err(ArtifactLedgerIdError(concat!(
                        $label,
                        " must be an RFC UUIDv7"
                    )));
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ArtifactLedgerIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
    };
}

artifact_uuid_id!(ArtifactWriteCapabilityId, "artifact write capability id");
artifact_uuid_id!(ArtifactReadCapabilityId, "artifact read capability id");
artifact_uuid_id!(ArtifactTaskId, "artifact task id");

artifact_uuid_id!(ArtifactAccessRequestId, "artifact access request id");
artifact_uuid_id!(ArtifactUploadId, "artifact upload id");
artifact_uuid_id!(ArtifactUploadRequestId, "upload idempotency key");

/// Private platform addresses identify ledger objects without claiming an MCP route.
#[derive(Clone, Copy, Debug)]
pub enum ArtifactLedgerAddress {
    Collection,
    AccessRequests,
    AccessRequest(ArtifactAccessRequestId),
    ReadCapability(ArtifactReadCapabilityId),
    WriteCapability(ArtifactWriteCapabilityId),
    Shares,
    Share(ArtifactShareLinkId),
}

impl ArtifactLedgerAddress {
    pub fn uri(self) -> ResourceUri {
        let (route, id) = match self {
            Self::Collection => ("veoveo://artifact-plane/artifacts", None),
            Self::AccessRequests => ("veoveo://artifact-plane/access-requests", None),
            Self::AccessRequest(id) => (
                "veoveo://artifact-plane/access-requests",
                Some(id.as_uuid()),
            ),
            Self::ReadCapability(id) => (
                "veoveo://artifact-plane/read-capabilities",
                Some(id.as_uuid()),
            ),
            Self::WriteCapability(id) => (
                "veoveo://artifact-plane/write-capabilities",
                Some(id.as_uuid()),
            ),
            Self::Shares => ("veoveo://artifact-plane/shares", None),
            Self::Share(id) => ("veoveo://artifact-plane/shares", Some(id.as_uuid())),
        };
        let mut builder = ResourceUriBuilder::new(route).expect("declared Artifact ledger route");
        if let Some(id) = id {
            builder = builder.segment(UriSegment::new(id.to_string()).expect("UUID segment"));
        }
        builder.build().expect("typed Artifact ledger address")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_addresses_preserve_typed_identity_and_uuid_admission() {
        let request = ArtifactAccessRequestId::new();
        assert_eq!(
            ArtifactLedgerAddress::AccessRequest(request).uri().as_str(),
            format!("veoveo://artifact-plane/access-requests/{request}")
        );
        assert_eq!(
            ArtifactLedgerAddress::AccessRequests.uri().as_str(),
            "veoveo://artifact-plane/access-requests"
        );
        let wrong_variant = "01994aa7-a220-7000-c000-000000000001";
        assert!(ArtifactAccessRequestId::parse(wrong_variant).is_err());
        assert!(ArtifactReadCapabilityId::parse(wrong_variant).is_err());
        assert!(ArtifactWriteCapabilityId::parse(wrong_variant).is_err());
        assert!(ArtifactTaskId::parse(wrong_variant).is_err());
        assert_eq!(
            serde_json::from_str::<ArtifactAccessRequestId>(
                &serde_json::to_string(&request).unwrap()
            )
            .unwrap(),
            request
        );
    }
}
