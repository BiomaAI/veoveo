//! View-owned addresses. The shared URI library handles parsing and encoding.
use std::fmt;

use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{
    CaptureLimits, FrameId, LayerId, PreviewScenePolicy, SceneCompositionId, Sha256Digest, ViewId,
};
use crate::uris;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid View resource address or parameters")]
pub struct ViewResourceError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewDocument {
    Agents,
    Design,
}
impl ViewDocument {
    pub fn parse(value: &str) -> Result<Self, ViewResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(ViewResourceError),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
}

/// Credential-free tile identity. It is distinct from a composition or output digest.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TileKey(Sha256Digest);
impl TileKey {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ViewResourceError> {
        Sha256Digest::parse(value)
            .map(Self)
            .map_err(|_| ViewResourceError)
    }
    pub fn from_bytes(value: &[u8]) -> Self {
        Self(Sha256Digest::from_bytes(value))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl fmt::Display for TileKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl TryFrom<String> for TileKey {
    type Error = ViewResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<TileKey> for String {
    fn from(value: TileKey) -> Self {
        value.to_string()
    }
}
impl schemars::JsonSchema for TileKey {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TileKey".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        resource_string_schema(generator)
    }
}

#[veoveo_types::resource_address(
    components(ViewResourceErrorAddresses),
    template = "view://layer/{layer_id}"
)]
pub struct LayerUri(
    #[resource(variable = "layer_id", error = |_| ViewResourceError, accessor = id)] LayerId,
);
#[veoveo_types::resource_address(
    components(ViewResourceErrorAddresses),
    template = "view://view/{view_id}"
)]
pub struct ViewUri(
    #[resource(variable = "view_id", error = |_| ViewResourceError, accessor = id)] ViewId,
);
#[veoveo_types::resource_address(
    components(ViewResourceErrorAddresses),
    template = "view://composition/{composition_id}"
)]
pub struct CompositionUri(
    #[resource(variable = "composition_id", error = |_| ViewResourceError, accessor = id)]
    SceneCompositionId,
);
#[veoveo_types::resource_address(
    components(ViewResourceErrorAddresses),
    template = "view://frame/{frame_id}"
)]
pub struct FrameUri(
    #[resource(variable = "frame_id", error = |_| ViewResourceError, accessor = id)] FrameId,
);
#[veoveo_types::resource_address(
    components(ViewResourceErrorAddresses),
    template = "view://tile/{tile_key}"
)]
pub struct TileUri(
    #[resource(variable = "tile_key", codec = TileKeyCodec, error = |_| ViewResourceError, accessor = id)]
     TileKey,
);
/// A checked scene request. Runtime admission also applies installation limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ViewSceneUri {
    view_id: ViewId,
    policy: PreviewScenePolicy,
}
// Construction excludes nonfinite values, so equality is reflexive.
impl Eq for ViewSceneUri {}
impl ViewSceneUri {
    pub fn new(view_id: ViewId, policy: PreviewScenePolicy) -> Result<Self, ViewResourceError> {
        policy
            .validate(&CaptureLimits {
                max_width_px: u32::MAX,
                max_height_px: u32::MAX,
                max_pixels: u64::MAX,
                max_deadline_ms: u32::MAX,
            })
            .map_err(|_| ViewResourceError)?;
        Ok(Self { view_id, policy })
    }
    pub fn view_id(&self) -> &ViewId {
        &self.view_id
    }
    pub fn policy(&self) -> PreviewScenePolicy {
        self.policy
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ViewResourceError> {
        match ViewResource::parse(value)? {
            ViewResource::Scene(uri) => Ok(uri),
            _ => Err(ViewResourceError),
        }
    }
    pub fn to_uri(&self) -> ResourceUri {
        ResourceUriBuilder::new("view://view")
            .expect("declared View root")
            .segment(UriSegment::new(self.view_id.as_str()).expect("admitted View ID"))
            .segment(UriSegment::new("scene").expect("declared View child"))
            .query_pair("width_px", &self.policy.width_px.to_string())
            .expect("declared query")
            .query_pair("height_px", &self.policy.height_px.to_string())
            .expect("declared query")
            .query_pair(
                "max_screen_error_px",
                &self.policy.max_screen_error_px.to_string(),
            )
            .expect("declared query")
            .build()
            .expect("admitted View scene")
    }
}
impl TryFrom<String> for ViewSceneUri {
    type Error = ViewResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<ViewSceneUri> for String {
    fn from(value: ViewSceneUri) -> Self {
        value.to_uri().to_string()
    }
}
impl fmt::Display for ViewSceneUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_uri().fmt(f)
    }
}
impl schemars::JsonSchema for ViewSceneUri {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ViewSceneUri".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        resource_string_schema(generator)
    }
}

/// Each address accepts only the IDs and parameters owned by its route.
/// ```compile_fail
/// use veoveo_view_mcp::{FrameId, ViewUri};
/// ViewUri::new(FrameId::parse("frame-1").unwrap());
/// ```
/// ```compile_fail
/// use veoveo_view_mcp::TileUri;
/// TileUri::new("unchecked tile key");
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ViewResource {
    Docs,
    Document(ViewDocument),
    Contract,
    PreviewApp,
    Layers,
    Layer(LayerUri),
    Compositions,
    Composition(CompositionUri),
    Views,
    View(ViewUri),
    Frames,
    Frame(FrameUri),
    Scene(ViewSceneUri),
    Tile(TileUri),
}
impl ViewResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ViewResourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| ViewResourceError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let path = path.iter().map(|p| p.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), path.as_slice()) {
            ("view", "view", [id, "scene"]) => {
                let query = parts.query_parameters();
                if query.len() != 3 {
                    return Err(ViewResourceError);
                }
                let policy = PreviewScenePolicy {
                    width_px: query
                        .get("width_px")
                        .ok_or(ViewResourceError)?
                        .parse()
                        .map_err(|_| ViewResourceError)?,
                    height_px: query
                        .get("height_px")
                        .ok_or(ViewResourceError)?
                        .parse()
                        .map_err(|_| ViewResourceError)?,
                    max_screen_error_px: query
                        .get("max_screen_error_px")
                        .ok_or(ViewResourceError)?
                        .parse()
                        .map_err(|_| ViewResourceError)?,
                };
                Ok(Self::Scene(ViewSceneUri::new(
                    ViewId::parse(*id).map_err(|_| ViewResourceError)?,
                    policy,
                )?))
            }
            _ if parts.has_query() => Err(ViewResourceError),
            ("view", "docs", []) => Ok(Self::Docs),
            ("view", "docs", [id]) => Ok(Self::Document(ViewDocument::parse(id)?)),
            ("view", "contract", []) => Ok(Self::Contract),
            ("ui", "view", ["preview.html"]) => Ok(Self::PreviewApp),
            ("view", "layers", []) => Ok(Self::Layers),
            ("view", "compositions", []) => Ok(Self::Compositions),
            ("view", "views", []) => Ok(Self::Views),
            ("view", "frames", []) => Ok(Self::Frames),
            ("view", "layer", [id]) => Ok(Self::Layer(LayerUri::new(
                LayerId::parse(*id).map_err(|_| ViewResourceError)?,
            ))),
            ("view", "composition", [id]) => Ok(Self::Composition(CompositionUri::new(
                SceneCompositionId::parse(*id).map_err(|_| ViewResourceError)?,
            ))),
            ("view", "view", [id]) => Ok(Self::View(ViewUri::new(
                ViewId::parse(*id).map_err(|_| ViewResourceError)?,
            ))),
            ("view", "frame", [id]) => Ok(Self::Frame(FrameUri::new(
                FrameId::parse(*id).map_err(|_| ViewResourceError)?,
            ))),
            ("view", "tile", [id]) => Ok(Self::Tile(TileUri::new(TileKey::parse(*id)?))),
            _ => Err(ViewResourceError),
        }?;
        if resource.to_uri()?.as_str() != value {
            return Err(ViewResourceError);
        }
        Ok(resource)
    }
}
impl ViewResource {
    pub fn is_subscribable(&self) -> bool {
        matches!(
            self,
            Self::Compositions | Self::Views | Self::Frames | Self::View(_)
        )
    }
}
impl ResourceAddress for ViewResource {
    type Error = ViewResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri)
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let literal = match self {
            Self::Docs => uris::DOCS,
            Self::Contract => uris::CONTRACT,
            Self::PreviewApp => uris::PREVIEW_APP_URI,
            Self::Layers => uris::LAYERS,
            Self::Compositions => uris::COMPOSITIONS,
            Self::Views => uris::VIEWS,
            Self::Frames => uris::FRAMES,
            Self::Document(id) => return Ok(path(uris::DOCS, id.as_str())),
            Self::Layer(uri) => return Ok(uri.to_uri()),
            Self::Composition(uri) => return Ok(uri.to_uri()),
            Self::View(uri) => return Ok(uri.to_uri()),
            Self::Frame(uri) => return Ok(uri.to_uri()),
            Self::Scene(uri) => return Ok(uri.to_uri()),
            Self::Tile(uri) => return Ok(uri.to_uri()),
        };
        ResourceUriBuilder::new(literal)
            .and_then(ResourceUriBuilder::build)
            .map_err(|_| ViewResourceError)
    }
}
impl TryFrom<String> for ViewResource {
    type Error = ViewResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<ViewResource> for String {
    fn from(value: ViewResource) -> Self {
        value.to_uri().expect("admitted View address").to_string()
    }
}
impl schemars::JsonSchema for ViewResource {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ViewResource".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        resource_string_schema(generator)
    }
}

fn path(root: &str, id: &str) -> ResourceUri {
    ResourceUriBuilder::new(root)
        .expect("declared View root")
        .segment(UriSegment::new(id).expect("admitted View segment"))
        .build()
        .expect("admitted View resource")
}

fn resource_string_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <String as schemars::JsonSchema>::json_schema(generator)
}

struct TileKeyCodec;
impl veoveo_types::ResourceFieldCodec<TileKey> for TileKeyCodec {
    type Error = ViewResourceError;
    fn parse(value: &str) -> Result<TileKey, Self::Error> {
        TileKey::parse(value)
    }
    fn text(value: &TileKey) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub struct ViewResourceErrorAddresses;
impl veoveo_types::ResourceProfile for ViewResourceErrorAddresses {
    type Error = ViewResourceError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| ViewResourceError,
        };
    const SCHEMA: Option<veoveo_types::ResourceSchema> = Some(veoveo_types::ResourceSchema {
        schema: |_, generator| resource_string_schema(generator),
        inline: true,
    });
}
