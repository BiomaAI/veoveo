//! Recording identities and redacted admission errors.
use serde::{Deserialize, Serialize};
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

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(error=RecordingContractError,admit=admit_recording_id,wire_string,constructor=parse,generate=uuid::Uuid::now_v7,schema=String::json_schema,schema_inline)]
pub struct RecordingId(uuid::Uuid);
impl RecordingId {
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl TryFrom<uuid::Uuid> for RecordingId {
    type Error = RecordingContractError;
    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        validate_recording_uuid(value).map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(error=RecordingContractError,admit=admit_recording_id,wire_string,constructor=parse,generate=uuid::Uuid::now_v7,schema=String::json_schema,schema_inline)]
pub struct RecordingDatasetId(uuid::Uuid);
impl RecordingDatasetId {
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl TryFrom<uuid::Uuid> for RecordingDatasetId {
    type Error = RecordingContractError;
    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        validate_recording_uuid(value).map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(error=RecordingContractError,admit=admit_recording_id,wire_string,constructor=parse,generate=uuid::Uuid::now_v7,schema=String::json_schema,schema_inline)]
pub struct RecordingLayerId(uuid::Uuid);
impl RecordingLayerId {
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl TryFrom<uuid::Uuid> for RecordingLayerId {
    type Error = RecordingContractError;
    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        validate_recording_uuid(value).map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(error=RecordingContractError,admit=admit_recording_id,wire_string,constructor=parse,generate=uuid::Uuid::now_v7,schema=String::json_schema,schema_inline)]
pub struct RecordingReadGrantId(uuid::Uuid);
impl RecordingReadGrantId {
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl TryFrom<uuid::Uuid> for RecordingReadGrantId {
    type Error = RecordingContractError;
    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        validate_recording_uuid(value).map(Self)
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(error=RecordingContractError,admit=admit_recording_id,wire_string,constructor=parse,generate=uuid::Uuid::now_v7,schema=String::json_schema,schema_inline)]
pub struct RecordingProjectionId(uuid::Uuid);
impl RecordingProjectionId {
    pub fn as_uuid(self) -> uuid::Uuid {
        self.0
    }
}
impl TryFrom<uuid::Uuid> for RecordingProjectionId {
    type Error = RecordingContractError;
    fn try_from(value: uuid::Uuid) -> Result<Self, Self::Error> {
        validate_recording_uuid(value).map(Self)
    }
}

fn validate_recording_uuid(value: uuid::Uuid) -> Result<uuid::Uuid, RecordingContractError> {
    if value.get_version_num() != 7 || value.get_variant() != uuid::Variant::RFC4122 {
        return Err(RecordingContractError::Identity);
    }
    Ok(value)
}
fn admit_recording_id(value: &str) -> Result<uuid::Uuid, RecordingContractError> {
    let id = uuid::Uuid::parse_str(value).map_err(|_| RecordingContractError::Identity)?;
    let id = validate_recording_uuid(id)?;
    if id.to_string() != value {
        return Err(RecordingContractError::Identity);
    }
    Ok(id)
}
