use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts,
    UriSegment,
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

fn build(segments: &[&str]) -> ResourceUri {
    let mut builder = ResourceUriBuilder::new("frames://world").expect("declared world root");
    for segment in segments {
        builder = builder.segment(
            UriSegment::new(*segment).expect("validated frame identity or declared route segment"),
        );
    }
    builder.build().expect("typed frame world address")
}

fn parse_segments<const N: usize>(value: &str) -> Result<[String; N], FrameUriError> {
    let parts = ResourceUriParts::parse(value)?;
    if parts.scheme() != "frames"
        || parts.authority() != "world"
        || parts.has_query()
        || value.contains('%')
    {
        return Err(FrameUriError::Route);
    }
    parts
        .path_segments()
        .map(|s| s.into_owned())
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| FrameUriError::Route)
}

macro_rules! wire_traits {
    ($name:ident) => {
        impl TryFrom<String> for $name {
            type Error = FrameUriError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl FromStr for $name {
            type Err = FrameUriError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.wire.into()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
        impl ResourceAddress for $name {
            type Error = FrameUriError;
            fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(uri.as_str())
            }
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                Ok(self.wire.clone())
            }
        }
        impl $name {
            pub fn as_str(&self) -> &str {
                self.wire.as_str()
            }
            pub fn as_resource_uri(&self) -> &ResourceUri {
                &self.wire
            }
        }
    };
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct FrameWorldUri {
    wire: ResourceUri,
    world_id: FrameWorldId,
}

impl FrameWorldUri {
    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameId, FrameWorldUri};
    /// FrameWorldUri::new(&FrameId::new("wrong-domain").unwrap());
    /// ```
    pub fn new(world_id: &FrameWorldId) -> Self {
        Self {
            wire: build(&[world_id.as_str()]),
            world_id: world_id.clone(),
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUriError> {
        let [world] = parse_segments(value.as_ref())?;
        Ok(Self::new(&FrameWorldId::new(world)?))
    }
    pub fn world_id(&self) -> FrameWorldId {
        self.world_id.clone()
    }
    pub fn revision(&self, revision_id: &FrameWorldRevisionId) -> FrameWorldRevisionUri {
        FrameWorldRevisionUri::new(&self.world_id, revision_id)
    }
}
wire_traits!(FrameWorldUri);

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct FrameWorldRevisionUri {
    wire: ResourceUri,
    world_id: FrameWorldId,
    revision_id: FrameWorldRevisionId,
}

impl FrameWorldRevisionUri {
    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameWorldId, FrameWorldRevisionUri};
    /// let world = FrameWorldId::new("survey").unwrap();
    /// FrameWorldRevisionUri::new(&world, &world);
    /// ```
    pub fn new(world_id: &FrameWorldId, revision_id: &FrameWorldRevisionId) -> Self {
        Self {
            wire: build(&[world_id.as_str(), "revision", revision_id.as_str()]),
            world_id: world_id.clone(),
            revision_id: revision_id.clone(),
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUriError> {
        let [world, marker, revision] = parse_segments(value.as_ref())?;
        if marker != "revision" {
            return Err(FrameUriError::Route);
        }
        Ok(Self::new(
            &FrameWorldId::new(world)?,
            &FrameWorldRevisionId::new(revision)?,
        ))
    }
    pub fn world_id(&self) -> FrameWorldId {
        self.world_id.clone()
    }
    pub fn revision_id(&self) -> FrameWorldRevisionId {
        self.revision_id.clone()
    }
    pub fn frame(&self, frame_id: &FrameId) -> WorldFrameUri {
        WorldFrameUri::new(self, frame_id)
    }
}
wire_traits!(FrameWorldRevisionUri);

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
    /// let world = FrameWorldUri::new(&FrameWorldId::new("survey").unwrap());
    /// WorldFrameUri::new(&world, &FrameId::new("camera").unwrap());
    /// ```
    pub fn new(revision: &FrameWorldRevisionUri, frame_id: &FrameId) -> Self {
        Self {
            wire: build(&[
                revision.world_id.as_str(),
                "revision",
                revision.revision_id.as_str(),
                "frame",
                frame_id.as_str(),
            ]),
            revision: revision.clone(),
            frame_id: frame_id.clone(),
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUriError> {
        let [world, revision_marker, revision, frame_marker, frame] =
            parse_segments(value.as_ref())?;
        if revision_marker != "revision" || frame_marker != "frame" {
            return Err(FrameUriError::Route);
        }
        let revision = FrameWorldRevisionUri::new(
            &FrameWorldId::new(world)?,
            &FrameWorldRevisionId::new(revision)?,
        );
        Ok(Self::new(&revision, &FrameId::new(frame)?))
    }
    pub fn revision_uri(&self) -> FrameWorldRevisionUri {
        self.revision.clone()
    }
    pub fn frame_id(&self) -> FrameId {
        self.frame_id.clone()
    }
}
wire_traits!(WorldFrameUri);

/// Address of one recorded Frames operation. It carries its operation identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct FrameOperationUri {
    wire: ResourceUri,
    operation_id: CoordinateOperationId,
}

impl FrameOperationUri {
    pub const TEMPLATE: &str = "frames://operation/{operation_id}";

    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameOperationUri, FrameWorldId};
    /// FrameOperationUri::new(&FrameWorldId::new("world").unwrap());
    /// ```
    pub fn new(operation_id: &CoordinateOperationId) -> Self {
        let wire = ResourceUriBuilder::new("frames://operation")
            .expect("declared operation root")
            .segment(UriSegment::new(operation_id.as_str()).expect("typed operation ID"))
            .build()
            .expect("typed operation address");
        Self {
            wire,
            operation_id: operation_id.clone(),
        }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameUriError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value)?;
        let segments = parts.path_segments().collect::<Vec<_>>();
        if parts.scheme() != "frames"
            || parts.authority() != "operation"
            || parts.has_query()
            || segments.len() != 1
        {
            return Err(FrameUriError::Route);
        }
        let result = Self::new(&CoordinateOperationId::new(segments[0].as_ref())?);
        if result.as_str() != value {
            return Err(FrameUriError::Route);
        }
        Ok(result)
    }

    pub fn operation_id(&self) -> &CoordinateOperationId {
        &self.operation_id
    }
}
wire_traits!(FrameOperationUri);
