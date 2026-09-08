use super::*;

#[test]
#[ignore = "requires VEOVEO_OWNERSHIP_TEST_CONTEXT; isolated ConfigMap plan/execution fixtures"]
fn live_plans_use_installed_provenance_and_reject_stale_decisions() {
    let context = std::env::var("VEOVEO_OWNERSHIP_TEST_CONTEXT").expect("explicit live context");
    let name = format!(
        "veoveo-plan-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    status_checked(
        "kubectl",
        ["--context", &context, "create", "namespace", &name],
        &[],
        None,
    )
    .unwrap();
    let namespace = Namespace { context, name };
    let root = tempfile::tempdir().unwrap();
    status_checked("git", ["init", "--quiet"], &[], Some(root.path())).unwrap();
    let installed = InstalledState::open(root.path(), &namespace.context, &[]).unwrap();
    let mut platform = component(&namespace.name, "platform", &["platform"]);
    let extension = component(&namespace.name, "extension", &["extension"]);
    let catalog = vec![platform.locked.clone(), extension.locked.clone()];
    for compiled in [&platform, &extension] {
        let requested = BTreeSet::from([compiled.locked.declaration.id.clone()]);
        let plan = installed
            .plan(
                &catalog,
                &requested,
                std::slice::from_ref(compiled),
                &BTreeSet::new(),
            )
            .unwrap();
        assert_eq!(
            plan.mutations[0].verb,
            ComponentMutationVerb::HelmUpgradeInstall
        );
        assert_eq!(
            installed
                .apply_planned(compiled, &compiled.units[0], &plan)
                .unwrap(),
            InstallOutcome::Applied { recorded: true }
        );
    }
    let requested = BTreeSet::from([platform.locked.declaration.id.clone()]);
    let unchanged = installed
        .plan(
            &catalog,
            &requested,
            std::slice::from_ref(&platform),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(
        unchanged.mutations[0].verb,
        ComponentMutationVerb::Unchanged
    );
    let before = snapshot(&namespace.context, &namespace.name);
    assert_eq!(
        installed
            .apply_planned(&platform, &platform.units[0], &unchanged)
            .unwrap(),
        InstallOutcome::Reused
    );
    assert_eq!(snapshot(&namespace.context, &namespace.name), before);
    platform.units[0].objects[0]["data"]["setting"] = json!("new-content");
    reseal(&mut platform);
    assert!(
        installed
            .apply_planned(&platform, &platform.units[0], &unchanged)
            .unwrap_err()
            .to_string()
            .contains("input changed")
    );
    assert_eq!(snapshot(&namespace.context, &namespace.name), before);
    let catalog = vec![platform.locked.clone(), extension.locked.clone()];
    let plan = installed
        .plan(
            &catalog,
            &requested,
            std::slice::from_ref(&platform),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(
        plan.mutations[0].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );
    assert_eq!(
        plan.unselected_objects,
        extension.locked.declaration.permitted_objects
    );
    installed
        .apply_planned(&platform, &platform.units[0], &plan)
        .unwrap();
    assert_eq!(
        installed
            .helm(&platform, &platform.units[0])
            .unwrap()
            .unwrap()
            .revision
            .revision,
        2
    );
    assert_eq!(
        installed
            .helm(&extension, &extension.units[0])
            .unwrap()
            .unwrap()
            .revision
            .revision,
        1
    );
    let extension_id = &extension.units[0].prepared.objects[0].identity;
    assert_eq!(
        read_objects(
            &namespace.context,
            &BTreeSet::from([extension_id.clone()]),
            None
        )
        .unwrap()[extension_id]["data"]["setting"],
        "public-test"
    );

    let stale = installed
        .plan(
            &catalog,
            &requested,
            std::slice::from_ref(&platform),
            &BTreeSet::new(),
        )
        .unwrap();
    status_checked(
        "kubectl",
        [
            "--context",
            &namespace.context,
            "--namespace",
            &namespace.name,
            "patch",
            "configmap",
            "platform",
            "--type=merge",
            "--patch",
            r#"{"data":{"setting":"external-drift"}}"#,
        ],
        &[],
        None,
    )
    .unwrap();
    let after_drift = snapshot(&namespace.context, &namespace.name);
    assert!(
        installed
            .apply_planned(&platform, &platform.units[0], &stale)
            .unwrap_err()
            .to_string()
            .contains("drifted after preflight")
    );
    assert_eq!(snapshot(&namespace.context, &namespace.name), after_drift);
    let fresh = installed
        .plan(
            &catalog,
            &requested,
            std::slice::from_ref(&platform),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(
        fresh.mutations[0].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );
    // Losing the local receipt does not invent installed-input provenance.
    installed
        .store
        .remove(&extension.units[0].prepared.target)
        .unwrap();
    let unknown = installed
        .plan(
            &catalog,
            &BTreeSet::from([extension.locked.declaration.id.clone()]),
            std::slice::from_ref(&extension),
            &BTreeSet::new(),
        )
        .unwrap();
    assert_eq!(
        unknown.mutations[0].verb,
        ComponentMutationVerb::HelmUpgradeInstall
    );
    println!(
        "Planned execution: absence, installed provenance, one-release update, missing baseline, and rejection of changed inputs or live drift verified."
    );
    let context = namespace.context.clone();
    let name = namespace.name.clone();
    drop(namespace);
    assert!(
        output_checked(
            "kubectl",
            [
                "--context",
                &context,
                "get",
                "namespace",
                &name,
                "--ignore-not-found",
                "--output=name"
            ],
            None
        )
        .unwrap()
        .iter()
        .all(u8::is_ascii_whitespace)
    );
}
