#[path = "admin/control_plane.rs"]
mod control_plane;
#[path = "admin/jwt_revocations.rs"]
mod jwt_revocations;

use veoveo_mcp_contract::GatewayProfileId;

pub(super) use control_plane::{read_control_plane, reload_control_plane, update_control_plane};
pub(super) use jwt_revocations::{prune_jwt_revocations, revoke_jwt};

fn admin_profile_id(profile: String) -> Option<GatewayProfileId> {
    GatewayProfileId::new(profile).ok()
}
