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
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapSourceCursor {
    wire: String,
    after: MapSourceId,
}
impl MapSourceCursor {
    pub fn new(after: MapSourceId) -> Self {
        Self {
            wire: hex::encode(
                serde_json::to_vec(&CursorWire {
                    version: 1,
                    collection: MapSourcesUri::ROOT.to_owned(),
                    after: after.clone(),
                })
                .expect("closed source cursor"),
            ),
            after,
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MapSourceError> {
        let wire = wire.into();
        if wire.len() > 1024 {
            return Err(MapSourceError::Cursor);
        }
        let cursor: CursorWire =
            serde_json::from_slice(&hex::decode(&wire).map_err(|_| MapSourceError::Cursor)?)
                .map_err(|_| MapSourceError::Cursor)?;
        if cursor.version != 1 || cursor.collection != MapSourcesUri::ROOT {
            return Err(MapSourceError::Cursor);
        }
        let admitted = Self::new(cursor.after);
        if admitted.wire != wire {
            return Err(MapSourceError::Cursor);
        }
        Ok(admitted)
    }
    pub fn after(&self) -> &MapSourceId {
        &self.after
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
impl TryFrom<String> for MapSourceCursor {
    type Error = MapSourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MapSourceCursor> for String {
    fn from(value: MapSourceCursor) -> Self {
        value.wire
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
impl TryFrom<PageWire> for MapSourcePage {
    type Error = MapSourceError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        if wire.limit != SOURCE_PAGE_SIZE
            || wire.items.len() > SOURCE_PAGE_SIZE
            || wire.next_cursor.as_ref().is_some_and(|cursor| {
                wire.items.len() != SOURCE_PAGE_SIZE
                    || wire
                        .items
                        .last()
                        .is_none_or(|item| item.source_id() != cursor.after())
            })
        {
            return Err(MapSourceError::Page);
        }
        check_order(&wire.items)?;
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
