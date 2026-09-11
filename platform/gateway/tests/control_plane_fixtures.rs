use std::{fs, path::Path};

use serde_json::Value;
use veoveo_mcp_contract::{GatewayControlPlane, LocalToolName};

const CORE_CONTROL_PLANES: [&str; 3] = [
    "../../configs/gateway.local.json",
    "../../configs/gateway.smoke.json",
    "../../examples/bioma/gateway.json",
];

#[test]
fn bioma_profiles_expose_computer_execution_and_retained_maintenance() {
    let control: GatewayControlPlane = serde_json::from_slice(
        &fs::read("../../examples/bioma/gateway.json").expect("read Bioma control plane"),
    )
    .expect("decode Bioma control plane");
    for id in ["operator", "agent", "admin"] {
        let profile = control
            .profiles
            .iter()
            .find(|p| p.id.as_str() == id)
            .unwrap();
        let computers = profile
            .servers
            .iter()
            .find(|s| s.server.as_str() == "computers")
            .unwrap();
        for name in [
            "execute",
            "grant_automation",
            "revoke_automation",
            "update_template",
            "resume_update",
        ] {
            let tool = LocalToolName::new(name).unwrap();
            assert!(
                veoveo_policy::exposure_contains(&computers.tools, &tool),
                "Bioma {id} hides delivered Computer workflow {name}"
            );
        }
    }
}

#[test]
fn core_control_planes_satisfy_the_gateway_contract() {
    for path in CORE_CONTROL_PLANES {
        let bytes = fs::read(path).expect("read core control plane");
        serde_json::from_slice::<GatewayControlPlane>(&bytes)
            .expect("decode core control plane")
            .validate()
            .unwrap_or_else(|error| panic!("{path}: {error}"));
    }
}

#[test]
fn core_control_planes_use_exact_list_change_capabilities() {
    for path in CORE_CONTROL_PLANES {
        let text = fs::read_to_string(Path::new(path)).expect("read core control plane");
        assert!(
            !text.contains("\"notifications\""),
            "{path} still uses the generic notification capability"
        );
        let value: Value = serde_json::from_str(&text).expect("decode core control plane");
        for server in value["servers"].as_array().expect("servers array") {
            let capabilities = server["capabilities"]
                .as_object()
                .expect("capability object");
            if capabilities
                .get("resources_list_changed")
                .and_then(Value::as_bool)
                == Some(true)
            {
                assert_eq!(
                    capabilities.get("resources").and_then(Value::as_bool),
                    Some(true),
                    "{} claims resource list changes without resources",
                    server["slug"]
                );
            }
        }
    }
}
