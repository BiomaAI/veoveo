use super::*;
use crate::{
    compile::objects::ObjectScopes,
    helm_bundle::{ChartMetadata, CompiledHelmRelease},
    ownership::live_tests::{Namespace, component, snapshot},
    process::{output_checked, status_checked},
};
use serde_json::json;
use veoveo_extension_contract::SourceRevision;

fn reseal(component: &mut CompiledComponent) {
    let unit = &mut component.units[0];
    unit.prepared.objects = ObjectScopes::default()
        .rendered(&unit.objects, "test", &BTreeSet::new())
        .unwrap();
    component.locked = lock_component(
        component.locked.declaration.clone(),
        vec![unit.prepared.clone()],
    )
    .unwrap();
    if unit.helm.is_some() {
        unit.helm = Some(
            CompiledHelmRelease::prepare(
                &ChartMetadata {
                    api_version: "v2".into(),
                    name: component.locked.declaration.id.to_string(),
                    version: "1.0.0".into(),
                    app_version: None,
                },
                &unit.objects,
                60,
            )
            .unwrap(),
        );
    }
}

fn receipt() -> InstalledUnitReceipt {
    let component = component("test", "platform", &["platform"]);
    InstalledUnitReceipt {
        schema_version: INSTALLED_UNIT_SCHEMA.into(),
        cluster_uid: "cluster-a".into(),
        component: component.locked.declaration.clone(),
        unit: component.locked.units[0].clone(),
        helm: Some(InstalledHelmRevision {
            revision: 1,
            manifest_digest: component.locked.units[0].digest.clone(),
        }),
        objects: vec![InstalledObjectObservation {
            identity: component.locked.units[0].objects[0].identity.clone(),
            state: InstalledObjectState::Present {
                uid: "object-a".into(),
                digest: component.locked.units[0].digest.clone(),
                completed_ttl_job: false,
            },
        }],
    }
}

#[test]
fn receipts_reject_forged_provenance_inventory_completion_and_revision() {
    let valid = receipt();
    valid.validate().unwrap();
    let mut changed = valid.clone();
    changed.unit.content_digest = changed.unit.digest.clone();
    assert!(changed.validate().is_err());
    let mut changed = valid.clone();
    changed.objects.clear();
    assert!(changed.validate().is_err());
    let mut changed = valid.clone();
    changed.objects.push(changed.objects[0].clone());
    assert!(changed.validate().is_err());
    let mut changed = valid.clone();
    changed.helm.as_mut().unwrap().revision = 0;
    assert!(changed.validate().is_err());
    let mut changed = valid.clone();
    changed.helm = None;
    assert!(changed.validate().is_err());
    let mut changed = valid.clone();
    let InstalledObjectState::Present {
        completed_ttl_job, ..
    } = &mut changed.objects[0].state
    else {
        unreachable!()
    };
    *completed_ttl_job = true;
    assert!(changed.validate().is_err());
    let mut value = serde_json::to_value(valid).unwrap();
    assert_eq!(value["objects"][0]["state"]["completedTtlJob"], false);
    value["objects"][0]["state"]["unrecognized"] = json!(true);
    assert!(serde_json::from_value::<InstalledUnitReceipt>(value).is_err());
}

#[test]
fn receipt_store_is_atomic_cluster_bound_and_exclusively_locked() {
    let root = tempfile::tempdir().unwrap();
    let receipt = receipt();
    let store = ReceiptStore::at(root.path(), "cluster-a".into()).unwrap();
    assert!(ReceiptStore::at(root.path(), "cluster-a".into()).is_err());
    store.save(&receipt).unwrap();
    assert_eq!(
        store.load(&receipt.unit.target).unwrap(),
        Some(receipt.clone())
    );
    let other = ReceiptStore::at(root.path(), "cluster-b".into()).unwrap();
    assert!(other.load(&receipt.unit.target).unwrap().is_none());
    assert!(other.save(&receipt).is_err());
    store.remove(&receipt.unit.target).unwrap();
    assert!(store.load(&receipt.unit.target).unwrap().is_none());
    store.remove(&receipt.unit.target).unwrap();
}

#[test]
#[ignore = "requires VEOVEO_OWNERSHIP_TEST_CONTEXT; creates isolated ConfigMap releases to verify reuse"]
fn live_installed_reuse_detects_drift_and_preserves_unchanged_revisions() {
    let context =
        std::env::var("VEOVEO_OWNERSHIP_TEST_CONTEXT").expect("explicit live test context");
    let name = format!(
        "veoveo-reuse-{}",
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
    let state = InstalledState::open(root.path(), &namespace.context, &[]).unwrap();
    let mut platform = component(&namespace.name, "platform", &["platform", "hook-cleanup"]);
    platform.units[0].objects[1]["metadata"]["annotations"]["helm.sh/hook-delete-policy"] =
        json!("hook-succeeded");
    reseal(&mut platform);
    let extension = component(&namespace.name, "extension", &["extension"]);
    let first_install_started = std::time::Instant::now();
    for component in [&platform, &extension] {
        assert_eq!(
            state.apply(component, &component.units[0]).unwrap(),
            InstallOutcome::Applied { recorded: true }
        );
    }
    let first_install_elapsed = first_install_started.elapsed();
    let before = snapshot(&namespace.context, &namespace.name);
    let platform_receipt = state
        .receipt(&platform, &platform.units[0])
        .unwrap()
        .unwrap();
    assert!(
        platform_receipt
            .objects
            .iter()
            .any(|object| matches!(object.state, InstalledObjectState::CompletedHook))
    );
    let reuse_started = std::time::Instant::now();
    for component in [&platform, &extension] {
        assert_eq!(
            state.apply(component, &component.units[0]).unwrap(),
            InstallOutcome::Reused
        );
    }
    println!(
        "Two ConfigMap releases: initial apply and receipt {:.3}s; verified unchanged rerun {:.3}s",
        first_install_elapsed.as_secs_f64(),
        reuse_started.elapsed().as_secs_f64()
    );
    assert_eq!(snapshot(&namespace.context, &namespace.name), before);
    // A commit-only change must preserve the original installed provenance.
    let revision = SourceRevision::new("d".repeat(40)).unwrap();
    platform.locked.declaration.source.revision = revision.clone();
    platform.units[0].prepared.source.revision = revision.clone();
    platform.units[0].prepared.inputs = platform.units[0]
        .prepared
        .inputs
        .iter()
        .cloned()
        .map(|mut input| {
            if let ComponentInput::Chart { source, .. } = &mut input {
                source.revision = revision.clone();
            }
            input
        })
        .collect();
    platform.locked.declaration.inputs = platform.units[0].prepared.inputs.clone();
    reseal(&mut platform);
    assert_ne!(
        platform.locked.units[0].digest,
        platform_receipt.unit.digest
    );
    assert_eq!(
        state.apply(&platform, &platform.units[0]).unwrap(),
        InstallOutcome::Reused
    );
    assert_eq!(
        state.receipt(&platform, &platform.units[0]).unwrap(),
        Some(platform_receipt.clone())
    );
    // A requested content change upgrades that release; the other release is reused.
    platform.units[0].objects[0]["data"]["setting"] = json!("updated");
    reseal(&mut platform);
    assert_eq!(
        state.apply(&platform, &platform.units[0]).unwrap(),
        InstallOutcome::Applied { recorded: true }
    );
    assert_eq!(
        state.apply(&extension, &extension.units[0]).unwrap(),
        InstallOutcome::Reused
    );
    let after = state
        .receipt(&platform, &platform.units[0])
        .unwrap()
        .unwrap();
    assert_eq!(
        after.helm.as_ref().unwrap().revision,
        platform_receipt.helm.as_ref().unwrap().revision + 1
    );
    assert_eq!(
        state
            .helm(&extension, &extension.units[0])
            .unwrap()
            .unwrap()
            .revision
            .revision,
        1
    );
    // Out-of-band object content is observed, never copied from the desired lock.
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
            "-p",
            "{\"data\":{\"setting\":\"drift\"}}",
        ],
        &[],
        None,
    )
    .unwrap();
    assert!(
        !state
            .reusable(&platform, &platform.units[0], &after)
            .unwrap()
    );
    // Helm's server-side-apply conflict protection is retained; reuse must not
    // conceal foreign ownership or preserve a receipt after this failed apply.
    assert!(state.apply(&platform, &platform.units[0]).is_err());
    assert!(
        state
            .receipt(&platform, &platform.units[0])
            .unwrap()
            .is_none()
    );
    let stable = state
        .receipt(&extension, &extension.units[0])
        .unwrap()
        .unwrap();
    status_checked(
        "kubectl",
        [
            "--context",
            &namespace.context,
            "--namespace",
            &namespace.name,
            "delete",
            "configmap",
            "extension",
        ],
        &[],
        None,
    )
    .unwrap();
    let mut replacement = extension.units[0].objects[0].clone();
    replacement["metadata"]["labels"] = json!({"app.kubernetes.io/managed-by":"Helm"});
    replacement["metadata"]["annotations"] = json!({"meta.helm.sh/release-name":"extension","meta.helm.sh/release-namespace":namespace.name});
    kubectl_apply_value(&namespace.context, &replacement).unwrap();
    assert!(
        !state
            .reusable(&extension, &extension.units[0], &stable)
            .unwrap()
    );
    // A failed operation invalidates its old receipt before it can change the cluster.
    assert!(
        state
            .run(&extension, &extension.units[0], || anyhow::bail!(
                "intentional apply failure"
            ))
            .is_err()
    );
    assert!(
        state
            .receipt(&extension, &extension.units[0])
            .unwrap()
            .is_none()
    );
    // Manifest sets use the same observation gate without a Helm revision.
    let mut raw = component(&namespace.name, "raw", &["raw"]);
    raw.units[0].helm = None;
    raw.units[0].prepared.target = AtomicTarget::ManifestSet {
        name: "public-resources".into(),
    };
    raw.locked.declaration.targets = BTreeSet::from([raw.units[0].prepared.target.clone()]);
    reseal(&mut raw);
    assert_eq!(
        state.apply(&raw, &raw.units[0]).unwrap(),
        InstallOutcome::Applied { recorded: true }
    );
    let before_raw = snapshot(&namespace.context, &namespace.name);
    assert_eq!(
        state.apply(&raw, &raw.units[0]).unwrap(),
        InstallOutcome::Reused
    );
    assert_eq!(snapshot(&namespace.context, &namespace.name), before_raw);
    println!(
        "Live reuse: unchanged Helm revisions and object versions, preserved installed provenance, one-release content update, drift/UID rejection, successful hook cleanup, and failed-operation invalidation verified."
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
    assert!(
        remaining.iter().all(u8::is_ascii_whitespace),
        "reuse test namespace was not removed"
    );
}
