use crate::contract::{MissionId, SessionId, StreamId, VehicleId};
use veoveo_mcp_contract::ServerResourceUris;

pub const SCHEME: &str = "uav-sim";
pub const SESSIONS: &str = "uav-sim://sessions";
pub const USAGE: &str = "uav-sim://usage";
pub const SESSION_TEMPLATE: &str = "uav-sim://session/{session_id}";
pub const WORLD_TEMPLATE: &str = "uav-sim://session/{session_id}/world";
pub const TILES_TEMPLATE: &str = "uav-sim://session/{session_id}/tiles";
pub const VEHICLES_TEMPLATE: &str = "uav-sim://session/{session_id}/vehicles";
pub const VEHICLE_TEMPLATE: &str = "uav-sim://session/{session_id}/vehicle/{vehicle_id}";
pub const RECORDINGS_TEMPLATE: &str = "uav-sim://session/{session_id}/recordings";
pub const STREAMS_TEMPLATE: &str = "uav-sim://session/{session_id}/streams";
pub const STREAM_TEMPLATE: &str = "uav-sim://session/{session_id}/stream/{stream_id}";
pub const MISSION_TEMPLATE: &str = "uav-sim://mission/{mission_id}";
pub const USAGE_TASK_TEMPLATE: &str = "uav-sim://usage/task/{task_id}";
pub const LIVE_APP_URI: &str = "ui://uav-sim/live.html";

pub fn session(session_id: &SessionId) -> String {
    format!("uav-sim://session/{session_id}")
}

pub fn world(session_id: &SessionId) -> String {
    format!("{}/world", session(session_id))
}

pub fn tiles(session_id: &SessionId) -> String {
    format!("{}/tiles", session(session_id))
}

pub fn vehicles(session_id: &SessionId) -> String {
    format!("{}/vehicles", session(session_id))
}

pub fn vehicle(session_id: &SessionId, vehicle_id: &VehicleId) -> String {
    format!("{}/vehicle/{vehicle_id}", session(session_id))
}

pub fn recordings(session_id: &SessionId) -> String {
    format!("{}/recordings", session(session_id))
}

pub fn streams(session_id: &SessionId) -> String {
    format!("{}/streams", session(session_id))
}

pub fn stream(session_id: &SessionId, stream_id: &StreamId) -> String {
    format!("{}/stream/{stream_id}", session(session_id))
}

pub fn mission(mission_id: &MissionId) -> String {
    format!("uav-sim://mission/{mission_id}")
}

pub fn usage_task(task_id: &str) -> String {
    format!("uav-sim://usage/task/{task_id}")
}

pub fn parse_session(uri: &str) -> Option<&str> {
    parse_one(uri, "uav-sim://session/")
}

pub fn parse_world(uri: &str) -> Option<&str> {
    parse_session_suffix(uri, "/world")
}

pub fn parse_tiles(uri: &str) -> Option<&str> {
    parse_session_suffix(uri, "/tiles")
}

pub fn parse_vehicles(uri: &str) -> Option<&str> {
    parse_session_suffix(uri, "/vehicles")
}

pub fn parse_recordings(uri: &str) -> Option<&str> {
    parse_session_suffix(uri, "/recordings")
}

pub fn parse_streams(uri: &str) -> Option<&str> {
    parse_session_suffix(uri, "/streams")
}

pub fn parse_stream(uri: &str) -> Option<(&str, &str)> {
    let rest = uri.strip_prefix("uav-sim://session/")?;
    let (session_id, stream_id) = rest.split_once("/stream/")?;
    if valid_segment(session_id) && valid_segment(stream_id) {
        Some((session_id, stream_id))
    } else {
        None
    }
}

pub fn parse_vehicle(uri: &str) -> Option<(&str, &str)> {
    let rest = uri.strip_prefix("uav-sim://session/")?;
    let (session_id, vehicle_id) = rest.split_once("/vehicle/")?;
    if valid_segment(session_id) && valid_segment(vehicle_id) {
        Some((session_id, vehicle_id))
    } else {
        None
    }
}

pub fn parse_mission(uri: &str) -> Option<&str> {
    parse_one(uri, "uav-sim://mission/")
}

pub fn parse_usage_task(uri: &str) -> Option<&str> {
    ServerResourceUris::new(SCHEME).parse_usage_task_uri(uri)
}

fn parse_one<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    let value = uri.strip_prefix(prefix)?;
    valid_segment(value).then_some(value)
}

fn parse_session_suffix<'a>(uri: &'a str, suffix: &str) -> Option<&'a str> {
    let value = uri
        .strip_prefix("uav-sim://session/")?
        .strip_suffix(suffix)?;
    valid_segment(value).then_some(value)
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty() && !value.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_identities_are_stable() {
        let session_id = SessionId::new("alpha").unwrap();
        let vehicle_id = VehicleId::new("uav-1").unwrap();
        let stream_id = StreamId::new("stream-1").unwrap();
        assert_eq!(session(&session_id), "uav-sim://session/alpha");
        assert_eq!(
            vehicle(&session_id, &vehicle_id),
            "uav-sim://session/alpha/vehicle/uav-1"
        );
        assert_eq!(parse_session(&session(&session_id)), Some("alpha"));
        assert_eq!(parse_world(&world(&session_id)), Some("alpha"));
        assert_eq!(
            parse_vehicle(&vehicle(&session_id, &vehicle_id)),
            Some(("alpha", "uav-1"))
        );
        assert_eq!(parse_usage_task(&usage_task("task-1")), Some("task-1"));
        assert_eq!(
            parse_stream(&stream(&session_id, &stream_id)),
            Some(("alpha", "stream-1"))
        );
        assert_eq!(parse_streams(&streams(&session_id)), Some("alpha"));
        assert_eq!(parse_session("uav-sim://session/a/world"), None);
    }
}
