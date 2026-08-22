use std::{
    fs::{self, OpenOptions},
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

const EVIDENCE_SCHEMA: &str = "veoveo.io/gitops-convergence-evidence/v2";
const GIT_REPOSITORY_RESOURCE: &str = "gitrepositories.source.toolkit.fluxcd.io";
const KUSTOMIZATION_RESOURCE: &str = "kustomizations.kustomize.toolkit.fluxcd.io";
const HELM_RELEASE_RESOURCE: &str = "helmreleases.helm.toolkit.fluxcd.io";

#[derive(Debug)]
pub(crate) struct GitopsConvergeArgs {
    pub(crate) context: String,
    pub(crate) source: String,
    pub(crate) root: String,
    pub(crate) releases: Vec<String>,
    pub(crate) revision: String,
    pub(crate) deployments: Vec<String>,
    pub(crate) timeout: Duration,
    pub(crate) evidence_output: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvergenceEvidence {
    schema_version: &'static str,
    observed_at_unix_millis: u128,
    context: String,
    source: ObjectRef,
    root: ObjectRef,
    releases: Vec<ReleaseObservation>,
    expected_revision: String,
    deployments: Vec<DeploymentRef>,
    timeout_millis: u128,
    elapsed_millis: u128,
    outcome: EvidenceOutcome,
    phases: Vec<PhaseEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObjectRef {
    namespace: String,
    name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentRef {
    namespace: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseObservation {
    namespace: String,
    name: String,
    ready: bool,
    attempted_revision: Option<String>,
    inventory_entries: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum EvidenceOutcome {
    Succeeded,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhaseEvidence {
    phase: &'static str,
    elapsed_millis: u128,
    status: PhaseStatus,
    diagnostic: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum PhaseStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FluxMetadata {
    #[serde(default)]
    generation: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitRepository {
    metadata: FluxMetadata,
    #[serde(default)]
    status: GitRepositoryStatus,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GitRepositoryStatus {
    observed_generation: Option<i64>,
    artifact: Option<SourceArtifact>,
    #[serde(default)]
    conditions: Vec<FluxCondition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceArtifact {
    revision: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FluxKustomization {
    metadata: FluxMetadata,
    #[serde(default)]
    status: KustomizationStatus,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KustomizationStatus {
    observed_generation: Option<i64>,
    last_applied_revision: Option<String>,
    #[serde(default)]
    conditions: Vec<FluxCondition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelmRelease {
    metadata: FluxMetadata,
    #[serde(default)]
    status: HelmReleaseStatus,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HelmReleaseStatus {
    observed_generation: Option<i64>,
    last_attempted_revision: Option<String>,
    inventory: Option<HelmInventory>,
    #[serde(default)]
    conditions: Vec<FluxCondition>,
}

#[derive(Debug, Deserialize)]
struct HelmInventory {
    #[serde(default)]
    entries: Vec<HelmInventoryEntry>,
}

#[derive(Debug, Deserialize)]
struct HelmInventoryEntry {
    #[allow(dead_code)]
    id: String,
}

#[derive(Debug, Deserialize)]
struct FluxCondition {
    #[serde(rename = "type")]
    condition_type: String,
    status: String,
}

struct Deadline {
    started: Instant,
    timeout: Duration,
}

impl Deadline {
    fn new(timeout: Duration) -> Self {
        Self {
            started: Instant::now(),
            timeout,
        }
    }

    fn remaining(&self, operation: &str) -> Result<Duration> {
        self.timeout
            .checked_sub(self.started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .with_context(|| format!("GitOps convergence timed out before {operation}"))
    }
}

pub(crate) fn converge(arguments: GitopsConvergeArgs) -> Result<()> {
    validate_revision("--revision", &arguments.revision)?;
    ensure!(
        !arguments.releases.is_empty(),
        "at least one --release is required"
    );
    ensure!(
        !arguments.deployments.is_empty(),
        "at least one --deployment namespace/name is required"
    );
    ensure!(
        !arguments.timeout.is_zero(),
        "--timeout-seconds must be greater than zero"
    );

    let source = ObjectRef::parse(&arguments.source, "Git source")?;
    let root = ObjectRef::parse(&arguments.root, "root Kustomization")?;
    let releases = arguments
        .releases
        .iter()
        .map(|value| ObjectRef::parse(value, "Helm release"))
        .collect::<Result<Vec<_>>>()?;
    let deployments = arguments
        .deployments
        .iter()
        .map(|value| DeploymentRef::parse(value))
        .collect::<Result<Vec<_>>>()?;
    let deadline = Deadline::new(arguments.timeout);
    let mut phases = Vec::new();

    let result = converge_inner(
        &arguments,
        &source,
        &root,
        &releases,
        &deployments,
        &deadline,
        &mut phases,
    );
    let release_observations = releases
        .iter()
        .filter_map(|release| observe_release(&arguments, release).ok())
        .collect();
    let evidence = ConvergenceEvidence {
        schema_version: EVIDENCE_SCHEMA,
        observed_at_unix_millis: unix_millis()?,
        context: arguments.context.clone(),
        source,
        root,
        releases: release_observations,
        expected_revision: arguments.revision.clone(),
        deployments,
        timeout_millis: arguments.timeout.as_millis(),
        elapsed_millis: deadline.started.elapsed().as_millis(),
        outcome: if result.is_ok() {
            EvidenceOutcome::Succeeded
        } else {
            EvidenceOutcome::Failed
        },
        phases,
    };
    write_evidence(&arguments.evidence_output, &evidence)?;
    println!(
        "GitOps convergence evidence: {}",
        arguments.evidence_output.display()
    );
    result
}

#[allow(clippy::too_many_arguments)]
fn converge_inner(
    arguments: &GitopsConvergeArgs,
    source: &ObjectRef,
    root: &ObjectRef,
    releases: &[ObjectRef],
    deployments: &[DeploymentRef],
    deadline: &Deadline,
    phases: &mut Vec<PhaseEvidence>,
) -> Result<()> {
    run_phase(phases, "source_fetch", || {
        request_reconciliation(arguments, GIT_REPOSITORY_RESOURCE, source)?;
        wait_for_resource::<GitRepository>(
            arguments,
            GIT_REPOSITORY_RESOURCE,
            source,
            deadline,
            |repository| repository_ready_at(repository, &arguments.revision),
        )
    })?;

    run_phase(phases, "desired_state_apply", || {
        request_reconciliation(arguments, KUSTOMIZATION_RESOURCE, root)?;
        wait_for_resource::<FluxKustomization>(
            arguments,
            KUSTOMIZATION_RESOURCE,
            root,
            deadline,
            |kustomization| kustomization_ready_at(kustomization, &arguments.revision),
        )
    })?;

    run_phase(phases, "helm_release", || {
        for release in releases {
            request_reconciliation(arguments, HELM_RELEASE_RESOURCE, release)?;
            wait_for_resource::<HelmRelease>(
                arguments,
                HELM_RELEASE_RESOURCE,
                release,
                deadline,
                helm_release_ready,
            )?;
        }
        Ok(())
    })?;

    run_phase(phases, "rollout", || {
        for deployment in deployments {
            run_kubectl(
                arguments,
                &[
                    "--namespace".into(),
                    deployment.namespace.clone(),
                    "rollout".into(),
                    "status".into(),
                    format!("deployment/{}", deployment.name),
                    "--watch=true".into(),
                    format!(
                        "--timeout={}s",
                        deadline.remaining("Deployment rollout")?.as_secs().max(1)
                    ),
                ],
                "watch Deployment rollout",
            )?;
        }
        Ok(())
    })?;

    run_phase(phases, "readiness", || {
        for deployment in deployments {
            run_kubectl(
                arguments,
                &[
                    "--namespace".into(),
                    deployment.namespace.clone(),
                    "wait".into(),
                    "--for=condition=Available".into(),
                    format!("deployment/{}", deployment.name),
                    format!(
                        "--timeout={}s",
                        deadline.remaining("Deployment readiness")?.as_secs().max(1)
                    ),
                ],
                "wait for Deployment readiness",
            )?;
        }
        wait_for_resource::<FluxKustomization>(
            arguments,
            KUSTOMIZATION_RESOURCE,
            root,
            deadline,
            |kustomization| kustomization_ready_at(kustomization, &arguments.revision),
        )?;
        for release in releases {
            wait_for_resource::<HelmRelease>(
                arguments,
                HELM_RELEASE_RESOURCE,
                release,
                deadline,
                helm_release_ready,
            )?;
        }
        Ok(())
    })
}

fn run_phase(
    phases: &mut Vec<PhaseEvidence>,
    name: &'static str,
    operation: impl FnOnce() -> Result<()>,
) -> Result<()> {
    println!("gitops phase {name}: started");
    let started = Instant::now();
    match operation() {
        Ok(()) => {
            let elapsed_millis = started.elapsed().as_millis();
            println!("gitops phase {name}: completed in {elapsed_millis} ms");
            phases.push(PhaseEvidence {
                phase: name,
                elapsed_millis,
                status: PhaseStatus::Succeeded,
                diagnostic: None,
            });
            Ok(())
        }
        Err(error) => {
            let elapsed_millis = started.elapsed().as_millis();
            let diagnostic = format!("{error:#}");
            println!("gitops phase {name}: failed in {elapsed_millis} ms: {diagnostic}");
            phases.push(PhaseEvidence {
                phase: name,
                elapsed_millis,
                status: PhaseStatus::Failed,
                diagnostic: Some(diagnostic),
            });
            Err(error)
        }
    }
}

fn request_reconciliation(
    arguments: &GitopsConvergeArgs,
    resource: &str,
    object: &ObjectRef,
) -> Result<()> {
    run_kubectl(
        arguments,
        &[
            "--namespace".into(),
            object.namespace.clone(),
            "annotate".into(),
            resource.into(),
            object.name.clone(),
            format!("reconcile.fluxcd.io/requestedAt={}", unix_millis()?),
            "--overwrite".into(),
        ],
        &format!(
            "request reconciliation for {resource} {}/{}",
            object.namespace, object.name
        ),
    )
}

fn wait_for_resource<T>(
    arguments: &GitopsConvergeArgs,
    resource: &str,
    object: &ObjectRef,
    deadline: &Deadline,
    predicate: impl Fn(&T) -> bool,
) -> Result<()>
where
    T: DeserializeOwned,
{
    let initial = get_resource(arguments, resource, object)?;
    if predicate(&initial) {
        return Ok(());
    }
    let remaining = deadline.remaining(&format!(
        "{resource} {}/{} observation",
        object.namespace, object.name
    ))?;
    let mut command = kubectl(arguments);
    command.args(resource_watch_arguments(resource, object, remaining));
    command.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut child = command
        .spawn()
        .with_context(|| format!("watching {resource} {}/{}", object.namespace, object.name))?;
    let stdout = child
        .stdout
        .take()
        .context("kubectl watch stdout is unavailable")?;
    let stream = serde_json::Deserializer::from_reader(BufReader::new(stdout)).into_iter::<Value>();
    for event in stream {
        let event = event.with_context(|| format!("decoding {resource} watch event"))?;
        if event.get("type").and_then(Value::as_str) == Some("ERROR") {
            bail!(
                "{resource} {}/{} watch returned an ERROR event: {event}",
                object.namespace,
                object.name
            );
        }
        let observed = event.get("object").unwrap_or(&event).clone();
        let observed: T = serde_json::from_value(observed).with_context(|| {
            format!(
                "decoding watched {resource} {}/{}",
                object.namespace, object.name
            )
        })?;
        if predicate(&observed) {
            child.kill().ok();
            child.wait().ok();
            return Ok(());
        }
    }
    let status = child.wait().context("waiting for kubectl resource watch")?;
    bail!(
        "{resource} {}/{} did not reach its required state before its watch ended with {status}",
        object.namespace,
        object.name
    )
}

fn resource_watch_arguments(resource: &str, object: &ObjectRef, timeout: Duration) -> Vec<String> {
    vec![
        "--namespace".into(),
        object.namespace.clone(),
        "get".into(),
        resource.into(),
        object.name.clone(),
        "--watch".into(),
        "--output-watch-events".into(),
        "--output=json".into(),
        format!("--request-timeout={}s", timeout.as_secs().max(1)),
    ]
}

fn get_resource<T>(arguments: &GitopsConvergeArgs, resource: &str, object: &ObjectRef) -> Result<T>
where
    T: DeserializeOwned,
{
    let output = kubectl(arguments)
        .args([
            "--namespace",
            &object.namespace,
            "get",
            resource,
            &object.name,
            "--output=json",
        ])
        .output()
        .with_context(|| format!("reading {resource} {}/{}", object.namespace, object.name))?;
    ensure!(
        output.status.success(),
        "read {resource} {}/{} failed with {}: {}",
        object.namespace,
        object.name,
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decoding {resource} {}/{}", object.namespace, object.name))
}

fn run_kubectl(
    arguments: &GitopsConvergeArgs,
    command_arguments: &[String],
    operation: &str,
) -> Result<()> {
    let status = kubectl(arguments)
        .args(command_arguments)
        .status()
        .with_context(|| operation.to_string())?;
    ensure!(status.success(), "{operation} failed with {status}");
    Ok(())
}

fn kubectl(arguments: &GitopsConvergeArgs) -> Command {
    let mut command = Command::new("kubectl");
    command.args(["--context", &arguments.context]);
    command
}

fn repository_ready_at(repository: &GitRepository, revision: &str) -> bool {
    generation_observed(
        repository.metadata.generation,
        repository.status.observed_generation,
    ) && ready(&repository.status.conditions)
        && repository
            .status
            .artifact
            .as_ref()
            .is_some_and(|artifact| revision_matches(&artifact.revision, revision))
}

fn kustomization_ready_at(kustomization: &FluxKustomization, revision: &str) -> bool {
    generation_observed(
        kustomization.metadata.generation,
        kustomization.status.observed_generation,
    ) && ready(&kustomization.status.conditions)
        && kustomization
            .status
            .last_applied_revision
            .as_deref()
            .is_some_and(|observed| revision_matches(observed, revision))
}

fn helm_release_ready(release: &HelmRelease) -> bool {
    generation_observed(
        release.metadata.generation,
        release.status.observed_generation,
    ) && ready(&release.status.conditions)
        && release
            .status
            .inventory
            .as_ref()
            .is_some_and(|inventory| !inventory.entries.is_empty())
}

fn generation_observed(generation: i64, observed_generation: Option<i64>) -> bool {
    generation > 0 && observed_generation == Some(generation)
}

fn ready(conditions: &[FluxCondition]) -> bool {
    conditions.iter().any(|condition| {
        condition.condition_type == "Ready" && condition.status.eq_ignore_ascii_case("true")
    })
}

fn revision_matches(observed: &str, expected: &str) -> bool {
    observed == expected
        || observed
            .strip_suffix(expected)
            .is_some_and(|prefix| prefix.ends_with(':') || prefix.ends_with('/'))
}

fn validate_revision(argument: &str, revision: &str) -> Result<()> {
    ensure!(
        matches!(revision.len(), 40 | 64) && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{argument} must be one complete 40- or 64-character hexadecimal Git object ID"
    );
    Ok(())
}

fn observe_release(
    arguments: &GitopsConvergeArgs,
    object: &ObjectRef,
) -> Result<ReleaseObservation> {
    let release = get_resource::<HelmRelease>(arguments, HELM_RELEASE_RESOURCE, object)?;
    Ok(ReleaseObservation {
        namespace: object.namespace.clone(),
        name: object.name.clone(),
        ready: helm_release_ready(&release),
        attempted_revision: release.status.last_attempted_revision,
        inventory_entries: release
            .status
            .inventory
            .map_or(0, |inventory| inventory.entries.len()),
    })
}

fn unix_millis() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock precedes the Unix epoch")?
        .as_millis())
}

fn write_evidence(path: &Path, evidence: &ConvergenceEvidence) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating evidence directory {}", parent.display()))?;
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating GitOps convergence evidence {}", path.display()))?;
    serde_json::to_writer_pretty(file, evidence)
        .with_context(|| format!("writing GitOps convergence evidence {}", path.display()))?;
    Ok(())
}

impl ObjectRef {
    fn parse(value: &str, label: &str) -> Result<Self> {
        let (namespace, name) = value
            .split_once('/')
            .with_context(|| format!("{label} `{value}` must use namespace/name"))?;
        ensure!(
            !namespace.is_empty() && !name.is_empty() && !name.contains('/'),
            "{label} `{value}` must use exactly one non-empty namespace/name pair"
        );
        Ok(Self {
            namespace: namespace.into(),
            name: name.into(),
        })
    }
}

impl DeploymentRef {
    fn parse(value: &str) -> Result<Self> {
        let object = ObjectRef::parse(value, "Deployment")?;
        Ok(Self {
            namespace: object.namespace,
            name: object.name,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn matches_exact_flux_revision_and_complete_helm_inventory() {
        let repository: GitRepository = serde_json::from_value(json!({
            "metadata": {"generation": 3},
            "status": {
                "observedGeneration": 3,
                "artifact": {"revision": format!("main@sha1:{REVISION}")},
                "conditions": [{"type": "Ready", "status": "True"}]
            }
        }))
        .unwrap();
        let release: HelmRelease = serde_json::from_value(json!({
            "metadata": {"generation": 2},
            "status": {
                "observedGeneration": 2,
                "lastAttemptedRevision": "0.1.0+sha256:abc",
                "inventory": {"entries": [{"id": "veoveo_gateway_apps_Deployment"}]},
                "conditions": [{"type": "Ready", "status": "True"}]
            }
        }))
        .unwrap();

        assert!(repository_ready_at(&repository, REVISION));
        assert!(helm_release_ready(&release));
    }

    #[test]
    fn rejects_stale_generation_wrong_revision_and_empty_inventory() {
        let kustomization: FluxKustomization = serde_json::from_value(json!({
            "metadata": {"generation": 4},
            "status": {
                "observedGeneration": 3,
                "lastAppliedRevision": "main@sha1:ffffffffffffffffffffffffffffffffffffffff",
                "conditions": [{"type": "Ready", "status": "True"}]
            }
        }))
        .unwrap();
        let release: HelmRelease = serde_json::from_value(json!({
            "metadata": {"generation": 1},
            "status": {
                "observedGeneration": 1,
                "inventory": {"entries": []},
                "conditions": [{"type": "Ready", "status": "True"}]
            }
        }))
        .unwrap();

        assert!(!kustomization_ready_at(&kustomization, REVISION));
        assert!(!helm_release_ready(&release));
    }

    #[test]
    fn parses_namespaced_references_and_requires_full_revision() {
        let deployment = DeploymentRef::parse("platform/console-bff").unwrap();
        assert_eq!(deployment.namespace, "platform");
        assert_eq!(deployment.name, "console-bff");
        assert!(DeploymentRef::parse("console-bff").is_err());
        assert!(validate_revision("--revision", REVISION).is_ok());
        assert!(validate_revision("--revision", "01234567").is_err());
    }

    #[test]
    fn resource_watch_lists_before_watching_with_portable_arguments() {
        let object = ObjectRef::parse("flux-system/bioma", "root").unwrap();
        let arguments =
            resource_watch_arguments(KUSTOMIZATION_RESOURCE, &object, Duration::from_secs(30));

        assert!(arguments.iter().any(|argument| argument == "--watch"));
        assert!(arguments.iter().all(|argument| argument != "--watch-only"));
        assert!(
            arguments
                .iter()
                .all(|argument| !argument.starts_with("--resource-version"))
        );
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--request-timeout=30s")
        );
    }
}
