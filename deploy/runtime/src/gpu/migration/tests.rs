use super::*;
use crate::{
    compile::CompiledComponent,
    helm_bundle::{ChartMetadata, CompiledHelmRelease},
    ownership::live_tests::{Namespace, component},
};
use veoveo_deploy_contract::components::lock_component;

fn deployment(namespace: &str, name: &str) -> CompiledComponent {
    let mut component = component(namespace, name, &[name]);
    let unit = &mut component.units[0];
    // API fixture only: zero replicas and a synthetic image never execute a Pod.
    unit.objects = vec![serde_json::json!({
        "apiVersion":"apps/v1","kind":"Deployment","metadata":{"name":name,"namespace":namespace},
        "spec":{"replicas":0,"selector":{"matchLabels":{"app":name}},"template":{"metadata":{"labels":{"app":name}},"spec":{"containers":[{"name":"fixture","image":format!("example.invalid/fixture@sha256:{}", "a".repeat(64))}]}}}
    })];
    unit.prepared.objects = ObjectScopes::default()
        .rendered(&unit.objects, namespace, &BTreeSet::new())
        .unwrap();
    component.locked.declaration.permitted_objects = unit
        .prepared
        .objects
        .iter()
        .map(|object| object.identity.clone())
        .collect();
    component.locked = lock_component(
        component.locked.declaration.clone(),
        vec![unit.prepared.clone()],
    )
    .unwrap();
    unit.helm = Some(
        CompiledHelmRelease::prepare(
            &ChartMetadata {
                api_version: "v2".into(),
                name: name.into(),
                version: "1.0.0".into(),
                app_version: None,
            },
            &unit.objects,
            60,
        )
        .unwrap(),
    );
    component
}

#[test]
fn migration_scope_requires_selected_workload_owners_and_retired_objects() {
    let platform = deployment("test", "platform");
    let extension = deployment("test", "extension");
    let catalog = vec![platform.locked.clone(), extension.locked.clone()];
    let selected = BTreeSet::from([platform.locked.declaration.id.clone()]);
    let platform_identity = platform.units[0].prepared.objects[0].identity.clone();
    let extension_identity = extension.units[0].prepared.objects[0].identity.clone();
    assert!(
        selected_workload_owners(
            &catalog,
            &selected,
            &BTreeSet::from([platform_identity.clone()])
        )
        .is_ok()
    );
    assert!(
        selected_workload_owners(&catalog, &selected, &BTreeSet::from([extension_identity]))
            .unwrap_err()
            .to_string()
            .contains("unselected")
    );
    let mut unknown = platform_identity.clone();
    unknown.name = "outside-catalog".into();
    assert!(selected_workload_owners(&catalog, &selected, &BTreeSet::from([unknown])).is_err());
    assert!(validate_retirement(&catalog, std::iter::once(&platform_identity)).is_err());
}

#[test]
#[ignore = "requires explicit context and digest-pinned VEOVEO_SCOPE_TEST_IMAGE with /bin/sleep; tests API ownership, not GPU runtime"]
fn live_quiesce_restores_a_planned_helm_workload() {
    let context = std::env::var("VEOVEO_OWNERSHIP_TEST_CONTEXT").expect("explicit live context");
    let image =
        std::env::var("VEOVEO_SCOPE_TEST_IMAGE").expect("explicit qualified CPU fixture image");
    veoveo_extension_contract::ArtifactDigest::new(
        image
            .split_once('@')
            .expect("image must be digest-pinned")
            .1,
    )
    .unwrap();
    let name = format!(
        "veoveo-restore-{}",
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
    let mut platform = deployment(&namespace.name, "platform");
    let unit = &mut platform.units[0];
    unit.objects[0]["spec"]["replicas"] = serde_json::json!(1);
    unit.objects[0]["spec"]["template"]["spec"]["terminationGracePeriodSeconds"] =
        serde_json::json!(2);
    unit.objects[0]["spec"]["template"]["spec"]["containers"][0]["image"] =
        serde_json::json!(image);
    unit.objects[0]["spec"]["template"]["spec"]["containers"][0]["command"] =
        serde_json::json!(["/bin/sleep", "600"]);
    unit.prepared.objects = ObjectScopes::default()
        .rendered(&unit.objects, &namespace.name, &BTreeSet::new())
        .unwrap();
    platform.locked = lock_component(
        platform.locked.declaration.clone(),
        vec![unit.prepared.clone()],
    )
    .unwrap();
    unit.helm = Some(
        CompiledHelmRelease::prepare(
            &ChartMetadata {
                api_version: "v2".into(),
                name: "platform".into(),
                version: "1.0.0".into(),
                app_version: None,
            },
            &unit.objects,
            60,
        )
        .unwrap(),
    );
    let allocator = component(&namespace.name, "allocator", &["allocator"]);
    let retired = component(&namespace.name, "old-driver", &["old-driver"]);
    retired.units[0]
        .helm
        .as_ref()
        .unwrap()
        .install(&namespace.context, &namespace.name, "old-driver")
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    status_checked("git", ["init", "--quiet"], &[], Some(root.path())).unwrap();
    let state =
        crate::installed::InstalledState::open(root.path(), &namespace.context, &[]).unwrap();
    assert_eq!(
        state.apply(&platform, &platform.units[0]).unwrap(),
        crate::installed::InstallOutcome::Applied { recorded: true }
    );
    let catalog = vec![allocator.locked.clone(), platform.locked.clone()];
    let selected = BTreeSet::from([
        allocator.locked.declaration.id.clone(),
        platform.locked.declaration.id.clone(),
    ]);
    let targets = BTreeSet::from([platform.units[0].prepared.objects[0].identity.clone()]);
    let migration = prepare_targets(
        &namespace.context,
        &ConflictingGpuDevicePluginRemoval::UninstallHelmRelease {
            namespace: namespace.name.clone(),
            release_name: "old-driver".into(),
            expected_chart_version: "1.0.0".into(),
        },
        &allocator.units[0].prepared.target,
        &targets,
        &catalog,
        &selected,
    )
    .unwrap();
    let requested = BTreeSet::from([platform.locked.declaration.id.clone()]);
    let plan = state
        .plan(
            &catalog,
            &requested,
            std::slice::from_ref(&platform),
            &migration.quiesced_workloads(),
        )
        .unwrap();
    assert_eq!(
        plan.mutations[0].verb,
        veoveo_deploy_contract::components::ComponentMutationVerb::HelmUpgradeInstall
    );
    migration.apply(&namespace.context).unwrap();
    let pods: serde_json::Value = serde_json::from_slice(
        &output_checked(
            "kubectl",
            [
                "--context",
                &namespace.context,
                "--namespace",
                &namespace.name,
                "get",
                "pods",
                "--output=json",
            ],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(
        pods["items"].as_array().unwrap().is_empty(),
        "old Pods must exit before driver retirement"
    );
    assert_eq!(
        read_objects(&namespace.context, &targets, None)
            .unwrap()
            .values()
            .next()
            .unwrap()["spec"]["replicas"],
        0
    );
    assert_eq!(
        state
            .apply_planned(&platform, &platform.units[0], &plan)
            .unwrap(),
        crate::installed::InstallOutcome::Applied { recorded: true }
    );
    let restored = read_objects(&namespace.context, &targets, None).unwrap();
    assert_eq!(restored.values().next().unwrap()["spec"]["replicas"], 1);
    assert_eq!(
        restored.values().next().unwrap()["status"]["readyReplicas"],
        1
    );
    println!(
        "Planned Helm release restored its ready replica after the checked quiesce operation without taking another manager's fields."
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

#[test]
#[ignore = "requires VEOVEO_OWNERSHIP_TEST_CONTEXT; isolated API fixtures, no GPU workload acceptance"]
fn live_migration_rejects_unselected_workloads_and_drift_before_quiescing() {
    let context = std::env::var("VEOVEO_OWNERSHIP_TEST_CONTEXT").expect("explicit live context");
    let name = format!(
        "veoveo-gpu-scope-{}",
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
    let platform = deployment(&namespace.name, "platform");
    let extension = deployment(&namespace.name, "extension");
    let allocator = component(&namespace.name, "allocator", &["allocator"]);
    let retired = component(
        &namespace.name,
        "old-driver",
        &["old-driver", "late-driver"],
    );
    for compiled in [&platform, &extension, &retired] {
        compiled.units[0]
            .helm
            .as_ref()
            .unwrap()
            .install(
                &namespace.context,
                &namespace.name,
                compiled.locked.declaration.id.as_str(),
            )
            .unwrap();
    }
    let catalog = vec![
        allocator.locked.clone(),
        platform.locked.clone(),
        extension.locked.clone(),
    ];
    let selected = BTreeSet::from([
        allocator.locked.declaration.id.clone(),
        platform.locked.declaration.id.clone(),
    ]);
    let platform_id = platform.units[0].prepared.objects[0].identity.clone();
    let extension_id = extension.units[0].prepared.objects[0].identity.clone();
    let targets = BTreeSet::from([platform_id.clone(), extension_id.clone()]);
    let before = read_objects(&namespace.context, &targets, None).unwrap();
    let policy = ConflictingGpuDevicePluginRemoval::UninstallHelmRelease {
        namespace: namespace.name.clone(),
        release_name: "old-driver".into(),
        expected_chart_version: "1.0.0".into(),
    };
    let target = &allocator.units[0].prepared.target;
    assert!(
        prepare_targets(
            &namespace.context,
            &policy,
            target,
            &targets,
            &catalog,
            &selected
        )
        .unwrap_err()
        .to_string()
        .contains("unselected")
    );
    assert_eq!(
        read_objects(&namespace.context, &targets, None).unwrap(),
        before
    );
    let selected_targets = BTreeSet::from([platform_id]);
    status_checked(
        "kubectl",
        [
            "--context",
            &namespace.context,
            "--namespace",
            &namespace.name,
            "delete",
            "configmap",
            "late-driver",
        ],
        &[],
        None,
    )
    .unwrap();
    let missing_plan = prepare_targets(
        &namespace.context,
        &policy,
        target,
        &selected_targets,
        &catalog,
        &selected,
    )
    .unwrap();
    let mut late = retired.units[0].objects[1].clone();
    crate::helm_bundle::own_object(&mut late, &namespace.name, "old-driver").unwrap();
    crate::process::kubectl_apply_value(&namespace.context, &late).unwrap();
    assert!(
        missing_plan
            .apply(&namespace.context)
            .unwrap_err()
            .to_string()
            .contains("appeared after preflight")
    );
    assert_eq!(
        read_objects(&namespace.context, &targets, None).unwrap(),
        before
    );
    let plan = prepare_targets(
        &namespace.context,
        &policy,
        target,
        &selected_targets,
        &catalog,
        &selected,
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
            "old-driver",
            "--type=merge",
            "--patch",
            r#"{"data":{"setting":"changed-after-preflight"}}"#,
        ],
        &[],
        None,
    )
    .unwrap();
    assert!(
        plan.apply(&namespace.context)
            .unwrap_err()
            .to_string()
            .contains("changed after preflight")
    );
    assert_eq!(
        read_objects(&namespace.context, &targets, None).unwrap(),
        before
    );
    let plan = prepare_targets(
        &namespace.context,
        &policy,
        target,
        &selected_targets,
        &catalog,
        &selected,
    )
    .unwrap();
    plan.apply(&namespace.context).unwrap();
    assert!(
        release_metadata(&namespace.context, &namespace.name, "old-driver")
            .unwrap()
            .is_none()
    );
    assert!(
        read_objects(
            &namespace.context,
            &BTreeSet::from([retired.units[0].prepared.objects[0].identity.clone()]),
            None
        )
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        read_objects(&namespace.context, &targets, None).unwrap()[&extension_id],
        before[&extension_id]
    );
    println!(
        "GPU migration inventory: unselected workload and changed retirement rejected before quiescing; checked retirement succeeds and preserves the unselected Deployment."
    );
    let context = namespace.context.clone();
    let name = namespace.name.clone();
    drop(namespace);
    let remaining = output_checked(
        "kubectl",
        [
            "--context",
            &context,
            "get",
            "namespace",
            &name,
            "--ignore-not-found",
            "--output=name",
        ],
        None,
    )
    .unwrap();
    assert!(remaining.iter().all(u8::is_ascii_whitespace));
}
