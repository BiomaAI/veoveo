//! Exercise Cargo's resolved graph, including the helper selected by xtask.
use std::{collections::BTreeSet, path::Path, process::Command};

#[test]
fn focused_clients_exclude_service_implementations() {
    for roots in [
        vec!["veoveo-flight-smoke"],
        vec!["veoveo-flight-smoke", "veoveo-mcp-conformance"],
        vec!["veoveo-browser-smoke"],
    ] {
        let output = Command::new(env!("CARGO"))
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
                "{p}",
            ])
            .args(roots.iter().flat_map(|root| ["--package", *root]))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let graph = String::from_utf8(output.stdout).unwrap();
        let names: BTreeSet<_> = graph
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        for name in &names {
            assert!(
                !name.starts_with("veoveo-")
                    || matches!(
                        *name,
                        "veoveo-flight-smoke"
                            | "veoveo-browser-smoke"
                            | "veoveo-mcp-contract"
                            | "veoveo-mcp-conformance"
                            | "veoveo-mcp-apps-extension"
                    ),
                "{roots:?} links an unrelated Veoveo implementation: {name}"
            );
            assert!(
                !name.starts_with("surreal")
                    && !name.starts_with("re_")
                    && !matches!(*name, "duckdb" | "libduckdb-sys"),
                "{roots:?} links database or recording implementation {name}"
            );
        }
        assert!(names.contains(roots[0]), "Cargo omitted selected client");
    }
}
