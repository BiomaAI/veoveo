//! Caller-owned prediction catalog and collection-bound continuation.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceFieldCodec, ResourceUri};

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
#[serde(transparent)]
pub struct MediaPredictionCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<MediaPredictionCursorCodec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MediaPredictionCursorCodec;
impl veoveo_types::StatelessCursorCodec for MediaPredictionCursorCodec {}
impl veoveo_types::CursorCodec for MediaPredictionCursorCodec {
    type Position = MediaPredictionId;
    type Error = MediaPredictionError;
    fn check(&self, _position: &Self::Position) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (MediaPredictionIndexUri::ROOT).to_owned(),
            after: position.clone(),
        })
        .expect("closed owner cursor fields serialize");
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 2048 {
            return Err(MediaPredictionError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| MediaPredictionError)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| MediaPredictionError)?;
        if decoded.version != 1 || decoded.collection != MediaPredictionIndexUri::ROOT {
            return Err(MediaPredictionError);
        }
        let position = decoded.after;
        Ok(position)
    }
}
impl MediaPredictionCursor {
    /// Positions require provider prediction identities.
    /// ```compile_fail
    /// use veoveo_media_mcp::contract::MediaPredictionCursor;
    /// MediaPredictionCursor::new("raw-prediction-id");
    /// ```
    pub fn new(after: MediaPredictionId) -> Result<Self, MediaPredictionError> {
        let cursor = veoveo_types::OpaqueCursor::try_new(MediaPredictionCursorCodec, after)?;
        Ok(Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MediaPredictionError> {
        veoveo_types::OpaqueCursor::parse(MediaPredictionCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> MediaPredictionId {
        self.cursor.position().clone()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[veoveo_types::resource_address(
    cached(MediaPredictionErrorAddresses),
    template = "media://predictions{?cursor}"
)]
pub struct MediaPredictionIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec=PredictionCursorCodec, error=|_| MediaPredictionError, accessor = cursor, argument = optional_borrowed)]
    cursor: Option<MediaPredictionCursor>,
}

impl MediaPredictionIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
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
impl veoveo_types::Check for EntryWire {
    type Error = MediaPredictionError;
    fn check(&self) -> Result<(), Self::Error> {
        if &self.id != self.prediction_uri.id() {
            return Err(MediaPredictionError);
        }

        Ok(())
    }
}
impl TryFrom<EntryWire> for MediaPredictionEntry {
    type Error = MediaPredictionError;
    fn try_from(value: EntryWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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
impl veoveo_types::Check for PageWire {
    type Error = MediaPredictionError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != MEDIA_PREDICTION_PAGE_SIZE
            || self.items.len() > MEDIA_PREDICTION_PAGE_SIZE
            || self
                .items
                .windows(2)
                .any(|pair| pair[0].id() >= pair[1].id())
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != MEDIA_PREDICTION_PAGE_SIZE
                    || self.items.last().map(MediaPredictionEntry::id) != Some(cursor.after())
            })
        {
            return Err(MediaPredictionError);
        }

        Ok(())
    }
}
impl TryFrom<PageWire> for MediaPredictionPage {
    type Error = MediaPredictionError;
    fn try_from(value: PageWire) -> Result<Self, Self::Error> {
        let value = veoveo_types::Checked::new(value)?.into_inner();
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

#[doc(hidden)]
pub struct MediaPredictionErrorAddresses;
impl veoveo_types::ResourceProfile for MediaPredictionErrorAddresses {
    type Error = MediaPredictionError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| MediaPredictionError,
        };
}
