use super::{MapRestrictionError, MapRestrictionsUri, RestrictionId, RestrictionSummary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const RESTRICTION_PAGE_SIZE: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: String,
    after: RestrictionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "String")]
pub struct MapRestrictionCursor {
    cursor: veoveo_types::OpaqueCursor<MapRestrictionCursorCodec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MapRestrictionCursorCodec;
impl veoveo_types::StatelessCursorCodec for MapRestrictionCursorCodec {}
impl veoveo_types::CursorCodec for MapRestrictionCursorCodec {
    type Position = RestrictionId;
    type Error = MapRestrictionError;
    fn check(&self, _position: &Self::Position) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (MapRestrictionsUri::ROOT).to_owned(),
            after: position.clone(),
        })
        .expect("closed owner cursor fields serialize");
        Ok(hex::encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(MapRestrictionError::Cursor);
        }
        let bytes = hex::decode(wire).map_err(|_| MapRestrictionError::Cursor)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| MapRestrictionError::Cursor)?;
        if decoded.version != 1 || decoded.collection != MapRestrictionsUri::ROOT {
            return Err(MapRestrictionError::Cursor);
        }
        let position = decoded.after;
        self.check(&position)?;
        if self.encode(&position)? != wire {
            return Err(MapRestrictionError::Cursor);
        }
        Ok(position)
    }
}
impl MapRestrictionCursor {
    pub fn new(after: RestrictionId) -> Self {
        let cursor = veoveo_types::OpaqueCursor::try_new(MapRestrictionCursorCodec, after)
            .expect("typed cursor position");
        Self { cursor }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MapRestrictionError> {
        veoveo_types::OpaqueCursor::parse(MapRestrictionCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn after(&self) -> &RestrictionId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct MapRestrictionPage {
    items: Vec<RestrictionSummary>,
    next_cursor: Option<MapRestrictionCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<RestrictionSummary>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<MapRestrictionCursor>,
}

impl MapRestrictionPage {
    /// SQL readers supply at most 101 rows in strictly increasing ID order.
    /// The last row is lookahead; all returned records retain typed identities.
    pub fn from_lookahead(mut items: Vec<RestrictionSummary>) -> Result<Self, MapRestrictionError> {
        if items.len() > RESTRICTION_PAGE_SIZE + 1 {
            return Err(MapRestrictionError::Page);
        }
        let more = items.len() > RESTRICTION_PAGE_SIZE;
        check_order(&items)?;
        items.truncate(RESTRICTION_PAGE_SIZE);
        let next_cursor = more.then(|| {
            MapRestrictionCursor::new(items.last().expect("full page").restriction_id().clone())
        });
        Ok(Self { items, next_cursor })
    }
    pub fn items(&self) -> &[RestrictionSummary] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&MapRestrictionCursor> {
        self.next_cursor.as_ref()
    }
}

fn check_order(items: &[RestrictionSummary]) -> Result<(), MapRestrictionError> {
    if items
        .windows(2)
        .any(|pair| pair[0].restriction_id() >= pair[1].restriction_id())
    {
        return Err(MapRestrictionError::Page);
    }
    Ok(())
}
impl veoveo_types::Check for PageWire {
    type Error = MapRestrictionError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != RESTRICTION_PAGE_SIZE
            || self.items.len() > RESTRICTION_PAGE_SIZE
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != RESTRICTION_PAGE_SIZE
                    || self
                        .items
                        .last()
                        .is_none_or(|item| item.restriction_id() != cursor.after())
            })
        {
            return Err(MapRestrictionError::Page);
        }
        check_order(&self.items)?;

        Ok(())
    }
}
impl TryFrom<PageWire> for MapRestrictionPage {
    type Error = MapRestrictionError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        let wire = veoveo_types::Checked::new(wire)?.into_inner();
        Ok(Self {
            items: wire.items,
            next_cursor: wire.next_cursor,
        })
    }
}
impl From<MapRestrictionPage> for PageWire {
    fn from(page: MapRestrictionPage) -> Self {
        Self {
            items: page.items,
            limit: RESTRICTION_PAGE_SIZE,
            next_cursor: page.next_cursor,
        }
    }
}
