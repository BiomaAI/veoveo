//! Runtime packaging must reflect the production dependency graph, not stale caches.
use super::{BakeDefinition, BakeTarget, Selection, prepare, target_dependency_closure};
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

fn provider_plan(input: Option<super::ProviderInput>, target: &str) -> super::BuildPlanV1 {
    super::BuildPlanV1 {
        schema_version: super::PLAN_SCHEMA,
        selection: Selection::target(target).unwrap(),
        source: super::SourceRevision {
            revision: "invoking-revision-is-not-manifest-identity".into(),
            dirty: false,
        },
        source_date_epoch: 0,
        build_date_epoch: 0,
        planning: super::PlanningTimings::default(),
        source_revision_targets: vec![target.into()],
        targets: Vec::new(),
        families: Vec::new(),
        provider_input: input,
        normalized_parents: Vec::new(),
    }
}

#[test]
fn provider_manifest_uses_selected_context_and_refuses_resolved_tampering() {
    let invoking = tempfile::tempdir().unwrap();
    let selected = tempfile::tempdir().unwrap();
    let relative = "platform/runtimes/computers/provider/manifest.json";
    for (directory, bytes) in [
        (invoking.path(), b"invoking".as_slice()),
        (selected.path(), b"selected".as_slice()),
    ] {
        std::fs::create_dir_all(directory.join(relative).parent().unwrap()).unwrap();
        std::fs::write(directory.join(relative), bytes).unwrap();
    }
    let input = super::ProviderInput::admit(selected.path()).unwrap();
    use sha2::Digest;
    assert_eq!(
        input.manifest_sha256,
        hex::encode(sha2::Sha256::digest(b"selected"))
    );
    let plan = provider_plan(Some(input), super::PROVIDER_TARGET);
    let overrides = super::make_override(&plan).unwrap();
    let mut definition: BakeDefinition =
        serde_json::from_value(serde_json::to_value(&overrides).unwrap()).unwrap();
    super::verify_override(&plan, &definition).unwrap();
    let provider = definition.target.get_mut(super::PROVIDER_TARGET).unwrap();
    assert_eq!(std::path::Path::new(&provider.context), selected.path());
    provider
        .args
        .insert(super::PROVIDER_MANIFEST_ARG.into(), "0".repeat(64));
    assert!(
        super::verify_override(&plan, &definition)
            .unwrap_err()
            .to_string()
            .contains("manifest identity")
    );
    definition
        .target
        .get_mut(super::PROVIDER_TARGET)
        .unwrap()
        .args = overrides.target[super::PROVIDER_TARGET].args.clone();
    definition
        .target
        .get_mut(super::PROVIDER_TARGET)
        .unwrap()
        .context = invoking.path().to_str().unwrap().into();
    assert!(
        super::verify_override(&plan, &definition)
            .unwrap_err()
            .to_string()
            .contains("source context")
    );
}

#[test]
fn provider_manifest_admission_refuses_missing_and_nonfile_inputs() {
    let selected = tempfile::tempdir().unwrap();
    assert!(
        super::ProviderInput::admit(selected.path())
            .unwrap_err()
            .to_string()
            .contains("unavailable")
    );
    std::fs::create_dir_all(
        selected
            .path()
            .join("platform/runtimes/computers/provider/manifest.json"),
    )
    .unwrap();
    assert!(
        super::ProviderInput::admit(selected.path())
            .unwrap_err()
            .to_string()
            .contains("regular file")
    );
}

#[test]
fn provider_manifest_admits_host_dependency_without_changing_other_targets() {
    let selected = tempfile::tempdir().unwrap();
    assert!(
        super::ProviderInput::for_targets(selected.path(), &["unrelated".into()])
            .unwrap()
            .is_none()
    );
    let definition = BakeDefinition {
        group: BTreeMap::new(),
        target: BTreeMap::from([
            (
                "computer-host".into(),
                BakeTarget {
                    contexts: BTreeMap::from([(
                        "provider".into(),
                        "target:computer-provider".into(),
                    )]),
                    ..Default::default()
                },
            ),
            (super::PROVIDER_TARGET.into(), BakeTarget::default()),
        ]),
    };
    let closure = target_dependency_closure(&definition, &["computer-host".into()]).unwrap();
    assert!(super::ProviderInput::for_targets(selected.path(), &closure).is_err());
    let manifest = selected
        .path()
        .join("platform/runtimes/computers/provider/manifest.json");
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(&manifest, b"admitted dependency").unwrap();
    let captured = super::ProviderInput::for_targets(selected.path(), &closure)
        .unwrap()
        .unwrap();
    let digest = captured.manifest_sha256.clone();
    std::fs::write(&manifest, b"changed after planning").unwrap();
    let mut plan = provider_plan(Some(captured), "computer-host");
    plan.source_revision_targets = closure;
    let overrides = super::make_override(&plan).unwrap();
    assert_eq!(
        overrides.target[super::PROVIDER_TARGET].args[super::PROVIDER_MANIFEST_ARG],
        digest
    );
    assert!(
        !overrides.target["computer-host"]
            .args
            .contains_key(super::PROVIDER_MANIFEST_ARG)
    );
}

#[test]
fn provider_manifest_arguments_leave_unrelated_plans_unchanged() {
    let plan = provider_plan(None, "unrelated-image");
    let overrides = super::make_override(&plan).unwrap();
    assert!(overrides.target["unrelated-image"].context.is_none());
    assert!(
        !overrides.target["unrelated-image"]
            .args
            .contains_key(super::PROVIDER_MANIFEST_ARG)
    );
    assert!(
        serde_json::to_value(&plan)
            .unwrap()
            .get("providerInput")
            .is_none()
    );
    let definition = BakeDefinition {
        group: BTreeMap::new(),
        target: overrides
            .target
            .into_iter()
            .map(|(name, target)| {
                (
                    name,
                    BakeTarget {
                        context: target.context.unwrap_or_default(),
                        contexts: target.contexts,
                        args: target.args,
                        ..Default::default()
                    },
                )
            })
            .collect(),
    };
    super::verify_override(&plan, &definition).unwrap();
    let dockerfile = include_str!("../../../../../platform/runtimes/computers/provider/Dockerfile");
    assert!(dockerfile.contains("test -n \"${PROVIDER_MANIFEST_SHA256:-}\""));
    assert!(dockerfile.contains("test \"$(sha256sum /release/manifest.json | cut -d' ' -f1)\" = \"$PROVIDER_MANIFEST_SHA256\""));
    assert!(dockerfile.contains("releaseAsset"));
    assert!(!dockerfile.contains("cargo build"));
    assert!(!dockerfile.contains("git apply"));
}
