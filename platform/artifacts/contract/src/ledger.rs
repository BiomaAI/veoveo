//! Private Artifact ledger identities and addresses shared by service adapters.
use crate::ArtifactShareLinkId;

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

#[veoveo_types::id(uuid(ArtifactWriteCapabilityIdProfile), fresh, error_context = concat!("artifact write capability id", " must be an RFC UUIDv7"))]
pub struct ArtifactWriteCapabilityId(uuid::Uuid);
#[veoveo_types::id(uuid(ArtifactWriteCapabilityIdProfile), fresh, error_context = concat!("artifact read capability id", " must be an RFC UUIDv7"))]
pub struct ArtifactReadCapabilityId(uuid::Uuid);
#[veoveo_types::id(uuid(ArtifactWriteCapabilityIdProfile), fresh, error_context = concat!("artifact task id", " must be an RFC UUIDv7"))]
pub struct ArtifactTaskId(uuid::Uuid);
#[veoveo_types::id(uuid(ArtifactWriteCapabilityIdProfile), fresh, error_context = concat!("artifact access request id", " must be an RFC UUIDv7"))]
pub struct ArtifactAccessRequestId(uuid::Uuid);
#[veoveo_types::id(uuid(ArtifactWriteCapabilityIdProfile), fresh, error_context = concat!("artifact upload id", " must be an RFC UUIDv7"))]
pub struct ArtifactUploadId(uuid::Uuid);
#[veoveo_types::id(uuid(ArtifactWriteCapabilityIdProfile), fresh, error_context = concat!("upload idempotency key", " must be an RFC UUIDv7"))]
pub struct ArtifactUploadRequestId(uuid::Uuid);
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

use veoveo_types::{IdProfile, IdProfileSpec, UuidGrammar};

#[doc(hidden)]
pub struct ArtifactWriteCapabilityIdProfile;
impl IdProfile for ArtifactWriteCapabilityIdProfile {
    type Error = ArtifactLedgerIdError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::generated_uuid(UuidGrammar::aliases(&[7]), |_, metadata, _| {
            ArtifactLedgerIdError(metadata.error_context)
        });
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
