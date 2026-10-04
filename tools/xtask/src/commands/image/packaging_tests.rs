//! Runtime packaging must reflect the production dependency graph, not stale caches.
use super::{Selection, prepare};
use crate::context::RepositoryContext;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

#[test]
fn gateway_runtime_does_not_require_analytics_artifacts() {
    let repository = RepositoryContext::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let plan = prepare(
        &repository,
        Selection::target("mcp-gateway").unwrap(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(plan.plan.families.len(), 1);
    assert!(plan.plan.families[0].auxiliary.is_empty());
    let packages = runtime_packages(&repository, "veoveo-gateway-composition");
    assert!(packages.contains("veoveo-gateway-composition"));
    for package in ["duckdb", "libduckdb-sys"] {
        assert!(
            !packages.contains(package),
            "gateway acquired an analytics dependency: {package}"
        );
    }
}

fn runtime_packages(repository: &RepositoryContext, package: &str) -> BTreeSet<String> {
    let output = Command::new("cargo")
        .args([
            "tree",
            "--locked",
            "--package",
            package,
            "--target",
            "x86_64-unknown-linux-gnu",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .current_dir(repository.root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    tree.lines()
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
}

#[test]
fn gateway_library_excludes_optional_owner_adapters() {
    let repository = RepositoryContext::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let packages = runtime_packages(&repository, "veoveo-mcp-gateway");
    for package in [
        "veoveo-gateway-composition",
        "veoveo-agent-runtime",
        "veoveo-recording-mcp",
        "veoveo-recording-protocol",
        "veoveo-speech-contract",
        "veoveo-computers-transport",
        "veoveo-computers",
        "veoveo-map-mcp",
        "veoveo-time-mcp",
        "veoveo-uav-sim-mcp",
        "veoveo-frames-mcp",
        "veoveo-media-mcp",
        "veoveo-optimization-mcp",
        "veoveo-workspace",
    ] {
        assert!(
            !packages.contains(package),
            "reusable gateway acquired optional owner {package}"
        );
    }
}
