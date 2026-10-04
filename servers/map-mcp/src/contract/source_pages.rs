use super::{MapSourceError, MapSourceId, MapSourcesUri, SourceSummary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const SOURCE_PAGE_SIZE: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: String,
    after: MapSourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "String")]
pub struct MapSourceCursor {
    cursor: veoveo_types::OpaqueCursor<MapSourceCursorCodec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MapSourceCursorCodec;
impl veoveo_types::StatelessCursorCodec for MapSourceCursorCodec {}
impl veoveo_types::CursorCodec for MapSourceCursorCodec {
    type Position = MapSourceId;
    type Error = MapSourceError;
    fn check(&self, _position: &Self::Position) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (MapSourcesUri::ROOT).to_owned(),
            after: position.clone(),
        })
        .expect("closed owner cursor fields serialize");
        Ok(hex::encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(MapSourceError::Cursor);
        }
        let bytes = hex::decode(wire).map_err(|_| MapSourceError::Cursor)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| MapSourceError::Cursor)?;
        if decoded.version != 1 || decoded.collection != MapSourcesUri::ROOT {
            return Err(MapSourceError::Cursor);
        }
        let position = decoded.after;
        self.check(&position)?;
        if self.encode(&position)? != wire {
            return Err(MapSourceError::Cursor);
        }
        Ok(position)
    }
}
impl MapSourceCursor {
    pub fn new(after: MapSourceId) -> Self {
        let cursor = veoveo_types::OpaqueCursor::try_new(MapSourceCursorCodec, after)
            .expect("typed cursor position");
        Self { cursor }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MapSourceError> {
        veoveo_types::OpaqueCursor::parse(MapSourceCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> &MapSourceId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct MapSourcePage {
    items: Vec<SourceSummary>,
    next_cursor: Option<MapSourceCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<SourceSummary>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<MapSourceCursor>,
}

impl MapSourcePage {
    /// SQL readers supply at most 101 rows in strictly increasing ID order.
    /// The last row is lookahead; all returned records retain typed identities.
    pub fn from_lookahead(mut items: Vec<SourceSummary>) -> Result<Self, MapSourceError> {
        if items.len() > SOURCE_PAGE_SIZE + 1 {
            return Err(MapSourceError::Page);
        }
        let more = items.len() > SOURCE_PAGE_SIZE;
        check_order(&items)?;
        items.truncate(SOURCE_PAGE_SIZE);
        let next_cursor = more
            .then(|| MapSourceCursor::new(items.last().expect("full page").source_id().clone()));
        Ok(Self { items, next_cursor })
    }
    pub fn items(&self) -> &[SourceSummary] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&MapSourceCursor> {
        self.next_cursor.as_ref()
    }
}

fn check_order(items: &[SourceSummary]) -> Result<(), MapSourceError> {
    if items
        .windows(2)
        .any(|pair| pair[0].source_id() >= pair[1].source_id())
    {
        return Err(MapSourceError::Page);
    }
    Ok(())
}
impl veoveo_types::Check for PageWire {
    type Error = MapSourceError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != SOURCE_PAGE_SIZE
            || self.items.len() > SOURCE_PAGE_SIZE
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != SOURCE_PAGE_SIZE
                    || self
                        .items
                        .last()
                        .is_none_or(|item| item.source_id() != cursor.after())
            })
        {
            return Err(MapSourceError::Page);
        }
        check_order(&self.items)?;

        Ok(())
    }
}
impl TryFrom<PageWire> for MapSourcePage {
    type Error = MapSourceError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        let wire = veoveo_types::Checked::new(wire)?.into_inner();
        Ok(Self {
            items: wire.items,
            next_cursor: wire.next_cursor,
        })
    }
}
impl From<MapSourcePage> for PageWire {
    fn from(page: MapSourcePage) -> Self {
        Self {
            items: page.items,
            limit: SOURCE_PAGE_SIZE,
            next_cursor: page.next_cursor,
        }
    }
}
