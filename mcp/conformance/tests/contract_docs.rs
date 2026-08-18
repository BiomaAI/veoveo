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

/// Well-Known Surface items every server must implement, not merely declare
/// (`mcp/contract/DESIGN.md` C18-C21): docs resources, the contract
/// declaration resource, the admin `docs/llms.txt` projection, and build-time
/// document embedding.
const WELL_KNOWN_SURFACE_IDS: [&str; 4] = ["C18", "C19", "C20", "C21"];

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
    assert!(contract.contains(".with_legacy_session_mode(false)"));
    assert!(contract.contains(".with_json_response(true)"));
    assert!(contract.contains(".with_stateless_protocol_metadata_required(true)"));
    assert!(contract.contains("NeverSessionManager"));
    assert!(!root.join("mcp/contract/src/session.rs").exists());

    let python =
        fs::read_to_string(root.join("templates/python-mcp/src/datasheet_mcp/server/main.py"))
            .unwrap();
    assert!(python.contains("json_response=True"));
    assert!(python.contains("stateless=True"));
    assert!(!python.contains("session_idle_timeout"));

    let gateway =
        fs::read_to_string(root.join("deploy/helm/veoveo/templates/gateway.yaml")).unwrap();
    let domains =
        fs::read_to_string(root.join("deploy/helm/veoveo/templates/domain-services.yaml")).unwrap();
    assert!(gateway.contains("replicas: {{ .Values.gateway.replicas }}"));
    assert!(domains.contains("replicas: {{ $serviceReplicas }}"));
    assert!(domains.contains("$.Values.domainServiceReplicas"));

    let values = fs::read_to_string(root.join("deploy/helm/veoveo/values.yaml")).unwrap();
    assert!(values.contains("gateway:\n"));
    assert!(values.contains("domainServiceReplicas:\n"));

    let store_model = fs::read_to_string(root.join("platform/store/src/models.rs")).unwrap();
    let transport_start = store_model.find("pub enum ServerTransport").unwrap();
    let transport_end = store_model[transport_start..].find("}\n}").unwrap() + transport_start;
    let transport = &store_model[transport_start..transport_end];
    assert!(transport.contains("StreamableHttp"));
    assert!(!transport.contains("Sse"));
    assert!(!transport.contains("Stdio"));

    let chart = fs::read_to_string(root.join("servers/chart-mcp/server.mjs")).unwrap();
    assert!(chart.contains("legacy: \"reject\""));
    assert!(chart.contains("responseMode: \"json\""));
    assert!(!chart.contains("sessionIdGenerator"));

    for relative in [
        "platform/gateway/src/bin/gateway/server.rs",
        "mcp/bridges/legacy/src/main.rs",
        "mcp/bridges/stdio/src/bin/bridge.rs",
        "showcase/sumo/sumo-mcp/src/server/service.rs",
    ] {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(source.contains("stateless_session_manager()"));
        assert!(!source.contains("canonical_session_manager()"));
        assert!(!source.contains("LocalSessionManager"));
    }

    for server in discovered_server_dirs() {
        let rust = rust_sources(&server);
        if rust.contains("StreamableHttpService") {
            assert!(
                rust.contains("stateless_session_manager()"),
                "{} bypasses canonical stateless transport ownership",
                server.display()
            );
            assert!(!rust.contains("canonical_session_manager()"));
            assert!(
                !rust.contains("LocalSessionManager"),
                "{} restores local protocol sessions",
                server.display()
            );
        }
    }
}

fn rust_sources(root: &std::path::Path) -> String {
    let mut combined = String::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            combined.push_str(&rust_sources(&path));
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            combined.push_str(&fs::read_to_string(path).unwrap());
        }
    }
    combined
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
fn every_rust_streamable_server_enforces_the_serialized_response_budget() {
    for server in discovered_server_dirs() {
        let rust = rust_sources(&server);
        if rust.contains("StreamableHttpService") {
            assert!(
                rust.contains("enforce_serialized_mcp_response"),
                "{} omits the shared serialized MCP response budget",
                server.display()
            );
        }
    }

    let root = repository_root();
    for relative in [
        "platform/gateway/src/bin/gateway/server.rs",
        "mcp/bridges/legacy/src/main.rs",
        "mcp/bridges/stdio/src/bin/bridge.rs",
    ] {
        let source = fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            source.contains("enforce_serialized_mcp_response"),
            "{relative} omits the shared serialized MCP response budget"
        );
    }
}

#[test]
fn optimization_completion_discovery_is_bounded_at_the_store() {
    let root = repository_root();
    let service =
        fs::read_to_string(root.join("servers/optimization-mcp/src/bin/server/service.rs"))
            .unwrap();
    let index =
        fs::read_to_string(root.join("servers/optimization-mcp/src/bin/server/index.rs")).unwrap();

    assert!(
        !service.contains("async fn visible_tasks("),
        "Optimization completions must not load the full task collection"
    );
    assert!(
        index.contains("COMPLETION_QUERY") && index.contains("LIMIT $limit"),
        "Optimization completion lookup must use a bounded store query"
    );
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
                    if WELL_KNOWN_SURFACE_IDS.contains(&id) && item.status != ComplianceStatus::Met
                    {
                        failures.push(format!(
                            "{name}: {id} must be met; the well-known surface \
                             (C18-C21) is mandatory for every server"
                        ));
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
