use super::{
    MapMobilityError, MapMobilityProfilesUri, MobilityProfile, MobilityProfileId,
    MobilityProfileVersion,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MOBILITY_PROFILE_PAGE_SIZE: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    collection: String,
    after_id: MobilityProfileId,
    after_version: MobilityProfileVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapMobilityProfileCursor {
    wire: String,
    after_id: MobilityProfileId,
    after_version: MobilityProfileVersion,
}
impl MapMobilityProfileCursor {
    pub fn new(after_id: MobilityProfileId, after_version: MobilityProfileVersion) -> Self {
        Self {
            wire: hex::encode(
                serde_json::to_vec(&CursorWire {
                    version: 1,
                    collection: MapMobilityProfilesUri::ROOT.to_owned(),
                    after_id: after_id.clone(),
                    after_version,
                })
                .expect("closed mobility profile cursor"),
            ),
            after_id,
            after_version,
        }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MapMobilityError> {
        let wire = wire.into();
        if wire.len() > 1024 {
            return Err(MapMobilityError::Cursor);
        }
        let cursor: CursorWire =
            serde_json::from_slice(&hex::decode(&wire).map_err(|_| MapMobilityError::Cursor)?)
                .map_err(|_| MapMobilityError::Cursor)?;
        if cursor.version != 1 || cursor.collection != MapMobilityProfilesUri::ROOT {
            return Err(MapMobilityError::Cursor);
        }
        let admitted = Self::new(cursor.after_id, cursor.after_version);
        if admitted.wire != wire {
            return Err(MapMobilityError::Cursor);
        }
        Ok(admitted)
    }
    pub fn after_id(&self) -> &MobilityProfileId {
        &self.after_id
    }
    pub fn after_version(&self) -> MobilityProfileVersion {
        self.after_version
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
impl TryFrom<String> for MapMobilityProfileCursor {
    type Error = MapMobilityError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MapMobilityProfileCursor> for String {
    fn from(value: MapMobilityProfileCursor) -> Self {
        value.wire
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "PageWire", into = "PageWire")]
pub struct MapMobilityProfilePage {
    items: Vec<MobilityProfile>,
    next_cursor: Option<MapMobilityProfileCursor>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PageWire {
    #[schemars(length(max = 100))]
    items: Vec<MobilityProfile>,
    #[schemars(range(min = 100, max = 100))]
    limit: usize,
    next_cursor: Option<MapMobilityProfileCursor>,
}

impl MapMobilityProfilePage {
    /// SQL readers supply at most 101 rows in strictly increasing (ID, numeric version) order.
    /// The last row is lookahead; all returned records retain typed identities.
    pub fn from_lookahead(mut items: Vec<MobilityProfile>) -> Result<Self, MapMobilityError> {
        if items.len() > MOBILITY_PROFILE_PAGE_SIZE + 1 {
            return Err(MapMobilityError::Page);
        }
        let more = items.len() > MOBILITY_PROFILE_PAGE_SIZE;
        check_order(&items)?;
        items.truncate(MOBILITY_PROFILE_PAGE_SIZE);
        let next_cursor = more.then(|| {
            let last = items.last().expect("full page").metadata();
            MapMobilityProfileCursor::new(last.profile_id.clone(), last.version)
        });
        Ok(Self { items, next_cursor })
    }
    pub fn items(&self) -> &[MobilityProfile] {
        &self.items
    }
    pub fn next_cursor(&self) -> Option<&MapMobilityProfileCursor> {
        self.next_cursor.as_ref()
    }
}

fn check_order(items: &[MobilityProfile]) -> Result<(), MapMobilityError> {
    for item in items {
        item.validate().map_err(|_| MapMobilityError::Metadata)?;
    }
    if items.windows(2).any(|pair| key(&pair[0]) >= key(&pair[1])) {
        return Err(MapMobilityError::Page);
    }
    Ok(())
}
impl TryFrom<PageWire> for MapMobilityProfilePage {
    type Error = MapMobilityError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        if wire.limit != MOBILITY_PROFILE_PAGE_SIZE
            || wire.items.len() > MOBILITY_PROFILE_PAGE_SIZE
            || wire.next_cursor.as_ref().is_some_and(|cursor| {
                wire.items.len() != MOBILITY_PROFILE_PAGE_SIZE
                    || wire
                        .items
                        .last()
                        .is_none_or(|item| key(item) != (cursor.after_id(), cursor.after_version()))
            })
        {
            return Err(MapMobilityError::Page);
        }
        check_order(&wire.items)?;
        Ok(Self {
            items: wire.items,
            next_cursor: wire.next_cursor,
        })
    }
}
impl From<MapMobilityProfilePage> for PageWire {
    fn from(page: MapMobilityProfilePage) -> Self {
        Self {
            items: page.items,
            limit: MOBILITY_PROFILE_PAGE_SIZE,
            next_cursor: page.next_cursor,
        }
    }
}

fn key(profile: &MobilityProfile) -> (&MobilityProfileId, MobilityProfileVersion) {
    let meta = profile.metadata();
    (&meta.profile_id, meta.version)
}
