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
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapRestrictionCursor {
    wire: String,
    after: RestrictionId,
}
impl MapRestrictionCursor {
    pub fn new(after: RestrictionId) -> Self {
        Self {
            wire: hex::encode(
                serde_json::to_vec(&CursorWire {
                    version: 1,
                    collection: MapRestrictionsUri::ROOT.to_owned(),
                    after: after.clone(),
                })
                .expect("closed restriction cursor"),
            ),
            after,
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MapRestrictionError> {
        let wire = wire.into();
        if wire.len() > 1024 {
            return Err(MapRestrictionError::Cursor);
        }
        let cursor: CursorWire =
            serde_json::from_slice(&hex::decode(&wire).map_err(|_| MapRestrictionError::Cursor)?)
                .map_err(|_| MapRestrictionError::Cursor)?;
        if cursor.version != 1 || cursor.collection != MapRestrictionsUri::ROOT {
            return Err(MapRestrictionError::Cursor);
        }
        let admitted = Self::new(cursor.after);
        if admitted.wire != wire {
            return Err(MapRestrictionError::Cursor);
        }
        Ok(admitted)
    }
    pub fn after(&self) -> &RestrictionId {
        &self.after
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
impl TryFrom<String> for MapRestrictionCursor {
    type Error = MapRestrictionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MapRestrictionCursor> for String {
    fn from(value: MapRestrictionCursor) -> Self {
        value.wire
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
impl TryFrom<PageWire> for MapRestrictionPage {
    type Error = MapRestrictionError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        if wire.limit != RESTRICTION_PAGE_SIZE
            || wire.items.len() > RESTRICTION_PAGE_SIZE
            || wire.next_cursor.as_ref().is_some_and(|cursor| {
                wire.items.len() != RESTRICTION_PAGE_SIZE
                    || wire
                        .items
                        .last()
                        .is_none_or(|item| item.restriction_id() != cursor.after())
            })
        {
            return Err(MapRestrictionError::Page);
        }
        check_order(&wire.items)?;
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
