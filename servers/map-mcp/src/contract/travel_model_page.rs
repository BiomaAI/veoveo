use super::{TravelModelRecord, TravelModelUriError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, TaskId};

pub const TRAVEL_MODEL_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct CursorWire {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "String")]
pub struct MapTravelModelCursor {
    cursor: veoveo_types::OpaqueCursor<MapTravelModelCursorCodec>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MapTravelModelCursorCodec;
impl veoveo_types::StatelessCursorCodec for MapTravelModelCursorCodec {}
impl veoveo_types::CursorCodec for MapTravelModelCursorCodec {
    type Position = TaskId;
    type Error = TravelModelUriError;
    fn check(&self, position: &Self::Position) -> Result<(), Self::Error> {
        if position.as_uuid().get_version_num() != 7
            || position.as_uuid().get_variant() != uuid::Variant::RFC4122
        {
            Err(TravelModelUriError)
        } else {
            Ok(())
        }
    }
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error> {
        let bytes = serde_json::to_vec(&CursorWire {
            version: 2,
            task_id: *position,
        })
        .expect("closed owner cursor fields serialize");
        Ok(hex::encode(bytes))
    }
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error> {
        if wire.is_empty() || wire.len() > 1024 {
            return Err(TravelModelUriError);
        }
        let bytes = hex::decode(wire).map_err(|_| TravelModelUriError)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| TravelModelUriError)?;
        if decoded.version != 2 {
            return Err(TravelModelUriError);
        }
        let position = decoded.task_id;
        self.check(&position)?;
        if self.encode(&position)? != wire {
            return Err(TravelModelUriError);
        }
        Ok(position)
    }
}
impl MapTravelModelCursor {
    pub fn new(task_id: TaskId) -> Result<Self, TravelModelUriError> {
        let cursor = veoveo_types::OpaqueCursor::try_new(MapTravelModelCursorCodec, task_id)?;
        Ok(Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, TravelModelUriError> {
        veoveo_types::OpaqueCursor::parse(MapTravelModelCursorCodec, wire)
            .map(|cursor| Self { cursor })
    }
    pub fn task_id(&self) -> TaskId {
        *self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapTravelModelsUri {
    wire: ResourceUri,
    cursor: Option<MapTravelModelCursor>,
}
impl MapTravelModelsUri {
    pub const ROOT: &str = "map://travel-models";
    pub const TEMPLATE: &str = "map://travel-models{?cursor}";
    pub fn new(cursor: Option<MapTravelModelCursor>) -> Self {
        let mut builder = ResourceUriBuilder::new(Self::ROOT).expect("declared Map root");
        if let Some(cursor) = &cursor {
            builder = builder
                .query_pair("cursor", cursor.as_str())
                .expect("typed cursor");
        }
        Self {
            wire: builder.build().expect("typed travel-model collection"),
            cursor,
        }
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TravelModelUriError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| TravelModelUriError)?;
        if parts.scheme() != "map"
            || parts.authority() != "travel-models"
            || parts.path_segments().next().is_some()
        {
            return Err(TravelModelUriError);
        }
        let cursor = if parts.has_query() {
            let query = parts.query_parameters();
            if query.len() != 1 {
                return Err(TravelModelUriError);
            }
            Some(MapTravelModelCursor::parse(
                query.get("cursor").ok_or(TravelModelUriError)?.clone(),
            )?)
        } else {
            None
        };
        let address = Self::new(cursor);
        if address.as_str() != value {
            return Err(TravelModelUriError);
        }
        Ok(address)
    }
    pub fn cursor(&self) -> Option<&MapTravelModelCursor> {
        self.cursor.as_ref()
    }
    pub fn as_str(&self) -> &str {
        self.wire.as_str()
    }
}
impl TryFrom<String> for MapTravelModelsUri {
    type Error = TravelModelUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MapTravelModelsUri> for String {
    fn from(value: MapTravelModelsUri) -> Self {
        value.wire.to_string()
    }
}
impl ResourceAddress for MapTravelModelsUri {
    type Error = TravelModelUriError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.wire.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MapTravelModelPage {
    pub items: Vec<TravelModelRecord>,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<MapTravelModelCursor>,
}

#[cfg(test)]
mod naming_cut_tests {
    use super::*;
    #[test]
    fn current_task_cursor_refuses_old_revision_and_field_names() {
        let cursor = MapTravelModelCursor::new(TaskId::new()).unwrap();
        assert_eq!(
            MapTravelModelCursor::parse(cursor.as_str()).unwrap(),
            cursor
        );
        let current: serde_json::Value =
            serde_json::from_slice(&hex::decode(cursor.as_str()).unwrap()).unwrap();
        assert_eq!(current["version"], 2);
        for mixed in [false, true] {
            let mut invalid = current.clone();
            invalid["task_id"] = invalid["taskId"].clone();
            if !mixed {
                invalid.as_object_mut().unwrap().remove("taskId");
            }
            assert!(
                MapTravelModelCursor::parse(hex::encode(serde_json::to_vec(&invalid).unwrap()))
                    .is_err()
            );
        }
        let mut old = current;
        old["version"] = serde_json::json!(1);
        assert!(
            MapTravelModelCursor::parse(hex::encode(serde_json::to_vec(&old).unwrap())).is_err()
        );
    }
}
