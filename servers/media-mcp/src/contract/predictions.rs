//! Caller-owned prediction catalog and collection-bound continuation.
use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceFieldCodec, ResourceUri};

pub const MEDIA_PREDICTION_PAGE_SIZE: usize = 100;

use super::{MediaPredictionError, MediaPredictionId, MediaPredictionUri};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: String,
    after: MediaPredictionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MediaPredictionCursor {
    wire: String,
    after: MediaPredictionId,
}

impl MediaPredictionCursor {
    /// Positions require provider prediction identities.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::MediaPredictionCursor;
    /// MediaPredictionCursor::new("raw-prediction-id");
    /// ```
    pub fn new(after: MediaPredictionId) -> Result<Self, MediaPredictionError> {
        let wire = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                collection: MediaPredictionIndexUri::ROOT.to_owned(),
                after: after.clone(),
            })
            .expect("closed prediction cursor fields serialize"),
        );
        Ok(Self { wire, after })
    }

    pub fn parse(wire: impl Into<String>) -> Result<Self, MediaPredictionError> {
        let wire = wire.into();
        if wire.is_empty() || wire.len() > 2048 {
            return Err(MediaPredictionError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(&wire)
            .map_err(|_| MediaPredictionError)?;
        let cursor: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| MediaPredictionError)?;
        if cursor.version != 1 || cursor.collection != MediaPredictionIndexUri::ROOT {
            return Err(MediaPredictionError);
        }
        Ok(Self {
            wire,
            after: cursor.after,
        })
    }

    pub fn after(&self) -> MediaPredictionId {
        self.after.clone()
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

impl TryFrom<String> for MediaPredictionCursor {
    type Error = MediaPredictionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MediaPredictionCursor> for String {
    fn from(value: MediaPredictionCursor) -> Self {
        value.wire
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[resource(template="media://predictions{?cursor}", error=MediaPredictionError, route_error=|_| MediaPredictionError, wire)]
pub struct MediaPredictionIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=PredictionCursorCodec, error=|_| MediaPredictionError)]
    cursor: Option<MediaPredictionCursor>,
}

impl MediaPredictionIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;

    pub fn new(cursor: Option<&MediaPredictionCursor>) -> Self {
        Self::resource_from_parts(cursor.cloned()).expect("typed prediction index address")
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, MediaPredictionError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| MediaPredictionError)?;
        <Self as ResourceAddress>::parse(&uri)
    }

    pub fn cursor(&self) -> Option<&MediaPredictionCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}

struct PredictionCursorCodec;
impl ResourceFieldCodec<MediaPredictionCursor> for PredictionCursorCodec {
    type Error = MediaPredictionError;
    fn parse(value: &str) -> Result<MediaPredictionCursor, Self::Error> {
        MediaPredictionCursor::parse(value)
    }
    fn text(value: &MediaPredictionCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
impl fmt::Display for MediaPredictionIndexUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "EntryWire", into = "EntryWire")]
pub struct MediaPredictionEntry {
    prediction_uri: MediaPredictionUri,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    id: MediaPredictionId,
    prediction_uri: MediaPredictionUri,
}

impl MediaPredictionEntry {
    pub fn new(id: MediaPredictionId) -> Result<Self, MediaPredictionError> {
        Ok(Self {
            prediction_uri: MediaPredictionUri::new(id),
        })
    }
    pub fn id(&self) -> MediaPredictionId {
        self.prediction_uri.id().clone()
    }
    pub fn prediction_uri(&self) -> &MediaPredictionUri {
        &self.prediction_uri
    }
}
impl TryFrom<EntryWire> for MediaPredictionEntry {
    type Error = MediaPredictionError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        if &value.id != value.prediction_uri.id() {
            return Err(MediaPredictionError);
        }
        Ok(Self {
            prediction_uri: value.prediction_uri,
        })
    }
}
impl From<MediaPredictionEntry> for EntryWire {
    fn from(value: MediaPredictionEntry) -> Self {
        Self {
            id: value.id(),
            prediction_uri: value.prediction_uri,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct MediaPredictionPage {
    items: Vec<MediaPredictionEntry>,
    next_cursor: Option<MediaPredictionCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<MediaPredictionEntry>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<MediaPredictionCursor>,
}

impl MediaPredictionPage {
    pub fn from_ids(
        ids: Vec<MediaPredictionId>,
        next: Option<MediaPredictionId>,
    ) -> Result<Self, MediaPredictionError> {
        Self::try_from(PageWire {
            items: ids
                .into_iter()
                .map(MediaPredictionEntry::new)
                .collect::<Result<_, _>>()?,
            limit: MEDIA_PREDICTION_PAGE_SIZE,
            next_cursor: next.map(MediaPredictionCursor::new).transpose()?,
        })
    }

    pub fn items(&self) -> &[MediaPredictionEntry] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&MediaPredictionCursor> {
        self.next_cursor.as_ref()
    }
}
impl TryFrom<PageWire> for MediaPredictionPage {
    type Error = MediaPredictionError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        if value.limit != MEDIA_PREDICTION_PAGE_SIZE
            || value.items.len() > MEDIA_PREDICTION_PAGE_SIZE
            || value
                .items
                .windows(2)
                .any(|pair| pair[0].id() >= pair[1].id())
            || value.next_cursor.as_ref().is_some_and(|cursor| {
                value.items.len() != MEDIA_PREDICTION_PAGE_SIZE
                    || value.items.last().map(MediaPredictionEntry::id) != Some(cursor.after())
            })
        {
            return Err(MediaPredictionError);
        }
        Ok(Self {
            items: value.items,
            next_cursor: value.next_cursor,
        })
    }
}
impl From<MediaPredictionPage> for PageWire {
    fn from(value: MediaPredictionPage) -> Self {
        Self {
            items: value.items,
            limit: MEDIA_PREDICTION_PAGE_SIZE,
            next_cursor: value.next_cursor,
        }
    }
}
