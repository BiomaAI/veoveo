use super::{TravelModelRecord, TravelModelUriError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, TaskId};

pub const TRAVEL_MODEL_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorWire {
    version: u8,
    task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub struct MapTravelModelCursor {
    wire: String,
    task_id: TaskId,
}
impl MapTravelModelCursor {
    pub fn new(task_id: TaskId) -> Result<Self, TravelModelUriError> {
        if task_id.as_uuid().get_version_num() != 7
            || task_id.as_uuid().get_variant() != uuid::Variant::RFC4122
        {
            return Err(TravelModelUriError);
        }
        Ok(Self {
            wire: hex::encode(
                serde_json::to_vec(&CursorWire {
                    version: 1,
                    task_id,
                })
                .expect("closed cursor"),
            ),
            task_id,
        })
    }
    pub fn parse(value: impl Into<String>) -> Result<Self, TravelModelUriError> {
        let wire = value.into();
        if wire.len() > 1024 {
            return Err(TravelModelUriError);
        }
        let cursor: CursorWire =
            serde_json::from_slice(&hex::decode(&wire).map_err(|_| TravelModelUriError)?)
                .map_err(|_| TravelModelUriError)?;
        if cursor.version != 1 {
            return Err(TravelModelUriError);
        }
        let admitted = Self::new(cursor.task_id)?;
        if admitted.wire != wire {
            return Err(TravelModelUriError);
        }
        Ok(admitted)
    }
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
impl TryFrom<String> for MapTravelModelCursor {
    type Error = TravelModelUriError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<MapTravelModelCursor> for String {
    fn from(value: MapTravelModelCursor) -> Self {
        value.wire
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
pub struct MapTravelModelPage {
    pub items: Vec<TravelModelRecord>,
    pub limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<MapTravelModelCursor>,
}
