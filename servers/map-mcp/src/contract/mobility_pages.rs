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
#[serde(transparent)]
#[schemars(with = "String")]
pub struct MapMobilityProfileCursor {
    cursor: veoveo_types::OpaqueCursor<MapMobilityProfileCursorCodec>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct MapMobilityProfileCursorPosition {
    after_id: MobilityProfileId,
    after_version: MobilityProfileVersion,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MapMobilityProfileCursorCodec;
impl veoveo_types::StatelessCursorCodec for MapMobilityProfileCursorCodec {}
impl veoveo_types::CursorCodec for MapMobilityProfileCursorCodec {
    type Position = MapMobilityProfileCursorPosition;
    type Error = MapMobilityError;
    fn check(&self, _position: &Self::Position) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 1,
            collection: (MapMobilityProfilesUri::ROOT).to_owned(),
            after_id: position.after_id.clone(),
            after_version: position.after_version,
        })
        .expect("closed owner cursor fields serialize");
        Ok(hex::encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(MapMobilityError::Cursor);
        }
        let bytes = hex::decode(wire).map_err(|_| MapMobilityError::Cursor)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| MapMobilityError::Cursor)?;
        if decoded.version != 1 || decoded.collection != MapMobilityProfilesUri::ROOT {
            return Err(MapMobilityError::Cursor);
        }
        let position = MapMobilityProfileCursorPosition {
            after_id: decoded.after_id,
            after_version: decoded.after_version,
        };
        self.check(&position)?;
        if self.encode(&position)? != wire {
            return Err(MapMobilityError::Cursor);
        }
        Ok(position)
    }
}
impl MapMobilityProfileCursor {
    pub fn new(after_id: MobilityProfileId, after_version: MobilityProfileVersion) -> Self {
        let cursor = veoveo_types::OpaqueCursor::try_new(
            MapMobilityProfileCursorCodec,
            MapMobilityProfileCursorPosition {
                after_id,
                after_version,
            },
        )
        .expect("typed cursor position");
        Self { cursor }
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, MapMobilityError> {
        veoveo_types::OpaqueCursor::parse(MapMobilityProfileCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn after_id(&self) -> &MobilityProfileId {
        &self.cursor.position().after_id
    }
    pub fn after_version(&self) -> MobilityProfileVersion {
        self.cursor.position().after_version
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
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
impl veoveo_types::Check for PageWire {
    type Error = MapMobilityError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.limit != MOBILITY_PROFILE_PAGE_SIZE
            || self.items.len() > MOBILITY_PROFILE_PAGE_SIZE
            || self.next_cursor.as_ref().is_some_and(|cursor| {
                self.items.len() != MOBILITY_PROFILE_PAGE_SIZE
                    || self
                        .items
                        .last()
                        .is_none_or(|item| key(item) != (cursor.after_id(), cursor.after_version()))
            })
        {
            return Err(MapMobilityError::Page);
        }
        check_order(&self.items)?;

        Ok(())
    }
}
impl TryFrom<PageWire> for MapMobilityProfilePage {
    type Error = MapMobilityError;
    fn try_from(wire: PageWire) -> Result<Self, Self::Error> {
        let wire = veoveo_types::Checked::new(wire)?.into_inner();
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
