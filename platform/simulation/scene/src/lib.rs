//! Provider-neutral governed scene declaration for Simulation View.

use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    LiveCameraSource, LiveSessionId, WorldFrameUri, parse_artifact_plane_uri,
};
use veoveo_simulation_pose::{EntityId, EpochId, FrameRevision, Sha256Digest};

/// Canonical Simulation View scene schema.
pub const SCENE_SCHEMA: &str = "veoveo.io/simulation-view-scene/v1";

fn validate_id(value: &str) -> Result<(), SceneContractError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(SceneContractError::InvalidIdentifier(value.to_owned()));
    }
    Ok(())
}

/// Stable identifier for one reusable visual prototype.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct PrototypeId(String);

impl PrototypeId {
    pub fn new(value: impl Into<String>) -> Result<Self, SceneContractError> {
        let value = value.into();
        validate_id(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrototypeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for PrototypeId {
    type Err = SceneContractError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for PrototypeId {
    type Error = SceneContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PrototypeId> for String {
    fn from(value: PrototypeId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuaternionXyzw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalTransform {
    pub translation_m: Vector3,
    pub orientation_xyzw: QuaternionXyzw,
    pub scale: Vector3,
}

impl LocalTransform {
    pub fn validate(&self) -> Result<(), SceneContractError> {
        let quaternion = [
            self.orientation_xyzw.x,
            self.orientation_xyzw.y,
            self.orientation_xyzw.z,
            self.orientation_xyzw.w,
        ];
        let norm = quaternion
            .into_iter()
            .map(|value| value * value)
            .sum::<f64>();
        if !self.translation_m.finite()
            || !self.scale.finite()
            || self.scale.x <= 0.0
            || self.scale.y <= 0.0
            || self.scale.z <= 0.0
            || quaternion.into_iter().any(|value| !value.is_finite())
            || (norm - 1.0).abs() > 1.0e-3
        {
            return Err(SceneContractError::InvalidTransform);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualAssetFormat {
    Usd,
    Usdz,
    Glb,
    Gltf,
    Ktx2,
    Png,
    Jpeg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GovernedArtifact {
    pub artifact_uri: String,
    pub digest: Sha256Digest,
    pub format: VisualAssetFormat,
    pub byte_length: u64,
}

impl GovernedArtifact {
    pub fn validate(&self) -> Result<(), SceneContractError> {
        if self.artifact_uri.len() > 512
            || parse_artifact_plane_uri(&self.artifact_uri).is_none()
            || self.byte_length == 0
        {
            return Err(SceneContractError::InvalidArtifact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualPrototype {
    pub prototype_id: PrototypeId,
    pub asset: GovernedArtifact,
    pub local_alignment: LocalTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneEntity {
    pub entity_id: EntityId,
    pub prototype_id: PrototypeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_transform: Option<LocalTransform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneAttribution {
    pub source: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneLighting {
    pub intensity_lux: f32,
    pub color_temperature_kelvin: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RendererMode {
    RaytracedLighting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterpolationPolicy {
    HoldLatest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneQualityPolicy {
    pub renderer: RendererMode,
    pub maximum_texture_dimension: u32,
    pub maximum_asset_bytes: u64,
    pub interpolation: InterpolationPolicy,
    pub maximum_pose_age_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneDeclarationBody {
    pub schema_version: String,
    pub session_id: LiveSessionId,
    pub epoch_id: EpochId,
    pub frame_revision: FrameRevision,
    pub simulation_frame: WorldFrameUri,
    pub environment: GovernedArtifact,
    pub prototypes: Vec<VisualPrototype>,
    pub entities: Vec<SceneEntity>,
    pub allowed_camera_kinds: Vec<LiveCameraSource>,
    pub lighting: SceneLighting,
    pub quality: SceneQualityPolicy,
    pub attribution: Vec<SceneAttribution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneDeclaration {
    pub body: SceneDeclarationBody,
    pub digest: Sha256Digest,
}

impl SceneDeclaration {
    pub fn validate(
        &self,
        maximum_entities: u32,
        maximum_asset_bytes: u64,
    ) -> Result<(), SceneContractError> {
        if self.body.schema_version != SCENE_SCHEMA
            || self.body.prototypes.is_empty()
            || self.body.entities.is_empty()
            || self.body.entities.len() > maximum_entities as usize
            || self.body.allowed_camera_kinds.is_empty()
            || self.body.attribution.is_empty()
            || self.body.lighting.intensity_lux <= 0.0
            || !self.body.lighting.intensity_lux.is_finite()
            || !(1_000..=20_000).contains(&self.body.lighting.color_temperature_kelvin)
            || self.body.quality.maximum_pose_age_ms == 0
            || self.body.quality.maximum_texture_dimension == 0
            || self.body.quality.maximum_asset_bytes == 0
            || self.body.quality.maximum_asset_bytes > maximum_asset_bytes
        {
            return Err(SceneContractError::InvalidScene);
        }
        self.body
            .frame_revision
            .validate()
            .map_err(|_| SceneContractError::InvalidScene)?;
        if self.body.simulation_frame.revision_uri().as_str() != self.body.frame_revision.uri {
            return Err(SceneContractError::InvalidScene);
        }
        self.body.environment.validate()?;
        if !renderable(self.body.environment.format) {
            return Err(SceneContractError::InvalidArtifact);
        }
        let mut camera_kinds = std::collections::BTreeSet::new();
        if self
            .body
            .allowed_camera_kinds
            .iter()
            .any(|kind| !camera_kinds.insert(*kind))
        {
            return Err(SceneContractError::InvalidScene);
        }
        for attribution in &self.body.attribution {
            if attribution.source.trim().is_empty()
                || attribution.license.trim().is_empty()
                || attribution
                    .attribution_url
                    .as_ref()
                    .is_some_and(|url| !is_https_url(url))
            {
                return Err(SceneContractError::InvalidScene);
            }
        }
        let mut prototypes = std::collections::BTreeSet::new();
        let mut total_bytes = self.body.environment.byte_length;
        for prototype in &self.body.prototypes {
            if !prototypes.insert(prototype.prototype_id.clone()) {
                return Err(SceneContractError::DuplicatePrototype);
            }
            prototype.asset.validate()?;
            if !renderable(prototype.asset.format) {
                return Err(SceneContractError::InvalidArtifact);
            }
            prototype.local_alignment.validate()?;
            total_bytes = total_bytes
                .checked_add(prototype.asset.byte_length)
                .ok_or(SceneContractError::InvalidArtifact)?;
        }
        if total_bytes > self.body.quality.maximum_asset_bytes || total_bytes > maximum_asset_bytes
        {
            return Err(SceneContractError::InvalidArtifact);
        }
        let mut entities = std::collections::BTreeSet::new();
        for entity in &self.body.entities {
            if !entities.insert(entity.entity_id.clone())
                || !prototypes.contains(&entity.prototype_id)
            {
                return Err(SceneContractError::InvalidEntity);
            }
            if let Some(transform) = entity.static_transform {
                transform.validate()?;
            }
        }
        let canonical =
            serde_json::to_vec(&self.body).map_err(|_| SceneContractError::InvalidScene)?;
        let computed = Sha256Digest::from_bytes(Sha256::digest(canonical).into());
        if computed != self.digest {
            return Err(SceneContractError::SceneDigest);
        }
        Ok(())
    }

    pub fn from_body(body: SceneDeclarationBody) -> Result<Self, SceneContractError> {
        let canonical = serde_json::to_vec(&body).map_err(|_| SceneContractError::InvalidScene)?;
        Ok(Self {
            body,
            digest: Sha256Digest::from_bytes(Sha256::digest(canonical).into()),
        })
    }
}

fn renderable(format: VisualAssetFormat) -> bool {
    matches!(
        format,
        VisualAssetFormat::Usd
            | VisualAssetFormat::Usdz
            | VisualAssetFormat::Glb
            | VisualAssetFormat::Gltf
    )
}

fn is_https_url(value: &str) -> bool {
    value.strip_prefix("https://").is_some_and(|authority| {
        !authority.is_empty() && !authority.chars().any(char::is_whitespace)
    }) && value.len() <= 2048
}

#[derive(Debug, thiserror::Error)]
pub enum SceneContractError {
    #[error("invalid simulation scene identifier {0:?}")]
    InvalidIdentifier(String),
    #[error("invalid local transform")]
    InvalidTransform,
    #[error("invalid governed artifact")]
    InvalidArtifact,
    #[error("invalid scene declaration")]
    InvalidScene,
    #[error("duplicate visual prototype")]
    DuplicatePrototype,
    #[error("invalid scene entity or prototype binding")]
    InvalidEntity,
    #[error("scene declaration digest does not match its canonical body")]
    SceneDigest,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_scene_fixture_uses_the_rust_canonical_digest() {
        let body: SceneDeclarationBody =
            serde_json::from_str(include_str!("../../fixtures/anonymous-scene-body.json")).unwrap();
        let declaration = SceneDeclaration::from_body(body).unwrap();
        assert_eq!(
            declaration.digest.as_str(),
            "sha256:67291c10c39898b2ea11ac9bbe12643148b0112bd868ed44579aaa818fea48e4"
        );
        declaration.validate(128, 1024 * 1024).unwrap();
    }

    #[test]
    fn scene_contract_rejects_non_renderable_primary_assets() {
        let mut body: SceneDeclarationBody =
            serde_json::from_str(include_str!("../../fixtures/anonymous-scene-body.json")).unwrap();
        body.environment.format = VisualAssetFormat::Png;
        let declaration = SceneDeclaration::from_body(body).unwrap();
        assert!(matches!(
            declaration.validate(128, 1024 * 1024),
            Err(SceneContractError::InvalidArtifact)
        ));
    }
}
