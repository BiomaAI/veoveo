//! Typed positions and addresses for the authored-world collection.
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts};

use super::{FrameWorldId, FrameWorldSummary};

pub const FRAME_WORLD_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameWorldPage {
    pub items: Vec<FrameWorldSummary>,
    pub limit: usize,
    pub next_cursor: Option<FrameWorldCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameCatalogError;

impl fmt::Display for FrameCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected a Frames world collection address with an optional valid v1 cursor")
    }
}
impl std::error::Error for FrameCatalogError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCursor {
    version: u8,
    collection: String,
    after: FrameWorldId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct FrameWorldCursor {
    wire: String,
    after: FrameWorldId,
}

impl FrameWorldCursor {
    /// A cursor is a position; every page rechecks current caller authority.
    /// ```compile_fail
    /// use veoveo_frames_contract::{FrameId, FrameWorldCursor};
    /// FrameWorldCursor::new(&FrameId::new("camera").unwrap());
    /// ```
    pub fn new(after: &FrameWorldId) -> Self {
        let wire = hex::encode(
            serde_json::to_vec(&WireCursor {
                version: 1,
                collection: FrameWorldsUri::ROOT.to_owned(),
                after: after.clone(),
            })
            .expect("closed cursor fields serialize"),
        );
        Self {
            wire,
            after: after.clone(),
        }
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, FrameCatalogError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 1024 {
            return Err(FrameCatalogError);
        }
        let bytes = hex::decode(&wire).map_err(|_| FrameCatalogError)?;
        let cursor: WireCursor = serde_json::from_slice(&bytes).map_err(|_| FrameCatalogError)?;
        if cursor.version != 1 || cursor.collection != FrameWorldsUri::ROOT {
            return Err(FrameCatalogError);
        }
        Ok(Self {
            wire,
            after: cursor.after,
        })
    }

    pub fn after(&self) -> &FrameWorldId {
        &self.after
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

impl TryFrom<String> for FrameWorldCursor {
    type Error = FrameCatalogError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<FrameWorldCursor> for String {
    fn from(value: FrameWorldCursor) -> Self {
        value.wire
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct FrameWorldsUri {
    wire: ResourceUri,
    cursor: Option<FrameWorldCursor>,
}

impl FrameWorldsUri {
    pub const ROOT: &str = "frames://worlds";
    pub const TEMPLATE: &str = "frames://worlds{?cursor}";

    pub fn new(cursor: Option<&FrameWorldCursor>) -> Self {
        let mut builder = ResourceUriBuilder::new(Self::ROOT).expect("declared world catalog root");
        if let Some(cursor) = cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .expect("typed world cursor");
        }
        Self {
            wire: builder.build().expect("typed world catalog URI"),
            cursor: cursor.cloned(),
        }
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, FrameCatalogError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| FrameCatalogError)?;
        if parts.scheme() != "frames"
            || parts.authority() != "worlds"
            || parts.path_segments().next().is_some()
        {
            return Err(FrameCatalogError);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(FrameCatalogError);
            }
            Some(FrameWorldCursor::parse(
                query.get("cursor").ok_or(FrameCatalogError)?.clone(),
            )?)
        } else {
            None
        };
        let uri = Self::new(cursor.as_ref());
        if uri.as_str() != value {
            return Err(FrameCatalogError);
        }
        Ok(uri)
    }

    pub fn cursor(&self) -> Option<&FrameWorldCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

impl ResourceAddress for FrameWorldsUri {
    type Error = FrameCatalogError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}
impl TryFrom<String> for FrameWorldsUri {
    type Error = FrameCatalogError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<FrameWorldsUri> for String {
    fn from(value: FrameWorldsUri) -> Self {
        value.wire.into()
    }
}
impl fmt::Display for FrameWorldsUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
