use std::{collections::BTreeSet, fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::{
    AccessSubject, DataLabelId, GatewayInternalIdentity, PolicyVersion, TenantId, WorkContextId,
};

pub const LIVE_VIEW_SCHEMA: &str = "veoveo.io/live-view/v3";

fn validate_id(value: &str) -> Result<(), LiveViewIdentityError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(LiveViewIdentityError(value.to_owned()));
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
            pub fn new(value: impl Into<String>) -> Result<Self, LiveViewIdentityError> {
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
            type Err = LiveViewIdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = LiveViewIdentityError;

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

id_type!(LiveViewId);
id_type!(LiveSessionId);
id_type!(LiveCameraId);
id_type!(LiveEntityId);
id_type!(LiveViewerInstanceId);
id_type!(LiveStreamProductId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewIdentityError(String);

impl fmt::Display for LiveViewIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid live-view identity {:?}", self.0)
    }
}

impl std::error::Error for LiveViewIdentityError {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct LiveViewUri(String);

impl LiveViewUri {
    pub fn new(value: impl Into<String>) -> Result<Self, LiveViewIdentityError> {
        let value = value.into();
        let parsed = Url::parse(&value).map_err(|_| LiveViewIdentityError(value.clone()))?;
        let valid_scheme = parsed.scheme().len() <= 64
            && parsed
                .scheme()
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !matches!(parsed.scheme(), "http" | "https" | "ws" | "wss");
        let segments = parsed
            .path_segments()
            .map(Iterator::collect::<Vec<_>>)
            .unwrap_or_default();
        if !valid_scheme
            || parsed.host_str() != Some("session")
            || segments.len() != 3
            || validate_id(segments[0]).is_err()
            || segments[1] != "live-view"
            || validate_id(segments[2]).is_err()
            || parsed.username() != ""
            || parsed.password().is_some()
            || parsed.port().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || value.len() > 512
        {
            return Err(LiveViewIdentityError(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for LiveViewUri {
    type Error = LiveViewIdentityError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LiveViewUri> for String {
    fn from(value: LiveViewUri) -> Self {
        value.0
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct LiveViewAccessToken(String);

impl LiveViewAccessToken {
    pub fn new(value: impl Into<String>) -> Result<Self, LiveViewIdentityError> {
        let value = value.into();
        if !(32..=512).contains(&value.len())
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(LiveViewIdentityError("<redacted>".to_owned()));
        }
        Ok(Self(value))
    }

    pub fn expose_for_stream(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for LiveViewAccessToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("LiveViewAccessToken(<redacted>)")
    }
}

impl TryFrom<String> for LiveViewAccessToken {
    type Error = LiveViewIdentityError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<LiveViewAccessToken> for String {
    fn from(value: LiveViewAccessToken) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveViewOwner {
    pub subject: AccessSubject,
    pub tenant: TenantId,
    pub work_context: WorkContextId,
    pub policy_revision: PolicyVersion,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
}

impl LiveViewOwner {
    pub fn from_identity(identity: &GatewayInternalIdentity) -> Self {
        let mut data_labels = identity.authority.output_policy.data_labels.clone();
        data_labels.extend(identity.authority.output_policy.classification.clone());
        Self {
            subject: identity.authority.output_policy.owner.clone(),
            tenant: identity.authority.tenant.clone(),
            work_context: identity.authority.work_context.clone(),
            policy_revision: identity.authority.policy_revision.clone(),
            data_labels,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveViewLifecycle {
    Starting,
    Ready,
    Live,
    Closed,
    Failed,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LiveCameraSource {
    Fixed,
    LookAt,
    Orbit,
    FollowEntity,
    ChaseEntity,
    StabilizedMountedEntity,
    FormationOverview,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveVector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl LiveVector3 {
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveQuaternionXyzw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl LiveQuaternionXyzw {
    pub fn normalized(self) -> bool {
        let norm_squared = self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w;
        norm_squared.is_finite() && (norm_squared - 1.0).abs() <= 1.0e-6
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LivePose {
    pub position_m: LiveVector3,
    pub orientation_xyzw: LiveQuaternionXyzw,
}

impl LivePose {
    pub fn validate(self) -> Result<(), LiveCameraContractError> {
        if !self.position_m.finite() || !self.orientation_xyzw.normalized() {
            return Err(LiveCameraContractError::Pose);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveCameraSmoothing {
    pub translation_half_life_ms: u32,
    pub rotation_half_life_ms: u32,
    pub teleport_distance_millimetres: u32,
    pub reset_after_gap_ms: u32,
}

impl LiveCameraSmoothing {
    pub fn validate(self) -> Result<(), LiveCameraContractError> {
        if self.translation_half_life_ms > 60_000
            || self.rotation_half_life_ms > 60_000
            || self.teleport_distance_millimetres == 0
            || self.teleport_distance_millimetres > 100_000_000
            || self.reset_after_gap_ms == 0
            || self.reset_after_gap_ms > 600_000
        {
            return Err(LiveCameraContractError::Smoothing);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum LiveCameraRig {
    Fixed {
        pose: LivePose,
    },
    LookAt {
        eye_m: LiveVector3,
        target_m: LiveVector3,
        smoothing: LiveCameraSmoothing,
    },
    Orbit {
        target_entity_id: LiveEntityId,
        radius_m: f64,
        azimuth_degrees: f64,
        elevation_degrees: f64,
        smoothing: LiveCameraSmoothing,
    },
    FollowEntity {
        target_entity_id: LiveEntityId,
        eye_offset_flu_m: LiveVector3,
        target_offset_flu_m: LiveVector3,
        smoothing: LiveCameraSmoothing,
    },
    ChaseEntity {
        target_entity_id: LiveEntityId,
        distance_m: f64,
        height_m: f64,
        smoothing: LiveCameraSmoothing,
    },
    StabilizedMountedEntity {
        target_entity_id: LiveEntityId,
        mount: LivePose,
        smoothing: LiveCameraSmoothing,
    },
    FormationOverview {
        target_entity_ids: Vec<LiveEntityId>,
        padding_m: f64,
        smoothing: LiveCameraSmoothing,
    },
}

impl LiveCameraRig {
    pub fn source(&self) -> LiveCameraSource {
        match self {
            Self::Fixed { .. } => LiveCameraSource::Fixed,
            Self::LookAt { .. } => LiveCameraSource::LookAt,
            Self::Orbit { .. } => LiveCameraSource::Orbit,
            Self::FollowEntity { .. } => LiveCameraSource::FollowEntity,
            Self::ChaseEntity { .. } => LiveCameraSource::ChaseEntity,
            Self::StabilizedMountedEntity { .. } => LiveCameraSource::StabilizedMountedEntity,
            Self::FormationOverview { .. } => LiveCameraSource::FormationOverview,
        }
    }

    pub fn validate(&self) -> Result<(), LiveCameraContractError> {
        match self {
            Self::Fixed { pose } => pose.validate(),
            Self::LookAt {
                eye_m,
                target_m,
                smoothing,
            } => {
                smoothing.validate()?;
                if !eye_m.finite() || !target_m.finite() || eye_m == target_m {
                    return Err(LiveCameraContractError::Rig);
                }
                Ok(())
            }
            Self::Orbit {
                radius_m,
                azimuth_degrees,
                elevation_degrees,
                smoothing,
                ..
            } => {
                smoothing.validate()?;
                if !radius_m.is_finite()
                    || *radius_m <= 0.1
                    || !azimuth_degrees.is_finite()
                    || !elevation_degrees.is_finite()
                    || !(-89.9..=89.9).contains(elevation_degrees)
                {
                    return Err(LiveCameraContractError::Rig);
                }
                Ok(())
            }
            Self::FollowEntity {
                eye_offset_flu_m,
                target_offset_flu_m,
                smoothing,
                ..
            } => {
                smoothing.validate()?;
                if !eye_offset_flu_m.finite() || !target_offset_flu_m.finite() {
                    return Err(LiveCameraContractError::Rig);
                }
                Ok(())
            }
            Self::ChaseEntity {
                distance_m,
                height_m,
                smoothing,
                ..
            } => {
                smoothing.validate()?;
                if !distance_m.is_finite() || *distance_m <= 0.1 || !height_m.is_finite() {
                    return Err(LiveCameraContractError::Rig);
                }
                Ok(())
            }
            Self::StabilizedMountedEntity {
                mount, smoothing, ..
            } => {
                mount.validate()?;
                smoothing.validate()
            }
            Self::FormationOverview {
                target_entity_ids,
                padding_m,
                smoothing,
            } => {
                smoothing.validate()?;
                if target_entity_ids.is_empty()
                    || target_entity_ids.len() > 256
                    || target_entity_ids.windows(2).any(|pair| pair[0] >= pair[1])
                    || !padding_m.is_finite()
                    || *padding_m < 0.0
                {
                    return Err(LiveCameraContractError::Rig);
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveCameraStreamPolicy {
    Disabled,
    Continuous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveCameraDescriptor {
    pub camera_id: LiveCameraId,
    pub session_id: LiveSessionId,
    pub revision: u64,
    pub rig: LiveCameraRig,
    pub width_px: u32,
    pub height_px: u32,
    pub frame_rate_millihertz: u32,
    pub vertical_fov_degrees: f64,
    pub near_clip_m: f64,
    pub far_clip_m: f64,
    pub stream_policy: LiveCameraStreamPolicy,
    pub health: LiveCameraHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_at: Option<DateTime<Utc>>,
}

impl LiveCameraDescriptor {
    pub fn validate(&self) -> Result<(), LiveCameraContractError> {
        self.rig.validate()?;
        if self.revision == 0
            || self.width_px == 0
            || self.height_px == 0
            || self.frame_rate_millihertz == 0
            || !self.vertical_fov_degrees.is_finite()
            || !(1.0..=160.0).contains(&self.vertical_fov_degrees)
            || !self.near_clip_m.is_finite()
            || !self.far_clip_m.is_finite()
            || self.near_clip_m <= 0.0
            || self.far_clip_m <= self.near_clip_m
        {
            return Err(LiveCameraContractError::Descriptor);
        }
        Ok(())
    }

    pub fn pixels_per_second(&self) -> u64 {
        u64::from(self.width_px)
            .saturating_mul(u64::from(self.height_px))
            .saturating_mul(u64::from(self.frame_rate_millihertz))
            / 1_000
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveStreamProductLifecycle {
    Inactive,
    Starting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveStreamProductState {
    pub stream_product_id: LiveStreamProductId,
    pub camera_id: LiveCameraId,
    pub lifecycle: LiveStreamProductLifecycle,
    pub active_viewers: u32,
    pub connected_viewers: u32,
    pub nvenc_sessions: u32,
    pub encoded_frames: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_to_render_p95_microseconds: Option<u64>,
    pub source_to_render_samples: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveViewCapacityDimension {
    LogicalCameras,
    StreamProducts,
    RenderPixelsPerSecond,
    NvencSessions,
    GpuMemoryBytes,
    NetworkBitsPerSecond,
}

impl fmt::Display for LiveViewCapacityDimension {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LogicalCameras => "logical_cameras",
            Self::StreamProducts => "stream_products",
            Self::RenderPixelsPerSecond => "render_pixels_per_second",
            Self::NvencSessions => "nvenc_sessions",
            Self::GpuMemoryBytes => "gpu_memory_bytes",
            Self::NetworkBitsPerSecond => "network_bits_per_second",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveViewCapacityProfile {
    pub profile: String,
    pub maximum_logical_cameras: u32,
    pub maximum_stream_products: u32,
    pub maximum_render_pixels_per_second: u64,
    pub maximum_nvenc_sessions: u32,
    pub gpu_memory_budget_bytes: u64,
    pub maximum_network_bits_per_second: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveViewCapacityUsage {
    pub logical_cameras: u32,
    pub stream_products: u32,
    pub render_pixels_per_second: u64,
    pub nvenc_sessions: u32,
    pub reserved_gpu_memory_bytes: u64,
    pub connected_viewers: u32,
    pub estimated_network_bits_per_second: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveViewCapacityState {
    pub limits: LiveViewCapacityProfile,
    pub usage: LiveViewCapacityUsage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveCameraContractError {
    Pose,
    Smoothing,
    Rig,
    Descriptor,
}

impl fmt::Display for LiveCameraContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Pose => "invalid live-camera pose",
            Self::Smoothing => "invalid live-camera smoothing profile",
            Self::Rig => "invalid live-camera rig",
            Self::Descriptor => "invalid live-camera descriptor",
        })
    }
}

impl std::error::Error for LiveCameraContractError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveViewCodec {
    H264,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveViewHardwareEncoder {
    NvidiaNvenc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveColorPrimaries {
    Bt709,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveColorTransfer {
    Bt709,
    Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveColorMatrix {
    Bt709,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveColorRange {
    Limited,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveColorMetadata {
    pub primaries: LiveColorPrimaries,
    pub transfer: LiveColorTransfer,
    pub matrix: LiveColorMatrix,
    pub range: LiveColorRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveMediaTransport {
    WebSocketH264,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveMediaEndpoint {
    pub transport: LiveMediaTransport,
    pub stream_url: String,
}

/// Returns whether a public live-stream URL uses the canonical
/// credential-free secure profile or the exact-loopback development exception.
pub fn is_valid_live_stream_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let loopback = match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    };
    (matches!(url.scheme(), "https" | "wss") || (url.scheme() == "ws" && loopback))
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

impl LiveMediaEndpoint {
    pub fn validate(&self) -> Result<(), LiveViewStateError> {
        if !is_valid_live_stream_url(&self.stream_url) {
            return Err(LiveViewStateError::Endpoint);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveCameraHealth {
    Warming,
    Healthy,
    Stale,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveViewState {
    pub schema_version: String,
    pub live_view_id: LiveViewId,
    pub stream_product_id: LiveStreamProductId,
    pub resource_uri: LiveViewUri,
    pub owner: LiveViewOwner,
    pub viewer_actor: crate::PrincipalId,
    pub viewer_instance_id: LiveViewerInstanceId,
    pub session_id: LiveSessionId,
    pub camera_id: LiveCameraId,
    pub lifecycle: LiveViewLifecycle,
    pub semantic_source: LiveCameraSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_entity_id: Option<String>,
    pub camera_revision: u64,
    pub codec: LiveViewCodec,
    pub hardware_encoder: LiveViewHardwareEncoder,
    pub color: LiveColorMetadata,
    pub width_px: u32,
    pub height_px: u32,
    pub frame_rate_millihertz: u32,
    pub connected_viewers: u32,
    pub camera_health: LiveCameraHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_to_render_p95_microseconds: Option<u64>,
    pub source_to_render_samples: u64,
    pub maximum_frame_age_ms: u32,
    pub endpoint: LiveMediaEndpoint,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl LiveViewState {
    pub fn validate(&self) -> Result<(), LiveViewStateError> {
        if self.schema_version != LIVE_VIEW_SCHEMA {
            return Err(LiveViewStateError::Schema);
        }
        if self.camera_revision == 0
            || self.width_px == 0
            || self.height_px == 0
            || self.frame_rate_millihertz == 0
            || self.maximum_frame_age_ms == 0
            || self.expires_at <= self.created_at
            || matches!(
                (
                    self.source_to_render_p95_microseconds,
                    self.source_to_render_samples
                ),
                (None, 1..) | (Some(_), 0)
            )
        {
            return Err(LiveViewStateError::Limits);
        }
        self.endpoint.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveViewConnection {
    pub stream: LiveViewState,
    pub access_token: LiveViewAccessToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveViewStateError {
    Schema,
    Limits,
    Endpoint,
}

impl fmt::Display for LiveViewStateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Schema => "unsupported live-view schema",
            Self::Limits => {
                "invalid live-view dimensions, cadence, viewer, frame-age, or expiry limits"
            }
            Self::Endpoint => "invalid live-view stream endpoint",
        })
    }
}

impl std::error::Error for LiveViewStateError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_token_debug_is_redacted() {
        let token = LiveViewAccessToken::new("a".repeat(32)).unwrap();
        assert_eq!(format!("{token:?}"), "LiveViewAccessToken(<redacted>)");
        assert_eq!(token.expose_for_stream(), "a".repeat(32));
    }

    #[test]
    fn resource_uri_is_domain_owned_and_canonical() {
        assert!(LiveViewUri::new("uav-sim://session/session-a/live-view/view-a").is_ok());
        assert!(LiveViewUri::new("ground-sim://session/session-a/live-view/view-a").is_ok());
        for invalid in [
            "uav-sim://owner/session-a/live-view/view-a",
            "uav-sim://stream/stream-a",
            "https://example.test/session/session-a/live-view/view-a",
            "uav-sim://session/session-a/live-view/view-a?token=secret",
        ] {
            assert!(LiveViewUri::new(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn camera_descriptor_validates_smoothing_and_optics() {
        let descriptor = LiveCameraDescriptor {
            camera_id: LiveCameraId::new("follow").unwrap(),
            session_id: LiveSessionId::new("session-a").unwrap(),
            revision: 1,
            rig: LiveCameraRig::FollowEntity {
                target_entity_id: LiveEntityId::new("uav-1").unwrap(),
                eye_offset_flu_m: LiveVector3 {
                    x: -8.0,
                    y: 2.0,
                    z: 3.0,
                },
                target_offset_flu_m: LiveVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.2,
                },
                smoothing: LiveCameraSmoothing {
                    translation_half_life_ms: 150,
                    rotation_half_life_ms: 120,
                    teleport_distance_millimetres: 100_000,
                    reset_after_gap_ms: 1_000,
                },
            },
            width_px: 1_280,
            height_px: 720,
            frame_rate_millihertz: 30_000,
            vertical_fov_degrees: 60.0,
            near_clip_m: 0.1,
            far_clip_m: 100_000.0,
            stream_policy: LiveCameraStreamPolicy::Continuous,
            health: LiveCameraHealth::Warming,
            last_frame_at: None,
        };
        assert!(descriptor.validate().is_ok());
        assert_eq!(descriptor.pixels_per_second(), 27_648_000);

        let mut invalid = descriptor;
        invalid.rig = LiveCameraRig::FormationOverview {
            target_entity_ids: vec![
                LiveEntityId::new("uav-2").unwrap(),
                LiveEntityId::new("uav-1").unwrap(),
            ],
            padding_m: 10.0,
            smoothing: LiveCameraSmoothing {
                translation_half_life_ms: 150,
                rotation_half_life_ms: 120,
                teleport_distance_millimetres: 100_000,
                reset_after_gap_ms: 1_000,
            },
        };
        assert_eq!(invalid.validate(), Err(LiveCameraContractError::Rig));
    }

    #[test]
    fn stream_url_accepts_secure_or_exact_loopback_transports() {
        for url in [
            "https://views.example.test/live",
            "wss://views.example.test/live",
            "ws://localhost:8782/live",
            "ws://LOCALHOST:8782/live",
            "ws://127.0.0.1:8782/live",
            "ws://127.255.255.254:8782/live",
            "ws://[::1]:8782/live",
        ] {
            assert!(is_valid_live_stream_url(url), "{url}");
        }
    }

    #[test]
    fn stream_url_rejects_insecure_or_ambiguous_transports() {
        for url in [
            "http://localhost:8782/live",
            "ws://localhost.example:8782/live",
            "ws://128.0.0.1:8782/live",
            "ws://192.0.2.1:8782/live",
            "ws://user@localhost:8782/live",
            "ws://localhost:8782/live?token=secret",
            "ws://localhost:8782/live#fragment",
            "not a URL",
        ] {
            assert!(!is_valid_live_stream_url(url), "{url}");
        }
    }

    #[test]
    fn media_endpoint_requires_a_canonical_stream_url() {
        let endpoint: LiveMediaEndpoint = serde_json::from_value(serde_json::json!({
            "transport": "web_socket_h264",
            "streamUrl": "wss://views.example.test/live"
        }))
        .unwrap();
        assert!(endpoint.validate().is_ok());

        for stream_url in [
            "ws://views.example.test/live",
            "wss://user@views.example.test/live",
            "wss://views.example.test/live?token=secret",
        ] {
            let result = serde_json::from_value::<LiveMediaEndpoint>(serde_json::json!({
                "transport": "web_socket_h264",
                "streamUrl": stream_url
            }));
            assert!(result.unwrap().validate().is_err(), "{stream_url}");
        }
    }
}
