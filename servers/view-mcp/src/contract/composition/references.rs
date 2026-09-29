//! The owner admits an address; View selects which resource kinds can describe a scene.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactUri;
use veoveo_frames_mcp::contract::FrameOperationUri;
use veoveo_map_mcp::contract::{
    MapRasterDerivationUri, MapRasterUri, MapRouteUri, MapSourceFeatureUri, MapSpatialDerivationUri,
};
use veoveo_recording_contract::RecordingUri;

use super::SceneCompositionError;

/// An exact input from its domain owner. The wire value is a URI string.
/// ```compile_fail
/// use veoveo_view_mcp::contract::GovernedResourceUri;
/// GovernedResourceUri::Recording("recording://recordings/arbitrary".to_owned());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum GovernedResourceUri {
    Artifact(ArtifactUri),
    SourceFeature(MapSourceFeatureUri),
    Raster(MapRasterUri),
    RasterDerivation(MapRasterDerivationUri),
    SpatialDerivation(MapSpatialDerivationUri),
    Route(MapRouteUri),
    Recording(RecordingUri),
    FrameOperation(FrameOperationUri),
}

impl GovernedResourceUri {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, SceneCompositionError> {
        let value = value.as_ref();
        macro_rules! admit {
            ($($ty:ty => $variant:ident),+ $(,)?) => {
                $(if let Ok(address) = <$ty>::parse(value) { return Ok(Self::$variant(address)); })+
            };
        }
        admit!(ArtifactUri => Artifact, MapSourceFeatureUri => SourceFeature,
            MapRasterUri => Raster, MapRasterDerivationUri => RasterDerivation,
            MapSpatialDerivationUri => SpatialDerivation, MapRouteUri => Route,
            RecordingUri => Recording, FrameOperationUri => FrameOperation);
        Err(SceneCompositionError::InvalidGovernedResourceUri)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Artifact(uri) => uri.as_str(),
            Self::SourceFeature(uri) => uri.as_str(),
            Self::Raster(uri) => uri.as_str(),
            Self::RasterDerivation(uri) => uri.as_str(),
            Self::SpatialDerivation(uri) => uri.as_str(),
            Self::Route(uri) => uri.as_str(),
            Self::Recording(uri) => uri.as_str(),
            Self::FrameOperation(uri) => uri.as_str(),
        }
    }

    pub fn artifact(&self) -> Option<&ArtifactUri> {
        match self {
            Self::Artifact(uri) => Some(uri),
            _ => None,
        }
    }

    pub fn is_artifact(&self) -> bool {
        self.artifact().is_some()
    }

    pub fn requires_map_release(&self) -> bool {
        matches!(
            self,
            Self::SourceFeature(_)
                | Self::Raster(_)
                | Self::RasterDerivation(_)
                | Self::SpatialDerivation(_)
                | Self::Route(_)
        ) || matches!(self, Self::Artifact(uri) if matches!(uri.address(),
            veoveo_artifact_contract::ArtifactAddress::Presented { scheme, .. }
            if scheme == &*veoveo_map_mcp::uris::SCHEME))
    }
}

impl std::fmt::Display for GovernedResourceUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl TryFrom<String> for GovernedResourceUri {
    type Error = SceneCompositionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<GovernedResourceUri> for String {
    fn from(value: GovernedResourceUri) -> Self {
        value.as_str().to_owned()
    }
}
