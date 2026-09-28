//! Version 1 cursor bodies retain their published field order and collection binding.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_types::{ResourceAddress, ResourceUriBuilder, TaskId};

use super::{UavResource, UavResourceError};
use crate::{
    contract::{ControlGrantId, LiveSessionId, LiveViewId, MissionId, MissionPlanId, SessionId},
    uris,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire<T> {
    version: u8,
    collection: String,
    position: T,
}

fn encode<T: Serialize>(collection: &str, position: &T) -> Result<String, UavResourceError> {
    let value = hex::encode(
        serde_json::to_vec(&Wire {
            version: 1,
            collection: collection.into(),
            position,
        })
        .map_err(|_| UavResourceError::InvalidCursor)?,
    );
    if value.len() > 2048 {
        return Err(UavResourceError::InvalidCursor);
    }
    Ok(value)
}
fn decode<T: DeserializeOwned>(collection: &str, value: &str) -> Result<T, UavResourceError> {
    if value.is_empty() || value.len() > 2048 {
        return Err(UavResourceError::InvalidCursor);
    }
    let bytes = hex::decode(value).map_err(|_| UavResourceError::InvalidCursor)?;
    let wire: Wire<T> =
        serde_json::from_slice(&bytes).map_err(|_| UavResourceError::InvalidCursor)?;
    if wire.version != 1 || wire.collection != collection {
        return Err(UavResourceError::InvalidCursor);
    }
    Ok(wire.position)
}

macro_rules! cursor {
    ($name:ident, $position:ty, $root:expr, $check:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            wire: String,
            position: $position,
        }
        impl $name {
            pub fn new(position: $position) -> Result<Self, UavResourceError> {
                if !($check)(&position) {
                    return Err(UavResourceError::InvalidCursor);
                }
                Ok(Self {
                    wire: encode($root, &position)?,
                    position,
                })
            }
            pub fn parse(value: impl Into<String>) -> Result<Self, UavResourceError> {
                let wire = value.into();
                let position: $position = decode($root, &wire)?;
                if !($check)(&position) {
                    return Err(UavResourceError::InvalidCursor);
                }
                Ok(Self { wire, position })
            }
            pub fn position(&self) -> &$position {
                &self.position
            }
            pub fn as_str(&self) -> &str {
                &self.wire
            }
        }
    };
}

cursor!(
    UavMissionCursor,
    MissionId,
    uris::MISSIONS,
    |_: &MissionId| true
);
cursor!(
    UavPlanCursor,
    MissionPlanId,
    uris::MISSION_PLANS,
    |_: &MissionPlanId| true
);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UavUsagePosition {
    pub created_at: DateTime<Utc>,
    pub task_id: TaskId,
}
cursor!(
    UavUsageCursor,
    UavUsagePosition,
    uris::USAGE,
    |position: &UavUsagePosition| position.task_id.as_uuid().get_version_num() == 7
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavGrantCursor {
    wire: String,
    session: Option<SessionId>,
    position: ControlGrantId,
}
impl UavGrantCursor {
    pub fn new(
        session: Option<&SessionId>,
        position: ControlGrantId,
    ) -> Result<Self, UavResourceError> {
        Ok(Self {
            wire: encode(&grant_collection(session)?, &position)?,
            session: session.cloned(),
            position,
        })
    }
    pub fn parse(
        session: Option<&SessionId>,
        value: impl Into<String>,
    ) -> Result<Self, UavResourceError> {
        let wire = value.into();
        let position = decode(&grant_collection(session)?, &wire)?;
        Ok(Self {
            wire,
            session: session.cloned(),
            position,
        })
    }
    pub fn session(&self) -> Option<&SessionId> {
        self.session.as_ref()
    }
    pub fn position(&self) -> &ControlGrantId {
        &self.position
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

fn grant_collection(session: Option<&SessionId>) -> Result<String, UavResourceError> {
    let mut uri = ResourceUriBuilder::new(uris::CONTROL_GRANTS)?;
    if let Some(session) = session {
        uri = uri.query_pair("active_session", session.as_str())?;
    }
    Ok(uri.build()?.into())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavLiveViewCursor {
    wire: String,
    session: LiveSessionId,
    position: LiveViewId,
}
impl UavLiveViewCursor {
    pub fn new(session: LiveSessionId, position: LiveViewId) -> Result<Self, UavResourceError> {
        let root = UavResource::LiveViews {
            session: session.clone(),
            cursor: None,
        }
        .to_uri()?;
        Ok(Self {
            wire: encode(root.as_str(), &position)?,
            session,
            position,
        })
    }
    pub fn parse(
        session: &LiveSessionId,
        value: impl Into<String>,
    ) -> Result<Self, UavResourceError> {
        let wire = value.into();
        let root = UavResource::LiveViews {
            session: session.clone(),
            cursor: None,
        }
        .to_uri()?;
        let position = decode(root.as_str(), &wire)?;
        Ok(Self {
            wire,
            session: session.clone(),
            position,
        })
    }
    pub fn session(&self) -> &LiveSessionId {
        &self.session
    }
    pub fn position(&self) -> &LiveViewId {
        &self.position
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
}
