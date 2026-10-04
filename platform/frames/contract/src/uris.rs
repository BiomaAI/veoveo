use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceRouteError, ResourceUri, ResourceUriError, ResourceUriParts,
};

use super::{CoordinateOperationId, FrameId, FrameIdError, FrameWorldId, FrameWorldRevisionId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameUriError {
    Components(ResourceUriError),
    Identity(FrameIdError),
    Route,
}

impl fmt::Display for FrameUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Components(error) => error.fmt(f),
            Self::Identity(error) => error.fmt(f),
            Self::Route => f.write_str("expected a Frames world, revision, frame, or operation route without escapes, query, or extra segments"),
        }
    }
}
impl std::error::Error for FrameUriError {}
impl From<ResourceUriError> for FrameUriError {
    fn from(value: ResourceUriError) -> Self {
        Self::Components(value)
    }
}
impl From<FrameIdError> for FrameUriError {
    fn from(value: FrameIdError) -> Self {
        Self::Identity(value)
    }
}

fn route_error(error: ResourceRouteError) -> FrameUriError {
    match error {
        ResourceRouteError::Uri(error) => FrameUriError::Components(error),
        _ => FrameUriError::Route,
    }
}
fn unescaped_route(parts: &ResourceUriParts) -> Result<(), FrameUriError> {
    if parts.as_str().contains('%') {
        Err(FrameUriError::Route)
    } else {
        Ok(())
    }
}

#[veoveo_types::resource_address(cached(FrameUriErrorAddresses), template = "frames://world/{world_id}", traits = ordered, constructor = borrowed, from_str, resource_uri, input = unescaped_route)]
pub struct FrameWorldUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(error=FrameUriError::Identity, accessor = world_id, clone_accessor)]
    world_id: FrameWorldId,
}

impl FrameWorldUri {
    pub fn revision(&self, revision_id: &FrameWorldRevisionId) -> FrameWorldRevisionUri {
        FrameWorldRevisionUri::new(&self.world_id, revision_id)
    }
}
#[veoveo_types::resource_address(cached(FrameUriErrorAddresses), template = "frames://world/{world_id}/revision/{revision_id}", traits = ordered, constructor = borrowed, from_str, resource_uri, input = unescaped_route)]
pub struct FrameWorldRevisionUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(error=FrameUriError::Identity, accessor = world_id, clone_accessor)]
    world_id: FrameWorldId,
    #[resource(error=FrameUriError::Identity, accessor = revision_id, clone_accessor)]
    revision_id: FrameWorldRevisionId,
}

impl FrameWorldRevisionUri {
    pub fn frame(&self, frame_id: &FrameId) -> WorldFrameUri {
        WorldFrameUri::new(self, frame_id)
    }
}
#[veoveo_types::resource_address(custom(template="frames://world/{world_id}/revision/{revision_id}/frame/{frame_id}", error=FrameUriError, route_error=route_error, input=unescaped_route))]
struct WorldFrameRoute {
    #[resource(error=FrameUriError::Identity)]
    world_id: FrameWorldId,
    #[resource(error=FrameUriError::Identity)]
    revision_id: FrameWorldRevisionId,
    #[resource(error=FrameUriError::Identity)]
    frame_id: FrameId,
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct WorldFrameUri {
    wire: ResourceUri,
    revision: FrameWorldRevisionUri,
    frame_id: FrameId,
}

impl WorldFrameUri {
    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameId, FrameWorldId, FrameWorldUri, WorldFrameUri};
    /// let world = FrameWorldUri::new(&FrameWorldId::parse("survey").unwrap());
    /// WorldFrameUri::new(&world, &FrameId::parse("camera").unwrap());
    /// ```
    pub fn new(revision: &FrameWorldRevisionUri, frame_id: &FrameId) -> Self {
        let route = WorldFrameRoute::resource_from_parts(
            revision.world_id.clone(),
            revision.revision_id.clone(),
            frame_id.clone(),
        )
        .expect("typed world frame components");
        Self {
            wire: route.to_uri().expect("typed world frame address"),
            revision: revision.clone(),
            frame_id: frame_id.clone(),
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUriError> {
        let uri = ResourceUri::new(value.as_ref())?;
        let route = <WorldFrameRoute as ResourceAddress>::parse(&uri)?;
        let revision = FrameWorldRevisionUri::new(&route.world_id, &route.revision_id);
        Ok(Self::new(&revision, &route.frame_id))
    }
    pub fn revision_uri(&self) -> FrameWorldRevisionUri {
        self.revision.clone()
    }
    pub fn frame_id(&self) -> FrameId {
        self.frame_id.clone()
    }
}
impl WorldFrameUri {
    pub const RESOURCE_ROUTES: &'static [veoveo_types::ResourceRoute] =
        WorldFrameRoute::RESOURCE_ROUTES;
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
    pub fn as_resource_uri(&self) -> &ResourceUri {
        &self.wire
    }
}
impl TryFrom<String> for WorldFrameUri {
    type Error = FrameUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl FromStr for WorldFrameUri {
    type Err = FrameUriError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl From<WorldFrameUri> for String {
    fn from(value: WorldFrameUri) -> Self {
        value.wire.into()
    }
}
impl fmt::Display for WorldFrameUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl ResourceAddress for WorldFrameUri {
    type Error = FrameUriError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}

/// Address of one recorded Frames operation. It carries its operation identity.
#[veoveo_types::resource_address(cached(FrameUriErrorAddresses), template = "frames://operation/{operation_id}", constructor = borrowed, from_str, resource_uri)]
pub struct FrameOperationUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(error=FrameUriError::Identity, accessor = operation_id)]
    operation_id: CoordinateOperationId,
}

impl FrameOperationUri {
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
}

#[doc(hidden)]
pub struct FrameUriErrorAddresses;
impl veoveo_types::ResourceProfile for FrameUriErrorAddresses {
    type Error = FrameUriError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, error| route_error(error),
        };
}

/// ```compile_fail
/// use veoveo_frames_contract::{FrameId, FrameWorldUri};
/// FrameWorldUri::new(&FrameId::parse("wrong-domain").unwrap());
/// ```
const _: () = ();
/// ```compile_fail
/// use veoveo_frames_contract::{FrameWorldId, FrameWorldRevisionUri};
/// let world = FrameWorldId::parse("survey").unwrap();
/// FrameWorldRevisionUri::new(&world, &world);
/// ```
const _: () = ();
/// ```compile_fail
/// use veoveo_frames_contract::{FrameOperationUri, FrameWorldId};
/// FrameOperationUri::new(&FrameWorldId::parse("world").unwrap());
/// ```
const _: () = ();
