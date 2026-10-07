//! Inspect Cargo's resolved normal/build graph, including multi-root feature union.
use std::{collections::BTreeSet, path::Path, process::Command};

fn admitted(graph: &str) -> Result<(), String> {
    for line in graph.lines().filter(|line| !line.is_empty()) {
        let name = line.split_whitespace().next().unwrap();
        if name.starts_with("veoveo-")
            && !matches!(
                name,
                "veoveo-testing-support"
                    | "veoveo-browser-smoke"
                    | "veoveo-deploy-contract"
                    | "veoveo-gateway-contract"
                    | "veoveo-mcp-contract"
                    | "veoveo-mcp-conformance"
                    | "veoveo-mcp-apps-extension"
                    | "veoveo-mcp-knowledge-extension"
                    | "veoveo-macros"
                    | "veoveo-types"
                    | "veoveo-modules"
            )
        {
            return Err(format!("unrelated Veoveo implementation: {name}"));
        }
        if name.starts_with("surreal")
            || name.starts_with("re_")
            || matches!(name, "duckdb" | "libduckdb-sys")
        {
            return Err(format!(
                "database/recording/analytical implementation: {name}"
            ));
        }
        if name == "veoveo-modules" {
            let features = line.split("features=").nth(1).unwrap_or("");
            if features.split(',').any(|feature| feature == "runner") {
                return Err("Modules runner feature entered generic graph".into());
            }
        }
        if name == "veoveo-mcp-contract" {
            let features = line.split("features=").nth(1).unwrap_or("");
            if features
                .split(',')
                .any(|f| matches!(f, "runtime" | "testing" | "analytics"))
            {
                return Err("MCP runtime feature entered generic graph".into());
            }
        }
    }
    Ok(())
}
fn graph(roots: &[&str], features: Option<&str>) -> String {
    let mut command = Command::new(env!("CARGO"));
    command
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .args([
            "tree",
            "--locked",
            "--offline",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p} features={f}",
        ])
        .args(roots.iter().flat_map(|root| ["--package", *root]));
    if let Some(features) = features {
        command.args(["--features", features]);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
fn focused_clients_exclude_service_implementations() {
    for roots in [
        vec!["veoveo-testing-support"],
        vec!["veoveo-mcp-conformance"],
        vec!["veoveo-testing-support", "veoveo-mcp-conformance"],
        vec!["veoveo-browser-smoke"],
    ] {
        let graph = graph(&roots, None);
        admitted(&graph).unwrap();
        let names: BTreeSet<_> = graph
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        assert!(names.contains(roots[0]), "Cargo omitted selected client");
    }
}
#[test]
fn indirect_runtime_edges_and_two_root_feature_unions_are_refused() {
    assert!(admitted("veoveo-testing-support v0.1.0 features=\nveoveo-mcp-contract v0.1.0 features=declaration,runtime\n").is_err());
    assert!(
        admitted(
            "veoveo-mcp-conformance v0.1.0 features=\nveoveo-media-mcp v0.1.0 features=contract\n"
        )
        .is_err()
    );
    // The same Cargo invocation unifies the runtime root with the generic client's declaration edge.
    let graph = graph(
        &["veoveo-mcp-conformance", "veoveo-mcp-contract"],
        Some("veoveo-mcp-contract/runtime"),
    );
    assert!(
        admitted(&graph).is_err(),
        "runtime feature union escaped normal/build admission"
    );
}

#[test]
fn module_runner_feature_union_is_refused() {
    assert!(admitted("veoveo-modules v0.1.0 features=serialization").is_ok());
    assert_eq!(
        admitted("veoveo-modules v0.1.0 features=runner,serialization").unwrap_err(),
        "Modules runner feature entered generic graph"
    );
    // Resolve the union through Cargo without compiling the runner's implementation.
    let graph = graph(
        &["veoveo-testing-support", "veoveo-modules"],
        Some("veoveo-modules/runner"),
    );
    assert!(
        graph.lines().any(|line| line.starts_with("veoveo-modules ")
            && line
                .split("features=")
                .nth(1)
                .unwrap_or("")
                .split(',')
                .any(|feature| feature == "runner")),
        "Cargo did not resolve the selected Modules runner union"
    );
    assert_eq!(
        admitted(&graph).unwrap_err(),
        "Modules runner feature entered generic graph"
    );
}
