//! UAV resource vocabulary. URI libraries own parsing and encoding.
use super::{
    ControlGrantId, LiveCameraId, LiveSessionId, LiveStreamProductId, LiveViewId, MissionId,
    MissionPlanId, SessionId, VehicleId,
};
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts, TaskId,
    UriSegment,
};
mod cursors;
pub use cursors::{
    UavGrantCursor, UavLiveViewCursor, UavMissionCursor, UavPlanCursor, UavUsageCursor,
    UavUsagePosition,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UavResourceError {
    Uri(ResourceUriError),
    UnknownResource,
    InvalidIdentifier,
    InvalidCursor,
}
impl From<ResourceUriError> for UavResourceError {
    fn from(value: ResourceUriError) -> Self {
        Self::Uri(value)
    }
}
impl fmt::Display for UavResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(error) => error.fmt(f),
            Self::UnknownResource => f.write_str("unknown or noncanonical UAV resource"),
            Self::InvalidIdentifier => f.write_str("invalid UAV resource identifier"),
            Self::InvalidCursor => f.write_str("invalid UAV cursor or collection parent"),
        }
    }
}
impl Error for UavResourceError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UavDocument {
    Agents,
    Design,
}
impl UavDocument {
    pub fn id(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
    pub fn parse(id: &str) -> Result<Self, UavResourceError> {
        match id {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(UavResourceError::UnknownResource),
        }
    }
}

/// Addresses keep their domain IDs and collection-specific cursors.
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::{UavResource, VehicleId};
/// let address = UavResource::Session(VehicleId::new("vehicle").unwrap());
/// ```
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::{UavResource, UavPlanCursor};
/// fn page(cursor: UavPlanCursor) { let _ = UavResource::Missions { cursor: Some(cursor) }; }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, JsonSchema)]
#[schemars(with = "String")]
pub enum UavResource {
    Docs,
    Document(UavDocument),
    Contract,
    LiveApp,
    Sessions,
    Session(SessionId),
    World(SessionId),
    Tiles(SessionId),
    Vehicles(SessionId),
    Recordings(SessionId),
    Vehicle {
        session: SessionId,
        vehicle: VehicleId,
    },
    LiveCameras(LiveSessionId),
    LiveCamera {
        session: LiveSessionId,
        camera: LiveCameraId,
    },
    StreamProducts(LiveSessionId),
    StreamProduct {
        session: LiveSessionId,
        product: LiveStreamProductId,
    },
    LiveViews {
        session: LiveSessionId,
        cursor: Option<UavLiveViewCursor>,
    },
    LiveView {
        session: LiveSessionId,
        view: LiveViewId,
    },
    ControlGrants {
        cursor: Option<UavGrantCursor>,
    },
    ControlGrant(ControlGrantId),
    MissionPlans {
        cursor: Option<UavPlanCursor>,
    },
    MissionPlan(MissionPlanId),
    Missions {
        cursor: Option<UavMissionCursor>,
    },
    Mission(MissionId),
    Usage {
        cursor: Option<UavUsageCursor>,
    },
    UsageTask(TaskId),
}

fn id<T: std::str::FromStr>(value: &str) -> Result<T, UavResourceError> {
    value
        .parse()
        .map_err(|_| UavResourceError::InvalidIdentifier)
}
fn query(parts: &ResourceUriParts) -> Result<Option<&str>, UavResourceError> {
    if !parts.has_query() {
        return Ok(None);
    }
    if parts.query_parameters().len() != 1 {
        return Err(UavResourceError::InvalidCursor);
    }
    parts
        .query_parameters()
        .get("cursor")
        .map(|s| Some(s.as_str()))
        .ok_or(UavResourceError::InvalidCursor)
}
impl UavResource {
    pub fn parse(value: &str) -> Result<Self, UavResourceError> {
        let parts = ResourceUriParts::parse(value)?;
        let decoded: Vec<_> = parts.path_segments().collect();
        let path: Vec<_> = decoded.iter().map(|s| s.as_ref()).collect();
        let route = (parts.scheme(), parts.authority(), path.as_slice());
        let resource = match route {
            ("uav-sim", "control-grants", []) => Self::ControlGrants {
                cursor: query(&parts)?
                    .map(|c| UavGrantCursor::parse(None, c))
                    .transpose()?,
            },
            ("uav-sim", "mission-plans", []) => Self::MissionPlans {
                cursor: query(&parts)?.map(UavPlanCursor::parse).transpose()?,
            },
            ("uav-sim", "missions", []) => Self::Missions {
                cursor: query(&parts)?.map(UavMissionCursor::parse).transpose()?,
            },
            ("uav-sim", "usage", []) => Self::Usage {
                cursor: query(&parts)?.map(UavUsageCursor::parse).transpose()?,
            },
            ("uav-sim", "session", [session, "live-views"]) => {
                let session = id(session)?;
                let cursor = query(&parts)?
                    .map(|c| UavLiveViewCursor::parse(&session, c))
                    .transpose()?;
                Self::LiveViews { session, cursor }
            }
            _ => {
                if parts.has_query() {
                    return Err(UavResourceError::UnknownResource);
                }
                match route {
                    ("uav-sim", "docs", []) => Self::Docs,
                    ("uav-sim", "docs", [doc]) => Self::Document(UavDocument::parse(doc)?),
                    ("uav-sim", "contract", []) => Self::Contract,
                    ("ui", "uav-sim", ["live.html"]) => Self::LiveApp,
                    ("uav-sim", "sessions", []) => Self::Sessions,
                    ("uav-sim", "session", [session]) => Self::Session(id(session)?),
                    ("uav-sim", "session", [session, "world"]) => Self::World(id(session)?),
                    ("uav-sim", "session", [session, "tiles"]) => Self::Tiles(id(session)?),
                    ("uav-sim", "session", [session, "vehicles"]) => Self::Vehicles(id(session)?),
                    ("uav-sim", "session", [session, "recordings"]) => {
                        Self::Recordings(id(session)?)
                    }
                    ("uav-sim", "session", [session, "vehicle", vehicle]) => Self::Vehicle {
                        session: id(session)?,
                        vehicle: id(vehicle)?,
                    },
                    ("uav-sim", "session", [session, "live-cameras"]) => {
                        Self::LiveCameras(id(session)?)
                    }
                    ("uav-sim", "session", [session, "live-camera", camera]) => Self::LiveCamera {
                        session: id(session)?,
                        camera: id(camera)?,
                    },
                    ("uav-sim", "session", [session, "stream-products"]) => {
                        Self::StreamProducts(id(session)?)
                    }
                    ("uav-sim", "session", [session, "stream-product", product]) => {
                        Self::StreamProduct {
                            session: id(session)?,
                            product: id(product)?,
                        }
                    }
                    ("uav-sim", "session", [session, "live-view", view]) => Self::LiveView {
                        session: id(session)?,
                        view: id(view)?,
                    },
                    ("uav-sim", "mission", [mission]) => Self::Mission(id(mission)?),
                    ("uav-sim", "mission-plan", [plan]) => Self::MissionPlan(id(plan)?),
                    ("uav-sim", "control-grant", [grant]) => Self::ControlGrant(id(grant)?),
                    ("uav-sim", "usage", ["task", task]) => {
                        let task: TaskId = id(task)?;
                        if task.as_uuid().get_version_num() != 7 {
                            return Err(UavResourceError::InvalidIdentifier);
                        }
                        Self::UsageTask(task)
                    }
                    _ => return Err(UavResourceError::UnknownResource),
                }
            }
        };
        if resource.to_uri()?.as_str() != value {
            return Err(UavResourceError::UnknownResource);
        }
        Ok(resource)
    }

    pub fn is_subscribable(&self) -> bool {
        !matches!(
            self,
            Self::Docs
                | Self::Document(_)
                | Self::Contract
                | Self::LiveApp
                | Self::Sessions
                | Self::ControlGrants { cursor: Some(_) }
                | Self::MissionPlans { cursor: Some(_) }
                | Self::Missions { cursor: Some(_) }
                | Self::Usage { cursor: Some(_) }
                | Self::LiveViews {
                    cursor: Some(_),
                    ..
                }
        )
    }
}

fn address(
    root: &str,
    path: &[&str],
    cursor: Option<&str>,
) -> Result<ResourceUri, UavResourceError> {
    let mut builder = ResourceUriBuilder::new(root)?;
    for part in path {
        builder = builder.segment(UriSegment::new(*part)?);
    }
    if let Some(cursor) = cursor {
        builder = builder.query_pair("cursor", cursor)?;
    }
    Ok(builder.build()?)
}

impl ResourceAddress for UavResource {
    type Error = UavResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let session_root = "uav-sim://session";
        match self {
            Self::Docs => address(uris::DOCS, &[], None),
            Self::Document(doc) => address(uris::DOCS, &[doc.id()], None),
            Self::Contract => address(uris::CONTRACT, &[], None),
            Self::LiveApp => address(uris::LIVE_APP_URI, &[], None),
            Self::Sessions => address(uris::SESSIONS, &[], None),
            Self::Session(session) => address(session_root, &[session.as_str()], None),
            Self::World(session) => address(session_root, &[session.as_str(), "world"], None),
            Self::Tiles(session) => address(session_root, &[session.as_str(), "tiles"], None),
            Self::Vehicles(session) => address(session_root, &[session.as_str(), "vehicles"], None),
            Self::Recordings(session) => {
                address(session_root, &[session.as_str(), "recordings"], None)
            }
            Self::Vehicle { session, vehicle } => address(
                session_root,
                &[session.as_str(), "vehicle", vehicle.as_str()],
                None,
            ),
            Self::LiveCameras(session) => {
                address(session_root, &[session.as_str(), "live-cameras"], None)
            }
            Self::LiveCamera { session, camera } => address(
                session_root,
                &[session.as_str(), "live-camera", camera.as_str()],
                None,
            ),
            Self::StreamProducts(session) => {
                address(session_root, &[session.as_str(), "stream-products"], None)
            }
            Self::StreamProduct { session, product } => address(
                session_root,
                &[session.as_str(), "stream-product", product.as_str()],
                None,
            ),
            Self::LiveViews { session, cursor } => {
                if cursor
                    .as_ref()
                    .is_some_and(|cursor| cursor.session() != session)
                {
                    return Err(UavResourceError::InvalidCursor);
                }
                address(
                    session_root,
                    &[session.as_str(), "live-views"],
                    cursor.as_ref().map(UavLiveViewCursor::as_str),
                )
            }
            Self::LiveView { session, view } => address(
                session_root,
                &[session.as_str(), "live-view", view.as_str()],
                None,
            ),
            Self::ControlGrants { cursor } => {
                if cursor
                    .as_ref()
                    .is_some_and(|cursor| cursor.session().is_some())
                {
                    return Err(UavResourceError::InvalidCursor);
                }
                address(
                    uris::CONTROL_GRANTS,
                    &[],
                    cursor.as_ref().map(UavGrantCursor::as_str),
                )
            }
            Self::ControlGrant(grant) => {
                address("uav-sim://control-grant", &[grant.as_str()], None)
            }
            Self::MissionPlans { cursor } => address(
                uris::MISSION_PLANS,
                &[],
                cursor.as_ref().map(UavPlanCursor::as_str),
            ),
            Self::MissionPlan(plan) => address("uav-sim://mission-plan", &[plan.as_str()], None),
            Self::Missions { cursor } => address(
                uris::MISSIONS,
                &[],
                cursor.as_ref().map(UavMissionCursor::as_str),
            ),
            Self::Mission(mission) => address("uav-sim://mission", &[mission.as_str()], None),
            Self::Usage { cursor } => address(
                uris::USAGE,
                &[],
                cursor.as_ref().map(UavUsageCursor::as_str),
            ),
            Self::UsageTask(task) => {
                if task.as_uuid().get_version_num() != 7 {
                    return Err(UavResourceError::InvalidIdentifier);
                }
                address(uris::USAGE, &["task", &task.to_string()], None)
            }
        }
    }
}
impl Serialize for UavResource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_uri()
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for UavResource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
