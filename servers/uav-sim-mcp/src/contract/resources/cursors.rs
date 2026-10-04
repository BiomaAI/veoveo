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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UavUsagePosition {
    pub created_at: DateTime<Utc>,
    pub task_id: TaskId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct UavMissionCursorCodec;
impl veoveo_types::CursorCodec for UavMissionCursorCodec {
    type Position = MissionId;
    type Error = UavResourceError;
    fn check(&self, _position: &MissionId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &MissionId) -> Result<String, Self::Error> {
        encode(uris::MISSIONS, position)
    }
    fn decode(&self, wire: &str) -> Result<MissionId, Self::Error> {
        decode(uris::MISSIONS, wire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavMissionCursor {
    cursor: veoveo_types::OpaqueCursor<UavMissionCursorCodec>,
}
impl UavMissionCursor {
    pub fn new(position: MissionId) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::try_new(UavMissionCursorCodec, position)
            .map(|cursor| Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::parse(UavMissionCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn position(&self) -> &MissionId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct UavPlanCursorCodec;
impl veoveo_types::CursorCodec for UavPlanCursorCodec {
    type Position = MissionPlanId;
    type Error = UavResourceError;
    fn check(&self, _position: &MissionPlanId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &MissionPlanId) -> Result<String, Self::Error> {
        encode(uris::MISSION_PLANS, position)
    }
    fn decode(&self, wire: &str) -> Result<MissionPlanId, Self::Error> {
        decode(uris::MISSION_PLANS, wire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavPlanCursor {
    cursor: veoveo_types::OpaqueCursor<UavPlanCursorCodec>,
}
impl UavPlanCursor {
    pub fn new(position: MissionPlanId) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::try_new(UavPlanCursorCodec, position)
            .map(|cursor| Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::parse(UavPlanCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn position(&self) -> &MissionPlanId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct UavUsageCursorCodec;
impl veoveo_types::CursorCodec for UavUsageCursorCodec {
    type Position = UavUsagePosition;
    type Error = UavResourceError;
    fn check(&self, position: &UavUsagePosition) -> Result<(), Self::Error> {
        if position.task_id.as_uuid().get_version_num() == 7 {
            Ok(())
        } else {
            Err(UavResourceError::InvalidCursor)
        }
    }
    fn encode(&self, position: &UavUsagePosition) -> Result<String, Self::Error> {
        encode(uris::USAGE, position)
    }
    fn decode(&self, wire: &str) -> Result<UavUsagePosition, Self::Error> {
        decode(uris::USAGE, wire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavUsageCursor {
    cursor: veoveo_types::OpaqueCursor<UavUsageCursorCodec>,
}
impl UavUsageCursor {
    pub fn new(position: UavUsagePosition) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::try_new(UavUsageCursorCodec, position)
            .map(|cursor| Self { cursor })
    }
    pub fn parse(wire: impl Into<String>) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::parse(UavUsageCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn position(&self) -> &UavUsagePosition {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct GrantCursorCodec {
    session: Option<SessionId>,
}
impl veoveo_types::CursorCodec for GrantCursorCodec {
    type Position = ControlGrantId;
    type Error = UavResourceError;
    fn check(&self, _position: &ControlGrantId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &ControlGrantId) -> Result<String, Self::Error> {
        encode(&grant_collection(self.session.as_ref())?, position)
    }
    fn decode(&self, wire: &str) -> Result<ControlGrantId, Self::Error> {
        decode(&grant_collection(self.session.as_ref())?, wire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavGrantCursor {
    cursor: veoveo_types::OpaqueCursor<GrantCursorCodec>,
}
impl UavGrantCursor {
    pub fn new(
        session: Option<&SessionId>,
        position: ControlGrantId,
    ) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::try_new(
            GrantCursorCodec {
                session: session.cloned(),
            },
            position,
        )
        .map(|cursor| Self { cursor })
    }
    pub fn parse(
        session: Option<&SessionId>,
        wire: impl Into<String>,
    ) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::parse(
            GrantCursorCodec {
                session: session.cloned(),
            },
            wire,
        )
        .map(|cursor| Self { cursor })
    }
    pub fn session(&self) -> Option<&SessionId> {
        self.cursor.codec().session.as_ref()
    }
    pub fn position(&self) -> &ControlGrantId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
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
struct LiveViewCursorCodec {
    session: LiveSessionId,
}
impl LiveViewCursorCodec {
    fn collection(&self) -> Result<veoveo_types::ResourceUri, UavResourceError> {
        UavResource::LiveViews {
            session: self.session.clone(),
            cursor: None,
        }
        .to_uri()
    }
}
impl veoveo_types::CursorCodec for LiveViewCursorCodec {
    type Position = LiveViewId;
    type Error = UavResourceError;
    fn check(&self, _position: &LiveViewId) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &LiveViewId) -> Result<String, Self::Error> {
        encode(self.collection()?.as_str(), position)
    }
    fn decode(&self, wire: &str) -> Result<LiveViewId, Self::Error> {
        decode(self.collection()?.as_str(), wire)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UavLiveViewCursor {
    cursor: veoveo_types::OpaqueCursor<LiveViewCursorCodec>,
}
impl UavLiveViewCursor {
    pub fn new(session: LiveSessionId, position: LiveViewId) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::try_new(LiveViewCursorCodec { session }, position)
            .map(|cursor| Self { cursor })
    }
    pub fn parse(
        session: &LiveSessionId,
        wire: impl Into<String>,
    ) -> Result<Self, UavResourceError> {
        veoveo_types::OpaqueCursor::parse(
            LiveViewCursorCodec {
                session: session.clone(),
            },
            wire,
        )
        .map(|cursor| Self { cursor })
    }
    pub fn session(&self) -> &LiveSessionId {
        &self.cursor.codec().session
    }
    pub fn position(&self) -> &LiveViewId {
        self.cursor.position()
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_cursors_preserve_alias_text_and_reject_other_sessions() {
        let session = SessionId::new("session-alpha").unwrap();
        let other = SessionId::new("session-beta").unwrap();
        let grant = ControlGrantId::new("grant-alpha").unwrap();
        let cursor = UavGrantCursor::new(Some(&session), grant.clone()).unwrap();
        let alias = cursor.as_str().to_ascii_uppercase();
        let parsed = UavGrantCursor::parse(Some(&session), alias.clone()).unwrap();
        assert_eq!(parsed.as_str(), alias);
        assert_eq!(parsed.position(), &grant);
        let context: Option<&SessionId> = parsed.session();
        assert_eq!(context, Some(&session));
        assert!(UavGrantCursor::parse(Some(&other), &alias).is_err());
        assert!(UavGrantCursor::parse(None, &alias).is_err());

        let session = LiveSessionId::new("live-alpha").unwrap();
        let other = LiveSessionId::new("live-beta").unwrap();
        let view = LiveViewId::new("view-alpha").unwrap();
        let cursor = UavLiveViewCursor::new(session.clone(), view.clone()).unwrap();
        let alias = cursor.as_str().to_ascii_uppercase();
        let parsed = UavLiveViewCursor::parse(&session, alias.clone()).unwrap();
        assert_eq!(parsed.as_str(), alias);
        let context: &LiveSessionId = parsed.session();
        assert_eq!(context, &session);
        assert_eq!(parsed.position(), &view);
        assert!(UavLiveViewCursor::parse(&other, &alias).is_err());
    }
}
