//! Repository-structure enforcement for the MCP server contract
//! (`mcp/contract/DESIGN.md` C22-C24).
//!
//! Servers are discovered by globbing `servers/*-mcp/`; nothing here
//! enumerates servers by hand, so adding a server extends coverage without
//! editing this test.

use std::{fs, path::PathBuf};

use veoveo_mcp_contract::docs::{
    CHECKLIST_IDS, ComplianceStatus, REQUIRED_AGENT_SECTIONS, parse_compliance,
};

fn servers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../servers")
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
