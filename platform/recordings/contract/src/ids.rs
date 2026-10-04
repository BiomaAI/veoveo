//! Recording identities and redacted admission errors.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingContractError {
    Identity,
    Resource,
    Cursor,
    Selection,
    Playback,
    RedapAddress,
    RedapLoopbackPort,
    CatalogGrant,
    Layer,
    CatalogView,
    Seal,
    ProjectionBounds,
    ProjectionSelection,
    ProjectionSampling,
    ProjectionMetadata,
    ProjectionResult,
}
impl fmt::Display for RecordingContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Identity => "Recording identity must be a canonical RFC UUIDv7",
            Self::Resource => "invalid Recording resource address",
            Self::Cursor => "invalid Recording catalog cursor",
            Self::Selection => "Recording catalog selection requires 1 to 500 recording IDs",
            Self::Playback => "invalid Recording playback manifest",
            Self::RedapAddress => "invalid Recording Redap address or HTTP(S) origin",
            Self::RedapLoopbackPort => "Rerun 0.38.1 rewrites loopback HTTP(S) default ports; configure an explicit nondefault port or a public host",
            Self::Layer => "invalid Recording layer identity, lifecycle or integrity metadata",
            Self::CatalogView => "invalid Recording catalog view or lifecycle relationships",
            Self::Seal => "invalid Recording seal metadata or repeated Artifact occurrence",
            Self::CatalogGrant => "invalid Recording catalog grant or dataset relationship",
            Self::ProjectionBounds => {
                "Recording projection limits must be positive and within the published bounds"
            }
            Self::ProjectionSelection => {
                "Recording projection selectors must be nonempty, unique and at most 1024 bytes"
            }
            Self::ProjectionSampling => {
                "Recording projection sampling must be ordered, temporal and within maximum_samples"
            }
            Self::ProjectionMetadata => {
                "invalid Recording projection deadline, idempotency key or result metadata"
            }
            Self::ProjectionResult => "Recording projection result has invalid bounds, sample counts or request relationships",
        })
    }
}
impl std::error::Error for RecordingContractError {}

pub(super) fn string_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <String as schemars::JsonSchema>::json_schema(generator)
}

#[veoveo_types::id(uuid(RecordingIds), fresh)]
pub struct RecordingId(uuid::Uuid);
#[veoveo_types::id(uuid(RecordingIds), fresh)]
pub struct RecordingDatasetId(uuid::Uuid);
#[veoveo_types::id(uuid(RecordingIds), fresh)]
pub struct RecordingLayerId(uuid::Uuid);
#[veoveo_types::id(uuid(RecordingIds), fresh)]
pub struct RecordingReadGrantId(uuid::Uuid);
#[veoveo_types::id(uuid(RecordingIds), fresh)]
pub struct RecordingProjectionId(uuid::Uuid);
use veoveo_types::{
    FreshId, IdGeneration, IdProfile, IdProfileSpec, IdSchema, UuidGrammar, UuidSpelling,
    UuidVariant,
};

#[doc(hidden)]
pub struct RecordingIds;
impl IdProfile for RecordingIds {
    type Error = RecordingContractError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        generation: IdGeneration {
            fresh: FreshId::UuidV7,
            stable_v5_namespace: None,
        },
        schema: IdSchema::Owner {
            schema: |generator, _| string_schema(generator),
            inline: true,
        },
        ..IdProfileSpec::uuid(
            UuidGrammar {
                versions: &[7],
                variant: UuidVariant::Rfc4122,
                spelling: UuidSpelling::CanonicalLowerHyphenated,
            },
            |_, _, _| RecordingContractError::Identity,
        )
    };
}
