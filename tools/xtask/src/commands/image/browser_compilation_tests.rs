//! The browser edge must keep its compiler action when runtime selection changes.
use super::{BuilderFamily, Selection, bake_print, prepare};
use crate::context::RepositoryContext;
use std::{collections::BTreeMap, path::Path};

#[test]
fn browser_compilation_is_identical_alone_and_with_backend_images() {
    let repository = RepositoryContext::discover(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let environment = BTreeMap::new();
    let alone = prepare(
        &repository,
        Selection::target("console-bff").unwrap(),
        &environment,
    )
    .unwrap();
    let combined = prepare(
        &repository,
        Selection::group("platform-core").unwrap(),
        &environment,
    )
    .unwrap();
    let browser = |plan: &super::BuildPlanV1| {
        plan.families
            .iter()
            .find(|family| family.family == BuilderFamily::RustTrixieBrowserV1)
            .map(|family| {
                assert_eq!(family.packages, ["veoveo-console-bff"]);
                assert_eq!(family.binaries, ["console-bff"]);
                assert_eq!(family.targets, ["console-bff"]);
                assert!(
                    !family
                        .source_inputs
                        .source_packages
                        .iter()
                        .any(|p| p == "veoveo-mcp-gateway")
                );
                assert!(
                    !family
                        .context_path
                        .join("apps/workspace/src/Conversation.tsx")
                        .exists()
                );
                serde_json::to_value(family).unwrap()
            })
            .expect("browser compilation has a dedicated family")
    };
    assert_eq!(browser(&alone.plan), browser(&combined.plan));
    assert_eq!(alone.plan.families.len(), 1);

    let artifact = "rust-trixie-browser-artifacts";
    let mut actions = Vec::new();
    for prepared in [&alone, &combined] {
        let resolved = bake_print(
            &repository,
            repository.root(),
            &prepared.plan.selection,
            &environment,
            &[prepared.override_file.path()],
        )
        .unwrap();
        assert_eq!(
            resolved.target["console-bff"].contexts["veoveo-rust-artifacts"],
            format!("target:{artifact}")
        );
        let compiler = &resolved.target[artifact];
        assert_eq!(compiler.args["VEOVEO_CARGO_PACKAGES"], "veoveo-console-bff");
        assert_eq!(compiler.args["VEOVEO_CARGO_BINARIES"], "console-bff");
        assert_eq!(
            compiler.args["RUST_IMAGE"],
            "docker.io/library/rust:1.98.1-slim-trixie@sha256:ce84a5edd80c5f91e05c5533b1e53eb1da54028f33734dc06aa6b49fa190462d"
        );
        actions.push((
            compiler.dockerfile.clone(),
            compiler.args.clone(),
            compiler.contexts.clone(),
        ));
    }
    assert_eq!(actions[0], actions[1]);
}
