//! Repository-structure enforcement for the MCP server contract
//! (`mcp/contract/DESIGN.md` C22-C29).
//!
//! Servers are discovered by globbing `servers/*-mcp/`; nothing here
//! enumerates servers by hand, so adding a server extends coverage without
//! editing this test.

use std::{fs, path::PathBuf};

use veoveo_mcp_contract::docs::{
    CHECKLIST_IDS, ComplianceStatus, REQUIRED_AGENT_SECTIONS, parse_compliance,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn servers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../servers")
}

#[test]
fn canonical_transport_and_deployment_surfaces_are_hard_cut() {
    let root = repository_root();
    let contract = fs::read_to_string(root.join("mcp/contract/src/transport.rs")).unwrap();
    assert!(contract.contains(".with_stateful_mode(true)"));
    assert!(contract.contains(".with_json_response(false)"));

    let python =
        fs::read_to_string(root.join("templates/python-mcp/src/datasheet_mcp/server/main.py"))
            .unwrap();
    assert!(python.contains("json_response=False, stateless=False"));

    let gateway =
        fs::read_to_string(root.join("deploy/helm/veoveo/templates/gateway.yaml")).unwrap();
    let domains =
        fs::read_to_string(root.join("deploy/helm/veoveo/templates/domain-services.yaml")).unwrap();
    for manifest in [&gateway, &domains] {
        assert!(manifest.contains("replicas: 1"));
        assert!(manifest.contains("type: Recreate"));
        assert!(!manifest.contains(".replicas"));
        assert!(!manifest.contains("RollingUpdate"));
    }

    let values = fs::read_to_string(root.join("deploy/helm/veoveo/values.yaml")).unwrap();
    assert!(!values.contains("defaultReplicas"));
    assert!(!values.contains("deploymentStrategy"));

    let store_model = fs::read_to_string(root.join("platform/store/src/models.rs")).unwrap();
    let transport_start = store_model.find("pub enum ServerTransport").unwrap();
    let transport_end = store_model[transport_start..].find("}\n}").unwrap() + transport_start;
    let transport = &store_model[transport_start..transport_end];
    assert!(transport.contains("StreamableHttp"));
    assert!(!transport.contains("Sse"));
    assert!(!transport.contains("Stdio"));

    let chart = fs::read_to_string(root.join("servers/chart-mcp/server.mjs")).unwrap();
    assert!(chart.contains("sessionIdGenerator: () => randomUUID()"));
    assert!(chart.contains("enableJsonResponse: false"));
}

#[test]
fn gateway_configs_use_exact_list_change_capabilities() {
    let root = repository_root();
    for relative in [
        "configs/gateway.local.json",
        "configs/gateway.smoke.json",
        "examples/bioma/gateway.json",
        "showcase/sumo/deploy/gateway.json",
    ] {
        let text = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            !text.contains("\"notifications\""),
            "{relative} still uses the generic notification capability"
        );
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        for server in value["servers"].as_array().unwrap() {
            let capabilities = server["capabilities"].as_object().unwrap();
            if capabilities
                .get("resources_list_changed")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                assert_eq!(
                    capabilities
                        .get("resources")
                        .and_then(serde_json::Value::as_bool),
                    Some(true),
                    "{} claims resource list changes without resources",
                    server["slug"]
                );
            }
        }
    }
}

fn discovered_server_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(servers_dir())
        .expect("servers/ directory is readable")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            (path.is_dir() && name.ends_with("-mcp")).then_some(path)
        })
        .collect();
    dirs.sort();
    dirs
}

#[test]
fn every_server_crate_carries_its_contract_documents() {
    let dirs = discovered_server_dirs();
    assert!(
        !dirs.is_empty(),
        "server discovery found nothing under servers/; the glob is broken"
    );

    let mut failures = Vec::new();
    for dir in &dirs {
        let name = dir.file_name().unwrap().to_string_lossy().to_string();

        if !dir.join("DESIGN.md").is_file() {
            failures.push(format!("{name}: missing DESIGN.md (C22)"));
        }

        let agents_path = dir.join("AGENTS.md");
        if !agents_path.is_file() {
            failures.push(format!("{name}: missing AGENTS.md (C23)"));
            continue;
        }
        let manual = fs::read_to_string(&agents_path).expect("AGENTS.md is readable");

        for section in REQUIRED_AGENT_SECTIONS {
            if !manual.contains(section) {
                failures.push(format!(
                    "{name}: AGENTS.md missing required section `{section}` (C23)"
                ));
            }
        }

        let items = parse_compliance(&manual);
        for id in CHECKLIST_IDS {
            match items.iter().find(|item| item.id == id) {
                None => failures.push(format!("{name}: Contract Compliance does not declare {id}")),
                Some(item) => {
                    if item.status == ComplianceStatus::Pending && item.note.is_none() {
                        failures.push(format!("{name}: {id} is pending without a reason"));
                    }
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "contract document violations:\n{}",
        failures.join("\n")
    );
}
