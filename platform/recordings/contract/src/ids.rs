//! Recording identities and redacted admission errors.
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

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

macro_rules! string_schema {
    ($name:ident) => {
        impl schemars::JsonSchema for $name {
            fn inline_schema() -> bool {
                true
            }
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <String as schemars::JsonSchema>::json_schema(g)
            }
        }
    };
}
pub(super) use string_schema;

macro_rules! recording_identity {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(uuid::Uuid);
        impl $name {
            pub fn new() -> Self {
                Self(uuid::Uuid::now_v7())
            }
            pub fn parse(value: impl AsRef<str>) -> Result<Self, RecordingContractError> {
                let value = value.as_ref();
                let uuid =
                    uuid::Uuid::parse_str(value).map_err(|_| RecordingContractError::Identity)?;
                let id = Self::try_from(uuid)?;
                if id.to_string() != value {
                    return Err(RecordingContractError::Identity);
                }
                Ok(id)
            }
            pub fn as_uuid(self) -> uuid::Uuid {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl TryFrom<uuid::Uuid> for $name {
            type Error = RecordingContractError;
            fn try_from(uuid: uuid::Uuid) -> Result<Self, Self::Error> {
                if uuid.get_version_num() != 7 || uuid.get_variant() != uuid::Variant::RFC4122 {
                    return Err(RecordingContractError::Identity);
                }
                Ok(Self(uuid))
            }
        }
        impl TryFrom<String> for $name {
            type Error = RecordingContractError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl FromStr for $name {
            type Err = RecordingContractError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.to_string()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        string_schema!($name);
    };
}
recording_identity!(RecordingId);
recording_identity!(RecordingDatasetId);
recording_identity!(RecordingLayerId);
recording_identity!(RecordingReadGrantId);
recording_identity!(RecordingProjectionId);
