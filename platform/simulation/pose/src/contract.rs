use std::{fmt, str::FromStr, time::Duration};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const POSE_PROTOCOL_VERSION: u16 = 1;
pub const POSE_PROTOCOL_SCHEMA: &str = "veoveo.io/simulation-view-pose/v1";

fn validate_id(value: &str) -> Result<(), PoseError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(PoseError::InvalidIdentifier(value.to_owned()));
    }
    Ok(())
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, PoseError> {
                let value = value.into();
                validate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = PoseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = PoseError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

id_type!(SessionId);
id_type!(EpochId);
id_type!(EntityId);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn new(value: impl Into<String>) -> Result<Self, PoseError> {
        let value = value.into();
        if value.len() != 71
            || !value.starts_with("sha256:")
            || !value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
            || value[7..].bytes().any(|byte| byte.is_ascii_uppercase())
        {
            return Err(PoseError::InvalidDigest(value));
        }
        Ok(Self(value))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        let mut encoded = String::with_capacity(71);
        encoded.push_str("sha256:");
        for byte in bytes {
            use fmt::Write as _;
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Self(encoded)
    }

    pub fn as_bytes(&self) -> [u8; 32] {
        let mut bytes = [0_u8; 32];
        for (index, pair) in self.0[7..].as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
        }
        bytes
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Sha256Digest {
    type Error = PoseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Sha256Digest> for String {
    fn from(value: Sha256Digest) -> Self {
        value.0
    }
}

fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => unreachable!("validated hexadecimal digest"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FrameRevision {
    pub uri: String,
    pub digest: Sha256Digest,
}

impl FrameRevision {
    pub fn validate(&self) -> Result<(), PoseError> {
        if !self.uri.starts_with("frames://world/") || self.uri.len() > 512 {
            return Err(PoseError::InvalidFrameRevision(self.uri.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateConvention {
    EnuMetersFluXyzw,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnuPosition {
    pub east_m: f64,
    pub north_m: f64,
    pub up_m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuaternionXyzw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FluVelocity {
    pub forward_mps: f32,
    pub left_mps: f32,
    pub up_mps: f32,
    pub roll_rps: f32,
    pub pitch_rps: f32,
    pub yaw_rps: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rgba8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SemanticDisplayState {
    pub color: Rgba8,
    pub status_code: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EntityPose {
    pub entity_id: EntityId,
    pub position: EnuPosition,
    pub orientation: QuaternionXyzw,
    pub active: bool,
    pub visible: bool,
    pub velocity: Option<FluVelocity>,
    pub display: Option<SemanticDisplayState>,
}

impl EntityPose {
    pub fn validate(&self) -> Result<(), PoseError> {
        let position = [
            self.position.east_m,
            self.position.north_m,
            self.position.up_m,
        ];
        if position.into_iter().any(|value| !value.is_finite()) {
            return Err(PoseError::NonFinite {
                entity: self.entity_id.clone(),
                field: "position",
            });
        }
        let quaternion = [
            self.orientation.x,
            self.orientation.y,
            self.orientation.z,
            self.orientation.w,
        ];
        if quaternion.into_iter().any(|value| !value.is_finite()) {
            return Err(PoseError::NonFinite {
                entity: self.entity_id.clone(),
                field: "orientation",
            });
        }
        let norm_squared = quaternion
            .into_iter()
            .map(|value| value * value)
            .sum::<f64>();
        if (norm_squared - 1.0).abs() > 1.0e-3 {
            return Err(PoseError::InvalidQuaternion(self.entity_id.clone()));
        }
        if let Some(velocity) = self.velocity {
            let values = [
                velocity.forward_mps,
                velocity.left_mps,
                velocity.up_mps,
                velocity.roll_rps,
                velocity.pitch_rps,
                velocity.yaw_rps,
            ];
            if values.into_iter().any(|value| !value.is_finite()) {
                return Err(PoseError::NonFinite {
                    entity: self.entity_id.clone(),
                    field: "velocity",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PoseSnapshot {
    pub protocol_version: u16,
    pub session_id: SessionId,
    pub epoch_id: EpochId,
    pub sequence: u64,
    pub simulation_timestamp_ns: i64,
    pub frame_revision: FrameRevision,
    pub coordinate_convention: CoordinateConvention,
    pub entity_table_revision: u64,
    pub entity_table_digest: Sha256Digest,
    pub entities: Vec<EntityPose>,
}

impl PoseSnapshot {
    pub fn validate(&self, limits: &PoseLimits) -> Result<(), PoseError> {
        if self.protocol_version != POSE_PROTOCOL_VERSION {
            return Err(PoseError::UnsupportedVersion(self.protocol_version));
        }
        if self.sequence == 0 || self.simulation_timestamp_ns < 0 {
            return Err(PoseError::InvalidSequenceOrTimestamp);
        }
        self.frame_revision.validate()?;
        if self.entities.is_empty() || self.entities.len() > limits.max_entities {
            return Err(PoseError::EntityCount {
                count: self.entities.len(),
                maximum: limits.max_entities,
            });
        }
        let mut previous: Option<&EntityId> = None;
        for entity in &self.entities {
            if previous.is_some_and(|prior| prior >= &entity.entity_id) {
                return Err(PoseError::EntityOrder);
            }
            entity.validate()?;
            previous = Some(&entity.entity_id);
        }
        let computed = entity_table_digest(self.entity_table_revision, &self.entities);
        if computed != self.entity_table_digest {
            return Err(PoseError::EntityTableDigest);
        }
        Ok(())
    }
}

pub fn entity_table_digest(revision: u64, entities: &[EntityPose]) -> Sha256Digest {
    entity_identity_table_digest(revision, entities.iter().map(|entity| &entity.entity_id))
}

pub fn entity_identity_table_digest<'a>(
    revision: u64,
    entities: impl IntoIterator<Item = &'a EntityId>,
) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(revision.to_be_bytes());
    for entity in entities {
        hasher.update((entity.as_str().len() as u16).to_be_bytes());
        hasher.update(entity.as_str().as_bytes());
    }
    Sha256Digest::from_bytes(hasher.finalize().into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoseBinding {
    pub session_id: SessionId,
    pub epoch_id: EpochId,
    pub frame_revision: FrameRevision,
    pub entity_table_revision: u64,
    pub entity_table_digest: Sha256Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoseLimits {
    pub max_entities: usize,
    pub max_message_bytes: usize,
    pub max_cadence_hz: u32,
    pub stale_after: Duration,
}

impl Default for PoseLimits {
    fn default() -> Self {
        Self {
            max_entities: 10_000,
            max_message_bytes: 4 * 1024 * 1024,
            max_cadence_hz: 120,
            stale_after: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoseError {
    #[error("invalid pose identity {0:?}")]
    InvalidIdentifier(String),
    #[error("invalid SHA-256 digest {0:?}")]
    InvalidDigest(String),
    #[error("unsupported pose protocol version {0}")]
    UnsupportedVersion(u16),
    #[error("invalid sequence or simulation timestamp")]
    InvalidSequenceOrTimestamp,
    #[error("invalid Frames revision URI {0:?}")]
    InvalidFrameRevision(String),
    #[error("pose snapshot contains {count} entities; maximum is {maximum}")]
    EntityCount { count: usize, maximum: usize },
    #[error("entity identities must be strictly ordered and unique")]
    EntityOrder,
    #[error("entity table digest does not match the ordered identities")]
    EntityTableDigest,
    #[error("entity {entity} contains non-finite {field}")]
    NonFinite {
        entity: EntityId,
        field: &'static str,
    },
    #[error("entity {0} quaternion is not normalized")]
    InvalidQuaternion(EntityId),
    #[error("pose message is {actual} bytes; maximum is {maximum}")]
    MessageBytes { actual: usize, maximum: usize },
    #[error("pose message is truncated")]
    Truncated,
    #[error("pose message has invalid magic")]
    InvalidMagic,
    #[error("pose message has trailing bytes")]
    TrailingBytes,
    #[error("pose snapshot does not match authorized {field}")]
    BindingMismatch { field: &'static str },
    #[error("pose cadence exceeds the admitted maximum")]
    CadenceExceeded,
    #[error("shared pose slot is invalid: {0}")]
    SharedSlot(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
