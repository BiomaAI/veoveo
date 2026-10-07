//! Real selected CLI execution with independent Git, OCI images, and API metadata.
mod fixture;
mod proxy;

use anyhow::{Result, ensure};
use fixture::{Fixture, output};
use proxy::{Proxy, Request};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use veoveo_deploy_contract::components::{
    AtomicToolScope, InstallationReceipt, PreparedAtomicUnit, UnitExecutionOutcome, lock_component,
};

#[derive(Debug, clap::Args)]
pub(crate) struct Args {
    #[arg(long)]
    repository: PathBuf,
    #[arg(long)]
    context: String,
    #[arg(long)]
    push_registry: String,
    #[arg(long)]
    pull_registry: String,
    /// Digest-pinned build parent containing /bin/sleep, reachable from the builder.
    #[arg(long)]
    base_image: String,
    #[arg(long)]
    evidence_output: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evidence {
    schema_version: String,
    namespace: String,
    fixture_directory: PathBuf,
    cases: Vec<Case>,
    canary: Vec<Request>,
    overlap: Vec<Request>,
    failure: Option<String>,
    cleanup_failure: Option<String>,
}
impl Evidence {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == "veoveo.ai/component-scope-evidence/v2",
            "unsupported component-scope evidence format"
        );
        for case in &self.cases {
            ensure!(
                case.receipt.schema_version == "veoveo.ai/component-installation/v3"
                    && case.receipt.plan.schema_version
                        == veoveo_deploy_contract::components::COMPONENT_MUTATION_PLAN_SCHEMA,
                "unsupported nested installation format"
            );
            ensure!(
                case.receipt.plan.requested
                    == std::collections::BTreeSet::from([case.selected.clone()]),
                "report changed selected component"
            );
            ensure!(
                case.unselected_before == case.unselected_after,
                "report changed unselected runtime state"
            );
        }
        Ok(())
    }
    #[cfg(test)]
    fn decode(bytes: &[u8]) -> Result<Self> {
        let value: Self = serde_json::from_slice(bytes)?;
        value.validate()?;
        Ok(value)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    selected: veoveo_deploy_contract::components::ComponentId,
    installation_elapsed_ms: u64,
    receipt: InstallationReceipt,
    requests: Vec<Request>,
    selected_before: RuntimeSnapshot,
    selected_after: RuntimeSnapshot,
    unselected_before: RuntimeSnapshot,
    unselected_after: RuntimeSnapshot,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RuntimeSnapshot {
    pods: Vec<PodState>,
    helm_storage: Vec<StorageState>,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PodState {
    name: String,
    uid: String,
    images: Vec<String>,
    restarts: Vec<u64>,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StorageState {
    name: String,
    uid: String,
    resource_version: String,
}

fn snapshot(proxy: &Proxy, namespace: &str, owner: &str) -> Result<RuntimeSnapshot> {
    #[derive(Deserialize)]
    struct PodList {
        items: Vec<Pod>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct NativeMetadata {
        name: String,
        uid: String,
        resource_version: String,
    }
    #[derive(Deserialize)]
    struct Pod {
        metadata: NativeMetadata,
        status: PodStatus,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PodStatus {
        container_statuses: Vec<ContainerStatus>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ContainerStatus {
        #[serde(rename = "imageID")]
        image_id: String,
        restart_count: u64,
        ready: bool,
    }
    let pods: PodList = serde_json::from_slice(&output(proxy.kubectl().args([
        "--namespace",
        namespace,
        "get",
        "pods",
        "--selector",
        &format!("app={owner}"),
        "--output=json",
    ]))?)?;
    let mut states = Vec::new();
    for pod in pods.items {
        let statuses = pod.status.container_statuses;
        ensure!(
            !statuses.is_empty()
                && statuses
                    .iter()
                    .all(|status| status.ready && !status.image_id.is_empty()),
            "fixture Pod has no Ready container image evidence"
        );
        states.push(PodState {
            name: pod.metadata.name,
            uid: pod.metadata.uid,
            images: statuses
                .iter()
                .map(|status| status.image_id.clone())
                .collect(),
            restarts: statuses.iter().map(|status| status.restart_count).collect(),
        });
    }
    ensure!(
        states.len() == 1,
        "fixture owner must have exactly one running Pod"
    );
    states.sort();
    // Decode only metadata; Helm release Secret bodies never enter evidence.
    #[derive(Deserialize)]
    struct StorageList {
        items: Vec<Storage>,
    }
    #[derive(Deserialize)]
    struct Storage {
        metadata: NativeMetadata,
    }
    let storage: StorageList = serde_json::from_slice(&output(proxy.kubectl().args([
        "--namespace",
        namespace,
        "get",
        "secrets",
        "--selector",
        &format!("owner=helm,name={owner}"),
        "--output=json",
    ]))?)?;
    let mut helm_storage = storage
        .items
        .into_iter()
        .map(|item| StorageState {
            name: item.metadata.name,
            uid: item.metadata.uid,
            resource_version: item.metadata.resource_version,
        })
        .collect::<Vec<_>>();
    helm_storage.sort();
    ensure!(!helm_storage.is_empty(), "fixture Helm storage is missing");
    Ok(RuntimeSnapshot {
        pods: states,
        helm_storage,
    })
}

fn install(
    proxy: &Proxy,
    fixture: &Fixture,
    lock: &Path,
    selected: Option<&str>,
    name: &str,
) -> Result<InstallationReceipt> {
    let path = fixture.directory.join(format!("{name}.installation.json"));
    let mut command = installer(proxy, fixture, lock, &path);
    if let Some(selected) = selected {
        command.args(["--component", selected]);
    } else {
        command.arg("--all-components");
    }
    output(&mut command)?;
    let receipt: InstallationReceipt = serde_json::from_slice(&fs::read(path)?)?;
    let lock: veoveo_deploy_contract::DeploymentLock = serde_json::from_slice(&fs::read(lock)?)?;
    let requested = if let Some(id) = selected {
        std::collections::BTreeSet::from([id.to_owned().try_into()?])
    } else {
        lock.components
            .iter()
            .map(|c| c.declaration.id.clone())
            .collect()
    };
    admit_receipt(&receipt, &lock.components, &requested)?;
    Ok(receipt)
}
fn admit_receipt(
    receipt: &InstallationReceipt,
    catalog: &[veoveo_deploy_contract::components::LockedComponent],
    requested: &std::collections::BTreeSet<veoveo_deploy_contract::components::ComponentId>,
) -> Result<()> {
    use veoveo_deploy_contract::components::{ComponentMutationVerb, select_components};
    ensure!(
        receipt.schema_version == "veoveo.ai/component-installation/v3"
            && receipt.plan.schema_version
                == veoveo_deploy_contract::components::COMPONENT_MUTATION_PLAN_SCHEMA,
        "unsupported component receipt or plan format"
    );
    ensure!(
        &receipt.plan.requested == requested
            && receipt.plan.expanded == select_components(catalog, requested)?,
        "component receipt changed the selected owner closure"
    );
    ensure!(
        receipt.coordination.released && !receipt.coordination.uid.is_empty(),
        "component receipt omitted released coordination"
    );
    let unselected = catalog
        .iter()
        .filter(|component| !receipt.plan.expanded.contains(&component.declaration.id))
        .collect::<Vec<_>>();
    let expected_objects = unselected
        .iter()
        .flat_map(|component| component.declaration.permitted_objects.iter())
        .collect::<std::collections::BTreeSet<_>>();
    let expected_releases = unselected
        .iter()
        .flat_map(|component| component.declaration.targets.iter())
        .filter(|target| {
            matches!(
                target,
                veoveo_deploy_contract::components::AtomicTarget::HelmRelease { .. }
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    ensure!(
        receipt
            .plan
            .unselected_objects
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            == expected_objects,
        "component plan does not cover the unselected locked object inventory"
    );
    for snapshot in [&receipt.unselected_before, &receipt.unselected_after] {
        let mut objects = std::collections::BTreeSet::new();
        for object in &snapshot.objects {
            ensure!(
                objects.insert(&object.identity),
                "component receipt repeats an unselected object"
            );
        }
        let mut releases = std::collections::BTreeSet::new();
        for release in &snapshot.releases {
            ensure!(
                releases.insert(&release.target),
                "component receipt repeats an unselected release"
            );
        }
        ensure!(
            objects == expected_objects,
            "component receipt does not cover the unselected locked object inventory"
        );
        ensure!(
            releases == expected_releases,
            "component receipt does not cover the unselected locked release inventory"
        );
    }
    ensure!(
        receipt.unselected_before == receipt.unselected_after,
        "component receipt changed unselected state"
    );
    let expected = catalog
        .iter()
        .filter(|c| receipt.plan.expanded.contains(&c.declaration.id))
        .flat_map(|c| {
            c.units
                .iter()
                .map(move |u| ((&c.declaration.id, &u.target), (c, u)))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut seen = std::collections::BTreeSet::new();
    for mutation in &receipt.plan.mutations {
        let key = (&mutation.component, &mutation.target);
        ensure!(seen.insert(key), "component receipt repeats a mutation");
        let (component, unit) = expected
            .get(&key)
            .ok_or_else(|| anyhow::anyhow!("component receipt includes an unselected target"))?;
        ensure!(
            mutation.source == component.declaration.source
                && mutation.configuration == component.declaration.configuration
                && mutation.digest == unit.digest
                && mutation.content_digest == unit.content_digest
                && mutation.objects == unit.objects,
            "component receipt does not match selected locked content"
        );
    }
    ensure!(
        seen.len() == expected.len(),
        "component receipt omits selected targets"
    );
    let mut operations = std::collections::BTreeSet::new();
    for operation in &receipt.operations {
        let key = (&operation.component, &operation.target);
        ensure!(
            operations.insert(key),
            "component receipt repeats an operation"
        );
        let mutation = receipt
            .plan
            .mutations
            .iter()
            .find(|m| (&m.component, &m.target) == key)
            .ok_or_else(|| anyhow::anyhow!("component operation is outside the admitted plan"))?;
        ensure!(
            (operation.outcome == UnitExecutionOutcome::Reused)
                == (mutation.verb == ComponentMutationVerb::Unchanged),
            "component outcome disagrees with admitted mutation"
        );
    }
    ensure!(
        operations == seen,
        "component receipt operations do not settle its complete plan"
    );
    Ok(())
}

fn installer(proxy: &Proxy, fixture: &Fixture, lock: &Path, receipt: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("current smoke binary"));
    command
        .env("KUBECONFIG", &proxy.config)
        .arg("profile-up")
        .arg("--profile")
        .arg(&fixture.profile.path)
        .arg("--lock")
        .arg(lock)
        .arg("--receipt-output")
        .arg(receipt);
    command
}

pub(crate) fn verify(args: Args) -> Result<()> {
    ensure!(
        !args.evidence_output.exists(),
        "scope evidence already exists"
    );
    let namespace = format!(
        "veoveo-scope-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let parent = args
        .evidence_output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let directory = fs::canonicalize(parent)?.join(&namespace);
    fs::create_dir(&directory)?;
    let mut evidence = Evidence {
        schema_version: "veoveo.ai/component-scope-evidence/v2".to_owned(),
        namespace: namespace.clone(),
        fixture_directory: directory.clone(),
        cases: Vec::new(),
        canary: Vec::new(),
        overlap: Vec::new(),
        failure: None,
        cleanup_failure: None,
    };
    let mut proxy = Proxy::start(&args.context, &namespace, &directory)?;
    let mut cleanup = Cleanup {
        config: proxy.config.clone(),
        context: proxy.context.clone(),
        namespace: namespace.clone(),
        complete: false,
    };
    let result = (|| -> Result<()> {
        let mut fixture = Fixture::create(&args, &namespace, &proxy.context, &directory)?;
        let base = fixture.lock_file("initial")?;
        install(&proxy, &fixture, &base, None, "initial")?;
        let start = proxy.checkpoint()?;
        output(proxy.kubectl().args([
            "--namespace",
            &namespace,
            "create",
            "configmap",
            "scope-observer-canary",
            "--from-literal=fixture=public",
        ]))?;
        output(proxy.kubectl().args([
            "--namespace",
            &namespace,
            "patch",
            "configmap",
            "scope-observer-canary",
            "--type=merge",
            "--patch",
            r#"{"data":{"fixture":"changed"}}"#,
        ]))?;
        output(proxy.kubectl().args([
            "--namespace",
            &namespace,
            "delete",
            "configmap",
            "scope-observer-canary",
        ]))?;
        evidence.canary = proxy.since(start)?;
        for method in [
            proxy::Method::Post,
            proxy::Method::Patch,
            proxy::Method::Delete,
        ] {
            ensure!(
                evidence
                    .canary
                    .iter()
                    .any(|request| request.method == method),
                "observer missed a canary write"
            );
        }
        for (selected, unselected) in [("platform", "workload"), ("workload", "platform")] {
            fixture.advance(selected)?;
            let lock = fixture.lock_file(selected)?;
            let before = snapshot(&proxy, &namespace, unselected)?;
            let selected_before = snapshot(&proxy, &namespace, selected)?;
            let hidden = fixture.hide(unselected)?;
            let start = proxy.checkpoint()?;
            let installation_started = Instant::now();
            let receipt = install(&proxy, &fixture, &lock, Some(selected), selected)?;
            let installation_elapsed_ms =
                u64::try_from(installation_started.elapsed().as_millis())?;
            let requests = proxy.since(start)?;
            drop(hidden);
            let after = snapshot(&proxy, &namespace, unselected)?;
            output(proxy.kubectl().args([
                "--namespace",
                &namespace,
                "wait",
                "--for=delete",
                &format!("pod/{}", selected_before.pods[0].name),
                "--timeout=30s",
            ]))?;
            let selected_after = snapshot(&proxy, &namespace, selected)?;
            ensure!(
                selected_before.pods[0].uid != selected_after.pods[0].uid
                    && selected_before.pods[0].images != selected_after.pods[0].images,
                "selected update did not replace its Pod with a different image"
            );
            ensure!(
                before == after,
                "unselected runtime or Helm storage changed"
            );
            ensure!(
                receipt.unselected_before == receipt.unselected_after,
                "unselected API object or Helm revision changed"
            );
            let applied = receipt
                .operations
                .iter()
                .filter(|operation| operation.outcome == UnitExecutionOutcome::Applied)
                .collect::<Vec<_>>();
            ensure!(
                applied.len() == 1 && applied[0].component.as_str() == selected,
                "update did not apply exactly one selected component"
            );
            ensure!(
                receipt.schema_version == "veoveo.ai/component-installation/v3"
                    && receipt.coordination.released
                    && !receipt.coordination.uid.is_empty()
                    && receipt.coordination.object.group == "coordination.k8s.io"
                    && receipt.coordination.object.kind == "Lease"
                    && receipt.coordination.object.namespace.as_deref() == Some("kube-system")
                    && receipt.coordination.object.name == "veoveo-profile-mutation",
                "installation omitted released cluster coordination"
            );
            let lock_collection = "/apis/coordination.k8s.io/v1/namespaces/kube-system/leases";
            let lock_resource = format!("{lock_collection}/{}", receipt.coordination.object.name);
            ensure!(
                requests
                    .iter()
                    .filter(|request| request.method == proxy::Method::Post
                        && request.uri.split('?').next() == Some(lock_collection))
                    .count()
                    == 1,
                "observer did not see exactly one coordination acquisition"
            );
            ensure!(
                requests
                    .iter()
                    .filter(|request| request.method == proxy::Method::Delete
                        && request.uri.split('?').next() == Some(lock_resource.as_str()))
                    .count()
                    == 1,
                "observer did not see exactly one coordination release"
            );
            let writes = requests
                .iter()
                .filter(|request| request.method.writes())
                .collect::<Vec<_>>();
            ensure!(!writes.is_empty(), "observer missed the selected update");
            for request in writes {
                ensure!(
                    !request.uri.contains(&format!("/{unselected}"))
                        && !request.uri.contains(&format!(".{unselected}.v")),
                    "API write addressed an unselected object"
                );
                ensure!(
                    request.uri.starts_with(&format!(
                        "/apis/apps/v1/namespaces/{namespace}/deployments/{selected}"
                    )) || request
                        .uri
                        .starts_with(&format!("/api/v1/namespaces/{namespace}/secrets"))
                        || (request.method == proxy::Method::Post
                            && request.uri.split('?').next() == Some(lock_collection))
                        || (request.method == proxy::Method::Delete
                            && request.uri.split('?').next() == Some(lock_resource.as_str())),
                    "update wrote outside selected application objects and its exact coordination Lease"
                );
            }
            println!(
                "{selected}-only update: unchanged {unselected} Deployment, Pods, Helm storage and revision; {} observed requests",
                requests.len()
            );
            evidence.cases.push(Case {
                selected: selected.to_owned().try_into()?,
                installation_elapsed_ms,
                receipt,
                requests,
                selected_before,
                selected_after,
                unselected_before: before,
                unselected_after: after,
            });
        }
        let mut overlap = fixture.lock.clone();
        let extra = overlap
            .components
            .iter()
            .find(|component| component.declaration.id.as_str() == "workload")
            .unwrap()
            .declaration
            .permitted_objects
            .clone();
        let component = overlap
            .components
            .iter_mut()
            .find(|component| component.declaration.id.as_str() == "platform")
            .unwrap();
        let mut declaration = component.declaration.clone();
        declaration.permitted_objects.extend(extra);
        let units = component
            .units
            .iter()
            .map(|unit| PreparedAtomicUnit {
                component: declaration.id.clone(),
                source: declaration.source.clone(),
                configuration: declaration.configuration.clone(),
                target: unit.target.clone(),
                inputs: unit.inputs.clone(),
                objects: unit.objects.clone(),
                tool_scope: AtomicToolScope::Exact,
            })
            .collect();
        *component = lock_component(declaration, units)?;
        let path = directory.join("overlap.lock.json");
        fs::write(&path, serde_json::to_vec_pretty(&overlap)?)?;
        let start = proxy.checkpoint()?;
        let rejected = installer(
            &proxy,
            &fixture,
            &path,
            &directory.join("overlap.installation.json"),
        )
        .args(["--component", "platform"])
        .output()?;
        ensure!(
            !rejected.status.success(),
            "overlapping ownership was accepted"
        );
        let diagnostic = String::from_utf8_lossy(&rejected.stderr);
        ensure!(
            diagnostic.contains("ownership")
                || diagnostic.contains("owned by")
                || diagnostic.contains("overlap"),
            "overlap failed for an unrelated reason: {diagnostic}"
        );
        evidence.overlap = proxy.since(start)?;
        ensure!(
            evidence
                .overlap
                .iter()
                .all(|request| !request.method.writes()),
            "overlap issued an API write"
        );
        Ok(())
    })();
    if let Err(error) = &result {
        evidence.failure = Some(format!("{error:#}"));
    }
    if let Err(error) = cleanup.remove() {
        evidence.cleanup_failure = Some(format!("{error:#}"));
    }
    evidence.validate()?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.evidence_output)?;
    serde_json::to_writer_pretty(&mut file, &evidence)?;
    file.sync_all()?;
    result?;
    ensure!(evidence.cleanup_failure.is_none(), "fixture cleanup failed");
    println!(
        "Component scope verified; evidence: {}",
        args.evidence_output.display()
    );
    Ok(())
}

struct Cleanup {
    config: PathBuf,
    context: String,
    namespace: String,
    complete: bool,
}
impl Cleanup {
    fn remove(&mut self) -> Result<()> {
        if self.complete {
            return Ok(());
        }
        let mut command = Command::new("kubectl");
        command
            .env("KUBECONFIG", &self.config)
            .args(["--context", &self.context]);
        output(command.args([
            "delete",
            "namespace",
            &self.namespace,
            "--ignore-not-found",
            "--wait=true",
            "--timeout=60s",
        ]))?;
        let absent = output(
            Command::new("kubectl")
                .env("KUBECONFIG", &self.config)
                .args([
                    "--context",
                    &self.context,
                    "get",
                    "namespace",
                    &self.namespace,
                    "--ignore-not-found",
                    "--output=name",
                ]),
        )?;
        ensure!(
            absent.iter().all(u8::is_ascii_whitespace),
            "fixture namespace remains"
        );
        self.complete = true;
        Ok(())
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use std::collections::BTreeSet;
    use veoveo_deploy_contract::{DeploymentLock, components::*};

    #[test]
    fn complete_current_receipt_checks_selected_content_and_nested_wire() {
        let lock: DeploymentLock = serde_json::from_str(include_str!(
            "../../../../deploy/contract/tests/fixtures/deployment-lock.json"
        ))
        .unwrap();
        let requested = BTreeSet::from([lock.components[0].declaration.id.clone()]);
        let expanded = select_components(&lock.components, &requested).unwrap();
        let mut prepared = Vec::new();
        let mut observed = Vec::new();
        for component in lock
            .components
            .iter()
            .filter(|c| expanded.contains(&c.declaration.id))
        {
            for unit in &component.units {
                prepared.push(PreparedAtomicUnit {
                    component: component.declaration.id.clone(),
                    source: component.declaration.source.clone(),
                    configuration: component.declaration.configuration.clone(),
                    target: unit.target.clone(),
                    inputs: unit.inputs.clone(),
                    objects: unit.objects.clone(),
                    tool_scope: AtomicToolScope::Exact,
                });
                observed.push(ObservedAtomicUnit {
                    component: component.declaration.id.clone(),
                    target: unit.target.clone(),
                    state: ObservedUnitState::Absent,
                });
            }
        }
        let plan =
            component_mutation_plan(&lock.components, &requested, &prepared, &observed).unwrap();
        let unselected = lock
            .components
            .iter()
            .filter(|component| !expanded.contains(&component.declaration.id))
            .collect::<Vec<_>>();
        let absent = UnselectedState {
            objects: unselected
                .iter()
                .flat_map(|component| component.declaration.permitted_objects.iter())
                .map(|identity| ObjectSnapshot {
                    identity: identity.clone(),
                    state: None,
                })
                .collect(),
            releases: unselected
                .iter()
                .flat_map(|component| component.declaration.targets.iter())
                .filter(|target| matches!(target, AtomicTarget::HelmRelease { .. }))
                .map(|target| ReleaseSnapshot {
                    target: target.clone(),
                    state: None,
                })
                .collect(),
        };
        assert!(!absent.objects.is_empty());
        assert!(!absent.releases.is_empty());
        let receipt = InstallationReceipt {
            schema_version: "veoveo.ai/component-installation/v3".into(),
            coordination: InstallationCoordination {
                object: ObjectIdentity {
                    group: "coordination.k8s.io".into(),
                    kind: "Lease".into(),
                    namespace: Some("fixture".into()),
                    name: "installation".into(),
                },
                uid: "fixture-lease".into(),
                holder_identity: "selected-invocation".into(),
                released: true,
            },
            operations: plan
                .mutations
                .iter()
                .map(|m| UnitExecution {
                    component: m.component.clone(),
                    target: m.target.clone(),
                    outcome: UnitExecutionOutcome::Applied,
                })
                .collect(),
            plan,
            unselected_before: absent.clone(),
            unselected_after: absent,
        };
        admit_receipt(&receipt, &lock.components, &requested).unwrap();
        // Matching incomplete snapshots cannot establish unchanged unselected owners.
        for inventory in ["objects", "releases"] {
            let original = serde_json::to_value(&receipt.unselected_before).unwrap();
            for corruption in ["empty", "missing", "duplicate", "extra", "foreign"] {
                let mut changed = original.clone();
                let entries = changed.get_mut(inventory).unwrap().as_array_mut().unwrap();
                match corruption {
                    "empty" => entries.clear(),
                    "missing" => {
                        entries.pop().unwrap();
                    }
                    "duplicate" => entries.push(entries[0].clone()),
                    "extra" | "foreign" => {
                        let mut foreign = entries[0].clone();
                        let parent = if inventory == "objects" {
                            "identity"
                        } else {
                            "target"
                        };
                        *foreign.get_mut(parent).unwrap().get_mut("name").unwrap() =
                            "unlocked-owner".into();
                        if corruption == "extra" {
                            entries.push(foreign);
                        } else {
                            entries[0] = foreign;
                        }
                    }
                    _ => unreachable!(),
                }
                for which in ["before", "after", "both"] {
                    let mut bad = receipt.clone();
                    if which != "after" {
                        bad.unselected_before = serde_json::from_value(changed.clone()).unwrap();
                    }
                    if which != "before" {
                        bad.unselected_after = serde_json::from_value(changed.clone()).unwrap();
                    }
                    let error = admit_receipt(&bad, &lock.components, &requested).unwrap_err();
                    assert!(
                        error.to_string().contains("unselected"),
                        "{inventory}/{corruption}/{which}: {error}"
                    );
                }
            }
        }
        let mut bad = receipt.clone();
        bad.plan.unselected_objects.clear();
        assert!(
            admit_receipt(&bad, &lock.components, &requested)
                .unwrap_err()
                .to_string()
                .contains("unselected locked object inventory")
        );
        let mut bad = receipt.clone();
        let mut extra = bad.plan.unselected_objects.first().unwrap().clone();
        extra.name = "unlocked-owner".into();
        bad.plan.unselected_objects.insert(extra);
        assert!(
            admit_receipt(&bad, &lock.components, &requested)
                .unwrap_err()
                .to_string()
                .contains("unselected locked object inventory")
        );
        let mut bad = receipt.clone();
        bad.unselected_after.objects[0].state = Some(ObservedObjectVersion {
            uid: "changed-object".into(),
            resource_version: "2".into(),
            digest: lock.components[0].units[0].digest.clone(),
        });
        assert!(
            admit_receipt(&bad, &lock.components, &requested)
                .unwrap_err()
                .to_string()
                .contains("changed unselected state")
        );
        let mut bad = receipt.clone();
        bad.unselected_after.releases[0].state = Some(ObservedReleaseVersion {
            revision: 2,
            status: "deployed".into(),
            chart: "fixture".into(),
            app_version: "current".into(),
        });
        assert!(
            admit_receipt(&bad, &lock.components, &requested)
                .unwrap_err()
                .to_string()
                .contains("changed unselected state")
        );
        let current = serde_json::to_value(&receipt).unwrap();
        let decoded: InstallationReceipt = serde_json::from_value(current.clone()).unwrap();
        admit_receipt(&decoded, &lock.components, &requested).unwrap();
        for (parent, key, old) in [
            ("", "schemaVersion", "schema_version"),
            ("/coordination", "holderIdentity", "holder_identity"),
            ("/plan", "contentDigest", "content_digest"),
        ] {
            // The plan's content digest belongs to each mutation, not its enclosing plan.
            let parent = if key == "contentDigest" {
                "/plan/mutations/0"
            } else {
                parent
            };
            for mixed in [false, true] {
                let mut bad = current.clone();
                let object = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
                let value = if mixed {
                    object[key].clone()
                } else {
                    object.remove(key).unwrap()
                };
                object.insert(old.into(), value);
                assert!(
                    serde_json::from_value::<InstallationReceipt>(bad).is_err(),
                    "{parent}/{old}"
                );
            }
        }
        let report = Evidence {
            schema_version: "veoveo.ai/component-scope-evidence/v2".into(),
            namespace: "fixture".into(),
            fixture_directory: PathBuf::from("fixture"),
            canary: vec![],
            overlap: vec![],
            failure: None,
            cleanup_failure: None,
            cases: vec![Case {
                selected: requested.first().unwrap().clone(),
                installation_elapsed_ms: 1,
                receipt: receipt.clone(),
                requests: vec![],
                selected_before: RuntimeSnapshot {
                    pods: vec![],
                    helm_storage: vec![],
                },
                selected_after: RuntimeSnapshot {
                    pods: vec![],
                    helm_storage: vec![],
                },
                unselected_before: RuntimeSnapshot {
                    pods: vec![],
                    helm_storage: vec![],
                },
                unselected_after: RuntimeSnapshot {
                    pods: vec![],
                    helm_storage: vec![],
                },
            }],
        };
        let report_wire = serde_json::to_value(&report).unwrap();
        Evidence::decode(&serde_json::to_vec(&report_wire).unwrap()).unwrap();
        for pointer in ["/schemaVersion", "/cases/0/receipt/schemaVersion"] {
            let mut bad = report_wire.clone();
            *bad.pointer_mut(pointer).unwrap() = serde_json::json!("retired/v1");
            assert!(Evidence::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for mixed in [false, true] {
            let mut bad = report_wire.clone();
            let object = bad.as_object_mut().unwrap();
            let value = if mixed {
                object["schemaVersion"].clone()
            } else {
                object.remove("schemaVersion").unwrap()
            };
            object.insert("schema_version".into(), value);
            assert!(Evidence::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut bad = receipt.clone();
        bad.coordination.released = false;
        assert!(admit_receipt(&bad, &lock.components, &requested).is_err());
        let mut bad = receipt.clone();
        bad.operations.push(bad.operations[0].clone());
        assert!(admit_receipt(&bad, &lock.components, &requested).is_err());
        let mut bad = receipt.clone();
        bad.plan.mutations[0].digest =
            veoveo_deploy_contract::ArtifactDigest::parse(format!("sha256:{}", "0".repeat(64)))
                .unwrap();
        assert!(admit_receipt(&bad, &lock.components, &requested).is_err());
        let other = BTreeSet::from([lock.components.last().unwrap().declaration.id.clone()]);
        assert!(admit_receipt(&receipt, &lock.components, &other).is_err());
    }
}
