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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: &'static str,
    namespace: String,
    fixture_directory: PathBuf,
    cases: Vec<Case>,
    canary: Vec<Request>,
    overlap: Vec<Request>,
    failure: Option<String>,
    cleanup_failure: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    selected: String,
    installation_elapsed_ms: u64,
    receipt: InstallationReceipt,
    requests: Vec<Request>,
    selected_before: RuntimeSnapshot,
    selected_after: RuntimeSnapshot,
    unselected_before: RuntimeSnapshot,
    unselected_after: RuntimeSnapshot,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeSnapshot {
    pods: Vec<PodState>,
    helm_storage: Vec<StorageState>,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PodState {
    name: String,
    uid: String,
    images: Vec<String>,
    restarts: Vec<u64>,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    struct Pod {
        metadata: StorageState,
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
        metadata: StorageState,
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
        .map(|item| item.metadata)
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
    Ok(serde_json::from_slice(&fs::read(path)?)?)
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
        schema_version: "veoveo.io/component-scope-evidence/v1",
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
        for (selected, unselected) in [("platform", "extension"), ("extension", "platform")] {
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
                receipt.schema_version == "veoveo.io/component-installation/v2"
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
                selected: selected.into(),
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
            .find(|component| component.declaration.id.as_str() == "extension")
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
