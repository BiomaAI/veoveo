use std::{fs, path::PathBuf};

use serde_json::Value;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("UAV MCP crate lives under <repository>/servers")
        .to_owned()
}

#[test]
fn local_registration_preserves_cross_server_resource_identities() {
    let path = repository_root().join("configs/gateway.local.json");
    let control_plane: Value =
        serde_json::from_slice(&fs::read(&path).expect("read local control plane"))
            .expect("parse local control plane");
    let uav = control_plane["servers"]
        .as_array()
        .expect("servers is an array")
        .iter()
        .find(|server| server["slug"] == "uav-sim")
        .expect("local installation registers the UAV server");

    assert_eq!(uav["resource_projection"], "server_owned");
    assert_eq!(
        uav["referenced_resource_schemes"],
        serde_json::json!(["artifact", "frames", "recording"]),
        "the registration must preserve artifact:// scene, frames:// world, and recording:// capture identities"
    );
}
