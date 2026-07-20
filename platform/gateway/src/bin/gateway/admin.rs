#[path = "admin/artifact_access.rs"]
mod artifact_access;
#[path = "admin/artifacts.rs"]
mod artifacts;
#[path = "admin/console/mod.rs"]
mod console;
#[path = "admin/control_plane.rs"]
mod control_plane;
#[path = "admin/jwt_revocations.rs"]
mod jwt_revocations;
#[path = "admin/server_proxy.rs"]
mod server_proxy;
#[path = "admin/tasks.rs"]
mod tasks;

use veoveo_mcp_contract::GatewayProfileId;

pub(super) use artifact_access::{
    cancel_artifact_access_request, create_artifact_access_request, decide_artifact_access_request,
    list_artifact_access_requests,
};
pub(super) use artifacts::{
    create_artifact_share_link, grant_artifact, revoke_artifact_grant, revoke_artifact_share_link,
    set_artifact_release_state,
};
pub(crate) use console::{
    ConsoleStreamRuntime, ServerHealthMonitor, spawn_console_wake_hub, spawn_server_health_prober,
};
pub(super) use console::{authorize_console_cluster, read_console_snapshot, stream_console};
pub(super) use control_plane::{read_control_plane, update_control_plane};
pub(super) use jwt_revocations::{prune_jwt_revocations, revoke_jwt};
pub(crate) use server_proxy::proxy_server_admin;
pub(super) use tasks::cancel_task;

fn admin_profile_id(profile: String) -> Option<GatewayProfileId> {
    GatewayProfileId::new(profile).ok()
}
