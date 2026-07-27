use std::{collections::BTreeSet, fmt, net::IpAddr, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::{Host, Url};

use crate::{
    DataLabelId, GatewayInternalIdentity, GatewayProfileId, InvocationAuthority, PrincipalId,
    TenantId,
};

pub const LIVE_VIEW_SCHEMA: &str = "veoveo.io/live-view/v1";

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
        if !value.starts_with("simulation-view://session/")
            || !value.contains("/stream/")
            || value.len() > 512
            || value.chars().any(char::is_whitespace)
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

    pub fn expose_for_signaling(&self) -> &str {
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
    pub principal: PrincipalId,
    pub tenant: TenantId,
    pub profile: GatewayProfileId,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    pub authority: InvocationAuthority,
}

impl LiveViewOwner {
    pub fn from_identity(identity: &GatewayInternalIdentity) -> Self {
        Self {
            principal: identity.actor.id.clone(),
            tenant: identity.authority.tenant.clone(),
            profile: identity.profile.clone(),
            data_labels: identity.actor.data_labels.clone(),
            authority: identity.authority.clone(),
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
    MountedEntity,
    FormationOverview,
}

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
    WebRtc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LiveMediaEndpoint {
    pub transport: LiveMediaTransport,
    pub signaling_url: String,
    pub media_host: IpAddr,
    pub media_port: u16,
}

/// Returns whether a public live-view signaling URL uses the canonical
/// credential-free secure profile or the exact-loopback development exception.
pub fn is_valid_live_signaling_url(value: &str) -> bool {
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
        if !is_valid_live_signaling_url(&self.signaling_url)
            || self.media_host.is_unspecified()
            || self.media_host.is_multicast()
            || self.media_port == 0
        {
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
    pub resource_uri: LiveViewUri,
    pub owner: LiveViewOwner,
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
    pub viewer_limit: u32,
    pub camera_health: LiveCameraHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_at: Option<DateTime<Utc>>,
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
            || self.viewer_limit == 0
            || self.connected_viewers > self.viewer_limit
            || self.maximum_frame_age_ms == 0
            || self.expires_at <= self.created_at
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
            Self::Endpoint => "invalid live-view signaling or media endpoint",
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
        assert_eq!(token.expose_for_signaling(), "a".repeat(32));
    }

    #[test]
    fn resource_uri_is_simulation_view_owned() {
        assert!(LiveViewUri::new("simulation-view://session/session-a/stream/stream-a").is_ok());
        assert!(LiveViewUri::new("uav-sim://stream/stream-a").is_err());
    }

    #[test]
    fn signaling_url_accepts_secure_or_exact_loopback_transports() {
        for url in [
            "https://views.example.test/signaling",
            "wss://views.example.test/signaling",
            "ws://localhost:8782/signaling",
            "ws://LOCALHOST:8782/signaling",
            "ws://127.0.0.1:8782/signaling",
            "ws://127.255.255.254:8782/signaling",
            "ws://[::1]:8782/signaling",
        ] {
            assert!(is_valid_live_signaling_url(url), "{url}");
        }
    }

    #[test]
    fn signaling_url_rejects_insecure_or_ambiguous_transports() {
        for url in [
            "http://localhost:8782/signaling",
            "ws://localhost.example:8782/signaling",
            "ws://128.0.0.1:8782/signaling",
            "ws://192.0.2.1:8782/signaling",
            "ws://user@localhost:8782/signaling",
            "ws://localhost:8782/signaling?token=secret",
            "ws://localhost:8782/signaling#fragment",
            "not a URL",
        ] {
            assert!(!is_valid_live_signaling_url(url), "{url}");
        }
    }

    #[test]
    fn media_endpoint_requires_a_numeric_unicast_address() {
        let endpoint: LiveMediaEndpoint = serde_json::from_value(serde_json::json!({
            "transport": "web_rtc",
            "signalingUrl": "wss://views.example.test/signaling",
            "mediaHost": "192.0.2.10",
            "mediaPort": 47998
        }))
        .unwrap();
        assert!(endpoint.validate().is_ok());

        for host in ["media.example.test", "0.0.0.0", "224.0.0.1", "::"] {
            let result = serde_json::from_value::<LiveMediaEndpoint>(serde_json::json!({
                "transport": "web_rtc",
                "signalingUrl": "wss://views.example.test/signaling",
                "mediaHost": host,
                "mediaPort": 47998
            }));
            match result {
                Ok(endpoint) => assert!(endpoint.validate().is_err(), "{host}"),
                Err(_) => assert_eq!(host, "media.example.test"),
            }
        }
    }
}
