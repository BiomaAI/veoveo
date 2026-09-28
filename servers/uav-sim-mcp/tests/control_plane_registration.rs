use std::{collections::BTreeSet, fs, path::PathBuf};

use serde::Deserialize;
use veoveo_mcp_contract::{GatewayControlPlane, ResourceProjectionMode, docs::CONTRACT_REVISION};
use veoveo_types::ResourceScheme;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("UAV MCP crate lives under <repository>/servers")
        .to_owned()
}

// Registration metadata is extensible; decode the declared contract field explicitly.
#[derive(Deserialize)]
struct ContractMetadata {
    contract_revision: u32,
}

#[test]
fn registrations_declare_the_contract_and_preserve_cross_server_identities() {
    for path in ["configs/gateway.local.json", "examples/bioma/gateway.json"] {
        let control_plane: GatewayControlPlane = serde_json::from_slice(
            &fs::read(repository_root().join(path)).expect("read installation registration"),
        )
        .expect("decode typed gateway control plane");
        control_plane
            .validate()
            .expect("valid installation registration");
        let uav = control_plane
            .servers
            .iter()
            .find(|server| server.slug.as_str() == "uav-sim")
            .expect("installation registers the UAV server");
        assert_eq!(
            uav.resource_projection,
            ResourceProjectionMode::ServerOwned,
            "{path}"
        );
        assert_eq!(
            uav.referenced_resource_schemes,
            ["frames", "map", "recording"]
                .map(|name| ResourceScheme::new(name).unwrap())
                .into_iter()
                .collect::<BTreeSet<_>>(),
            "{path}"
        );
        let metadata: ContractMetadata = serde_json::from_value(uav.metadata.clone())
            .expect("registration declares its contract revision");
        assert_eq!(metadata.contract_revision, CONTRACT_REVISION, "{path}");
    }
}
