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
    let output = Command::new("cargo")
        .args([
            "tree",
            "--locked",
            "--package",
            "veoveo-mcp-gateway",
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
    let packages = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect::<BTreeSet<_>>();
    assert!(packages.contains("veoveo-mcp-gateway"));
    for package in ["duckdb", "libduckdb-sys"] {
        assert!(
            !packages.contains(package),
            "gateway acquired an analytics dependency: {package}"
        );
    }
}
