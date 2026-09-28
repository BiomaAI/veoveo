//! Declared UAV discovery roots and templates; dynamic addresses belong to `UavResource`.
pub const SCHEME: &str = "uav-sim";
pub const DOCS: &str = "uav-sim://docs";
pub const CONTRACT: &str = "uav-sim://contract";
pub const SESSIONS: &str = "uav-sim://sessions";
pub const MISSIONS: &str = "uav-sim://missions";
pub const USAGE_PAGE_TEMPLATE: &str = "uav-sim://usage{?cursor}";
pub const MISSIONS_PAGE_TEMPLATE: &str = "uav-sim://missions{?cursor}";
pub const CONTROL_GRANTS_PAGE_TEMPLATE: &str = "uav-sim://control-grants{?cursor}";
pub const MISSION_PLANS_PAGE_TEMPLATE: &str = "uav-sim://mission-plans{?cursor}";
pub const USAGE: &str = "uav-sim://usage";
pub const CONTROL_GRANTS: &str = "uav-sim://control-grants";
pub const MISSION_PLANS: &str = "uav-sim://mission-plans";
pub const LIVE_APP_URI: &str = "ui://uav-sim/live.html";
pub const DOC_TEMPLATE: &str = "uav-sim://docs/{doc_id}";
pub const SESSION_TEMPLATE: &str = "uav-sim://session/{session_id}";
pub const WORLD_TEMPLATE: &str = "uav-sim://session/{session_id}/world";
pub const TILES_TEMPLATE: &str = "uav-sim://session/{session_id}/tiles";
pub const VEHICLES_TEMPLATE: &str = "uav-sim://session/{session_id}/vehicles";
pub const VEHICLE_TEMPLATE: &str = "uav-sim://session/{session_id}/vehicle/{vehicle_id}";
pub const RECORDINGS_TEMPLATE: &str = "uav-sim://session/{session_id}/recordings";
pub const MISSION_TEMPLATE: &str = "uav-sim://mission/{mission_id}";
pub const CONTROL_GRANT_TEMPLATE: &str = "uav-sim://control-grant/{grant_id}";
pub const MISSION_PLAN_TEMPLATE: &str = "uav-sim://mission-plan/{plan_id}";
pub const USAGE_TASK_TEMPLATE: &str = "uav-sim://usage/task/{task_id}";
pub const LIVE_CAMERAS_TEMPLATE: &str = "uav-sim://session/{session_id}/live-cameras";
pub const LIVE_CAMERA_TEMPLATE: &str = "uav-sim://session/{session_id}/live-camera/{camera_id}";
pub const STREAM_PRODUCTS_TEMPLATE: &str = "uav-sim://session/{session_id}/stream-products";
pub const STREAM_PRODUCT_TEMPLATE: &str =
    "uav-sim://session/{session_id}/stream-product/{product_id}";
pub const LIVE_VIEWS_TEMPLATE: &str = "uav-sim://session/{session_id}/live-views";
pub const LIVE_VIEWS_PAGE_TEMPLATE: &str = "uav-sim://session/{session_id}/live-views{?cursor}";
pub const LIVE_VIEW_TEMPLATE: &str = "uav-sim://session/{session_id}/live-view/{live_view_id}";

use crate::contract::{
    ControlGrantId, LiveCameraId, LiveSessionId, LiveStreamProductId, LiveViewId, MissionId,
    MissionPlanId, SessionId, UavDocument, UavResource, UavResourceError, VehicleId,
};
use veoveo_types::{ResourceAddress, ResourceUri, TaskId};

fn admitted(resource: UavResource) -> ResourceUri {
    resource
        .to_uri()
        .expect("admitted UAV IDs form a resource address")
}

pub fn doc(document: UavDocument) -> ResourceUri {
    admitted(UavResource::Document(document))
}
pub fn session(id: &SessionId) -> ResourceUri {
    admitted(UavResource::Session(id.clone()))
}
pub fn world(id: &SessionId) -> ResourceUri {
    admitted(UavResource::World(id.clone()))
}
pub fn tiles(id: &SessionId) -> ResourceUri {
    admitted(UavResource::Tiles(id.clone()))
}
pub fn vehicles(id: &SessionId) -> ResourceUri {
    admitted(UavResource::Vehicles(id.clone()))
}
pub fn recordings(id: &SessionId) -> ResourceUri {
    admitted(UavResource::Recordings(id.clone()))
}
pub fn mission(id: &MissionId) -> ResourceUri {
    admitted(UavResource::Mission(id.clone()))
}
pub fn mission_plan(id: &MissionPlanId) -> ResourceUri {
    admitted(UavResource::MissionPlan(id.clone()))
}
pub fn control_grant(id: &ControlGrantId) -> ResourceUri {
    admitted(UavResource::ControlGrant(id.clone()))
}
pub fn live_cameras(id: &LiveSessionId) -> ResourceUri {
    admitted(UavResource::LiveCameras(id.clone()))
}
pub fn stream_products(id: &LiveSessionId) -> ResourceUri {
    admitted(UavResource::StreamProducts(id.clone()))
}
pub fn vehicle(session: &SessionId, vehicle: &VehicleId) -> ResourceUri {
    admitted(UavResource::Vehicle {
        session: session.clone(),
        vehicle: vehicle.clone(),
    })
}
pub fn live_camera(session: &LiveSessionId, camera: &LiveCameraId) -> ResourceUri {
    admitted(UavResource::LiveCamera {
        session: session.clone(),
        camera: camera.clone(),
    })
}
pub fn stream_product(session: &LiveSessionId, product: &LiveStreamProductId) -> ResourceUri {
    admitted(UavResource::StreamProduct {
        session: session.clone(),
        product: product.clone(),
    })
}
pub fn live_view(session: &LiveSessionId, view: &LiveViewId) -> ResourceUri {
    admitted(UavResource::LiveView {
        session: session.clone(),
        view: view.clone(),
    })
}
pub fn live_views(session: &LiveSessionId) -> ResourceUri {
    admitted(UavResource::LiveViews {
        session: session.clone(),
        cursor: None,
    })
}
pub fn usage_task(task: TaskId) -> Result<ResourceUri, UavResourceError> {
    UavResource::UsageTask(task).to_uri()
}
