use std::{collections::BTreeMap, fmt, path::Path, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;
use veoveo_mcp_contract::{FrameWorldRevisionUri, WorldFrameUri};
use veoveo_simulation_pose::FrameRevision;

pub const LAYER_CATALOG_SCHEMA: &str = "veoveo.io/simulation-view-layer-catalog/v1";

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct GeospatialLayerId(String);

impl GeospatialLayerId {
    pub fn new(value: impl Into<String>) -> Result<Self, LayerCatalogError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(LayerCatalogError::InvalidLayerId(value));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GeospatialLayerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for GeospatialLayerId {
    type Err = LayerCatalogError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for GeospatialLayerId {
    type Error = LayerCatalogError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<GeospatialLayerId> for String {
    fn from(value: GeospatialLayerId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeospatialLayerType {
    #[serde(rename = "streamed_3d_tiles")]
    Streamed3dTiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum GeospatialLayerSource {
    CesiumIon {
        asset_id: u64,
        server_url: String,
        api_url: String,
        application_id: u64,
        credential_environment: String,
    },
    Https3dTiles {
        root_url: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeodeticOrigin {
    pub latitude_degrees: f64,
    pub longitude_degrees: f64,
    pub ellipsoid_height_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerGeoreference {
    pub world: FrameWorldRevisionUri,
    pub frame_revision: FrameRevision,
    pub local_enu_frame: WorldFrameUri,
    pub origin: GeodeticOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerBudgets {
    pub maximum_cache_bytes: u64,
    pub maximum_tile_bytes: u64,
    pub maximum_visible_tiles: u32,
    pub maximum_pending_tiles: u32,
    pub maximum_screen_space_error: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerLicense {
    pub identifier: String,
    pub attribution: String,
    pub attribution_url: String,
    pub display_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeospatialLayerDefinition {
    pub layer_id: GeospatialLayerId,
    pub layer_type: GeospatialLayerType,
    pub source: GeospatialLayerSource,
    pub allowed_hosts: Vec<String>,
    pub allowed_redirect_hosts: Vec<String>,
    pub budgets: LayerBudgets,
    pub license: LayerLicense,
    pub georeference: LayerGeoreference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeospatialLayerCatalogFile {
    pub schema_version: String,
    pub layers: Vec<GeospatialLayerDefinition>,
}

#[derive(Debug, Clone)]
pub struct GeospatialLayerCatalog {
    layers: BTreeMap<GeospatialLayerId, GeospatialLayerDefinition>,
}

impl GeospatialLayerCatalog {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, LayerCatalogError> {
        let file: GeospatialLayerCatalogFile = serde_json::from_slice(bytes)
            .map_err(|error| LayerCatalogError::Json(error.to_string()))?;
        Self::from_file(file)
    }

    pub fn from_path(path: &Path) -> Result<Self, LayerCatalogError> {
        let bytes = std::fs::read(path).map_err(|error| {
            LayerCatalogError::Io(path.display().to_string(), error.to_string())
        })?;
        Self::from_slice(&bytes)
    }

    pub fn from_file(file: GeospatialLayerCatalogFile) -> Result<Self, LayerCatalogError> {
        if file.schema_version != LAYER_CATALOG_SCHEMA {
            return Err(LayerCatalogError::Schema(file.schema_version));
        }
        let mut layers = BTreeMap::new();
        for layer in file.layers {
            layer.validate()?;
            let id = layer.layer_id.clone();
            if layers.insert(id.clone(), layer).is_some() {
                return Err(LayerCatalogError::DuplicateLayer(id));
            }
        }
        Ok(Self { layers })
    }

    pub fn empty() -> Self {
        Self {
            layers: BTreeMap::new(),
        }
    }

    pub fn get(&self, id: &GeospatialLayerId) -> Option<&GeospatialLayerDefinition> {
        self.layers.get(id)
    }

    pub fn validate_scene_binding(
        &self,
        id: &GeospatialLayerId,
        frame_revision: &FrameRevision,
        simulation_frame: &WorldFrameUri,
    ) -> Result<&GeospatialLayerDefinition, LayerCatalogError> {
        let layer = self
            .get(id)
            .ok_or_else(|| LayerCatalogError::UnknownLayer(id.clone()))?;
        if &layer.georeference.frame_revision != frame_revision
            || &layer.georeference.local_enu_frame != simulation_frame
            || layer.georeference.world.as_str() != frame_revision.uri
        {
            return Err(LayerCatalogError::GeoreferenceMismatch(id.clone()));
        }
        Ok(layer)
    }
}

impl GeospatialLayerDefinition {
    fn validate(&self) -> Result<(), LayerCatalogError> {
        if self.allowed_hosts.is_empty()
            || self.budgets.maximum_cache_bytes == 0
            || self.budgets.maximum_tile_bytes == 0
            || self.budgets.maximum_tile_bytes > self.budgets.maximum_cache_bytes
            || self.budgets.maximum_visible_tiles == 0
            || self.budgets.maximum_pending_tiles == 0
            || !self.budgets.maximum_screen_space_error.is_finite()
            || self.budgets.maximum_screen_space_error <= 0.0
            || self.budgets.maximum_screen_space_error > 256.0
        {
            return Err(LayerCatalogError::InvalidBudgets(self.layer_id.clone()));
        }
        for host in self
            .allowed_hosts
            .iter()
            .chain(&self.allowed_redirect_hosts)
        {
            validate_host(host)?;
        }
        let source_urls = match &self.source {
            GeospatialLayerSource::CesiumIon {
                asset_id,
                server_url,
                api_url,
                application_id,
                credential_environment,
            } => {
                if *asset_id == 0 || *application_id == 0 {
                    return Err(LayerCatalogError::InvalidSource(self.layer_id.clone()));
                }
                validate_environment(credential_environment)?;
                vec![server_url, api_url]
            }
            GeospatialLayerSource::Https3dTiles { root_url } => vec![root_url],
        };
        for source_url in source_urls {
            let url = validate_https_url(source_url)?;
            let host = url.host_str().expect("validated HTTPS URL has a host");
            if !self.allowed_hosts.iter().any(|allowed| allowed == host) {
                return Err(LayerCatalogError::DeniedHost(host.to_owned()));
            }
        }
        let georeference = &self.georeference;
        georeference
            .frame_revision
            .validate()
            .map_err(|_| LayerCatalogError::InvalidGeoreference(self.layer_id.clone()))?;
        if georeference.world.as_str() != georeference.frame_revision.uri
            || georeference.local_enu_frame.revision_uri().as_str()
                != georeference.frame_revision.uri
            || !georeference.origin.latitude_degrees.is_finite()
            || !(-90.0..=90.0).contains(&georeference.origin.latitude_degrees)
            || !georeference.origin.longitude_degrees.is_finite()
            || !(-180.0..=180.0).contains(&georeference.origin.longitude_degrees)
            || !georeference.origin.ellipsoid_height_m.is_finite()
        {
            return Err(LayerCatalogError::InvalidGeoreference(
                self.layer_id.clone(),
            ));
        }
        if self.license.identifier.trim().is_empty()
            || self.license.attribution.trim().is_empty()
            || !self.license.display_required
        {
            return Err(LayerCatalogError::InvalidLicense(self.layer_id.clone()));
        }
        validate_https_url(&self.license.attribution_url)?;
        Ok(())
    }
}

fn validate_environment(value: &str) -> Result<(), LayerCatalogError> {
    if !value.starts_with("SIMULATION_VIEW_LAYER_")
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        || !value.as_bytes()[0].is_ascii_uppercase()
    {
        return Err(LayerCatalogError::InvalidCredentialEnvironment(
            value.to_owned(),
        ));
    }
    Ok(())
}

fn validate_host(value: &str) -> Result<(), LayerCatalogError> {
    if value.is_empty()
        || value.len() > 253
        || value.contains(['/', ':', '@'])
        || value.chars().any(char::is_whitespace)
    {
        return Err(LayerCatalogError::InvalidHost(value.to_owned()));
    }
    Ok(())
}

fn validate_https_url(value: &str) -> Result<Url, LayerCatalogError> {
    let url = Url::parse(value).map_err(|_| LayerCatalogError::InvalidUrl(value.to_owned()))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(LayerCatalogError::InvalidUrl(value.to_owned()));
    }
    Ok(url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerLifecycle {
    Configured,
    Loading,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LayerFailureCode {
    MissingCredentials,
    InvalidGeoreference,
    UnavailableCoverage,
    DeniedHost,
    BudgetExceeded,
    ProviderUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerFailureDiagnostic {
    pub code: LayerFailureCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeospatialLayerHealth {
    pub layer_id: GeospatialLayerId,
    pub lifecycle: LayerLifecycle,
    pub resident_bytes: u64,
    pub visible_tile_count: u32,
    pub pending_tile_count: u32,
    pub attribution: String,
    pub attribution_url: String,
    pub failure: Option<LayerFailureDiagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum LayerCatalogError {
    #[error("invalid geospatial layer identifier {0:?}")]
    InvalidLayerId(String),
    #[error("unsupported geospatial layer catalog schema {0:?}")]
    Schema(String),
    #[error("duplicate geospatial layer {0}")]
    DuplicateLayer(GeospatialLayerId),
    #[error("geospatial layer {0} is not configured")]
    UnknownLayer(GeospatialLayerId),
    #[error("geospatial layer {0} does not match the scene Frames revision and local ENU frame")]
    GeoreferenceMismatch(GeospatialLayerId),
    #[error("geospatial layer {0} has invalid budgets")]
    InvalidBudgets(GeospatialLayerId),
    #[error("geospatial layer {0} has an invalid source")]
    InvalidSource(GeospatialLayerId),
    #[error("geospatial layer {0} has an invalid georeference")]
    InvalidGeoreference(GeospatialLayerId),
    #[error("geospatial layer {0} has invalid license or attribution")]
    InvalidLicense(GeospatialLayerId),
    #[error("geospatial layer source host {0:?} is denied")]
    DeniedHost(String),
    #[error("invalid geospatial source host {0:?}")]
    InvalidHost(String),
    #[error("invalid geospatial HTTPS URL {0:?}")]
    InvalidUrl(String),
    #[error("invalid geospatial credential environment {0:?}")]
    InvalidCredentialEnvironment(String),
    #[error("failed reading geospatial layer catalog {0}: {1}")]
    Io(String, String),
    #[error("invalid geospatial layer catalog JSON: {0}")]
    Json(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"{
      "schemaVersion":"veoveo.io/simulation-view-layer-catalog/v1",
      "layers":[{
        "layerId":"synthetic-world",
        "layerType":"streamed_3d_tiles",
        "source":{"kind":"cesium_ion","assetId":2275207,"serverUrl":"https://ion.example/","apiUrl":"https://api.example/","applicationId":413,"credentialEnvironment":"SIMULATION_VIEW_LAYER_TOKEN"},
        "allowedHosts":["ion.example","api.example"],
        "allowedRedirectHosts":["assets.example"],
        "budgets":{"maximumCacheBytes":1073741824,"maximumTileBytes":67108864,"maximumVisibleTiles":4096,"maximumPendingTiles":64,"maximumScreenSpaceError":16.0},
        "license":{"identifier":"provider-terms","attribution":"Imagery provider","attributionUrl":"https://example.com/terms","displayRequired":true},
        "georeference":{"world":"frames://world/test/revision/r1","frameRevision":{"uri":"frames://world/test/revision/r1","digest":"sha256:1111111111111111111111111111111111111111111111111111111111111111"},"localEnuFrame":"frames://world/test/revision/r1/frame/simulation","origin":{"latitudeDegrees":40.0,"longitudeDegrees":-105.0,"ellipsoidHeightM":1600.0}}
      }]
    }"#;

    #[test]
    fn catalog_validates_exact_frame_binding_without_credentials() {
        let catalog = GeospatialLayerCatalog::from_slice(CATALOG.as_bytes()).unwrap();
        let layer = catalog
            .get(&GeospatialLayerId::new("synthetic-world").unwrap())
            .unwrap();
        assert_eq!(layer.budgets.maximum_visible_tiles, 4096);
        catalog
            .validate_scene_binding(
                &layer.layer_id,
                &layer.georeference.frame_revision,
                &layer.georeference.local_enu_frame,
            )
            .unwrap();
        assert!(!CATALOG.contains("secret-value"));
    }

    #[test]
    fn catalog_rejects_unknown_and_mismatched_scene_layers() {
        let catalog = GeospatialLayerCatalog::from_slice(CATALOG.as_bytes()).unwrap();
        let layer = catalog
            .get(&GeospatialLayerId::new("synthetic-world").unwrap())
            .unwrap();
        assert!(matches!(
            catalog.validate_scene_binding(
                &GeospatialLayerId::new("missing").unwrap(),
                &layer.georeference.frame_revision,
                &layer.georeference.local_enu_frame,
            ),
            Err(LayerCatalogError::UnknownLayer(_))
        ));
        let mismatched =
            WorldFrameUri::parse("frames://world/test/revision/r1/frame/other").unwrap();
        assert!(matches!(
            catalog.validate_scene_binding(
                &layer.layer_id,
                &layer.georeference.frame_revision,
                &mismatched,
            ),
            Err(LayerCatalogError::GeoreferenceMismatch(_))
        ));
    }

    #[test]
    fn source_host_must_be_admitted() {
        let denied = CATALOG.replace("\"ion.example\",", "");
        assert!(matches!(
            GeospatialLayerCatalog::from_slice(denied.as_bytes()),
            Err(LayerCatalogError::DeniedHost(_))
        ));
    }
}
