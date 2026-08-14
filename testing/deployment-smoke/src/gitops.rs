use std::{
    fs::{self, OpenOptions},
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;

const EVIDENCE_SCHEMA: &str = "veoveo.io/gitops-convergence-evidence/v1";

#[derive(Debug)]
pub(crate) struct GitopsConvergeArgs {
    pub(crate) context: String,
    pub(crate) control_namespace: String,
    pub(crate) parent: String,
    pub(crate) children: Vec<String>,
    pub(crate) source_ref: String,
    pub(crate) parent_revision: String,
    pub(crate) configuration_revision: String,
    pub(crate) deployments: Vec<String>,
    pub(crate) timeout: Duration,
    pub(crate) evidence_output: PathBuf,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConvergenceEvidence<'a> {
    schema_version: &'static str,
    observed_at_unix_millis: u128,
    context: &'a str,
    control_namespace: &'a str,
    parent_application: &'a str,
    child_applications: &'a [String],
    source_ref: &'a str,
    expected_parent_revision: &'a str,
    expected_configuration_revision: &'a str,
    deployments: Vec<DeploymentRef>,
    timeout_millis: u128,
    elapsed_millis: u128,
    outcome: EvidenceOutcome,
    phases: Vec<PhaseEvidence>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentRef {
    namespace: String,
    name: String,
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
    validate_revision("--parent-revision", &arguments.parent_revision)?;
    validate_revision(
        "--configuration-revision",
        &arguments.configuration_revision,
    )?;
    ensure!(
        !arguments.children.is_empty(),
        "at least one --child application is required"
    );
    ensure!(
        !arguments.deployments.is_empty(),
        "at least one --deployment namespace/name is required"
    );
    ensure!(
        !arguments.timeout.is_zero(),
        "--timeout-seconds must be greater than zero"
    );
    let deployments = arguments
        .deployments
        .iter()
        .map(|value| DeploymentRef::parse(value))
        .collect::<Result<Vec<_>>>()?;
    let deadline = Deadline::new(arguments.timeout);
    let mut phases = Vec::new();

    let result = converge_inner(&arguments, &deployments, &deadline, &mut phases);
    let evidence = ConvergenceEvidence {
        schema_version: EVIDENCE_SCHEMA,
        observed_at_unix_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock precedes the Unix epoch")?
            .as_millis(),
        context: &arguments.context,
        control_namespace: &arguments.control_namespace,
        parent_application: &arguments.parent,
        child_applications: &arguments.children,
        source_ref: &arguments.source_ref,
        expected_parent_revision: &arguments.parent_revision,
        expected_configuration_revision: &arguments.configuration_revision,
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

fn converge_inner(
    arguments: &GitopsConvergeArgs,
    deployments: &[DeploymentRef],
    deadline: &Deadline,
    phases: &mut Vec<PhaseEvidence>,
) -> Result<()> {
    run_phase(phases, "parent_fetch", || {
        refresh_application(arguments, &arguments.parent)?;
        wait_for_application(arguments, &arguments.parent, deadline, |application| {
            status_contains_revision(application, &arguments.parent_revision)
        })
    })?;

    run_phase(phases, "child_render", || {
        for child in &arguments.children {
            wait_for_application(arguments, child, deadline, |application| {
                desired_source_matches(
                    application,
                    &arguments.source_ref,
                    &arguments.configuration_revision,
                )
            })?;
            refresh_application(arguments, child)?;
        }
        Ok(())
    })?;

    run_phase(phases, "apply", || {
        for child in &arguments.children {
            wait_for_application(arguments, child, deadline, |application| {
                application_applied(application, &arguments.configuration_revision)
            })?;
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
        for child in &arguments.children {
            wait_for_application(arguments, child, deadline, |application| {
                application_ready(application, &arguments.configuration_revision)
            })?;
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

fn refresh_application(arguments: &GitopsConvergeArgs, application: &str) -> Result<()> {
    run_kubectl(
        arguments,
        &[
            "--namespace".into(),
            arguments.control_namespace.clone(),
            "annotate".into(),
            "applications.argoproj.io".into(),
            application.into(),
            "argocd.argoproj.io/refresh=hard".into(),
            "--overwrite".into(),
        ],
        &format!("request hard refresh for Application {application}"),
    )
}

fn wait_for_application(
    arguments: &GitopsConvergeArgs,
    application: &str,
    deadline: &Deadline,
    predicate: impl Fn(&Value) -> bool,
) -> Result<()> {
    let initial = get_application(arguments, application)?;
    if predicate(&initial) {
        return Ok(());
    }
    let remaining = deadline.remaining(&format!("Application {application} observation"))?;
    let mut command = kubectl(arguments);
    command.args(application_watch_arguments(
        &arguments.control_namespace,
        application,
        remaining,
    ));
    command.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut child = command
        .spawn()
        .with_context(|| format!("watching Application {application}"))?;
    let stdout = child
        .stdout
        .take()
        .context("kubectl Application watch stdout is unavailable")?;
    let stream = serde_json::Deserializer::from_reader(BufReader::new(stdout)).into_iter::<Value>();
    for event in stream {
        let event = event.with_context(|| format!("decoding Application {application} watch"))?;
        if event.get("type").and_then(Value::as_str) == Some("ERROR") {
            bail!("Application {application} watch returned an ERROR event: {event}");
        }
        let observed = event.get("object").unwrap_or(&event);
        if predicate(observed) {
            child.kill().ok();
            child.wait().ok();
            return Ok(());
        }
    }
    let status = child
        .wait()
        .with_context(|| format!("waiting for Application {application} watch"))?;
    bail!(
        "Application {application} did not reach its required state before its watch ended with {status}"
    )
}

fn application_watch_arguments(
    namespace: &str,
    application: &str,
    timeout: Duration,
) -> Vec<String> {
    vec![
        "--namespace".into(),
        namespace.into(),
        "get".into(),
        "applications.argoproj.io".into(),
        application.into(),
        "--watch".into(),
        "--output-watch-events".into(),
        "--output=json".into(),
        format!("--request-timeout={}s", timeout.as_secs().max(1)),
    ]
}

fn get_application(arguments: &GitopsConvergeArgs, application: &str) -> Result<Value> {
    let output = kubectl(arguments)
        .args([
            "--namespace",
            &arguments.control_namespace,
            "get",
            "applications.argoproj.io",
            application,
            "--output=json",
        ])
        .output()
        .with_context(|| format!("reading Application {application}"))?;
    ensure!(
        output.status.success(),
        "read Application {application} failed with {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decoding Application {application}"))
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

fn desired_source_matches(application: &Value, source_ref: &str, revision: &str) -> bool {
    application
        .pointer("/spec/sources")
        .and_then(Value::as_array)
        .is_some_and(|sources| {
            sources.iter().any(|source| {
                source.get("ref").and_then(Value::as_str) == Some(source_ref)
                    && source.get("targetRevision").and_then(Value::as_str) == Some(revision)
            })
        })
}

fn status_contains_revision(application: &Value, revision: &str) -> bool {
    application
        .pointer("/status/sync/revision")
        .and_then(Value::as_str)
        == Some(revision)
        || application
            .pointer("/status/sync/revisions")
            .and_then(Value::as_array)
            .is_some_and(|revisions| {
                revisions
                    .iter()
                    .any(|value| value.as_str() == Some(revision))
            })
}

fn sync_result_contains_revision(application: &Value, revision: &str) -> bool {
    application
        .pointer("/status/operationState/syncResult/revision")
        .and_then(Value::as_str)
        == Some(revision)
        || application
            .pointer("/status/operationState/syncResult/revisions")
            .and_then(Value::as_array)
            .is_some_and(|revisions| {
                revisions
                    .iter()
                    .any(|value| value.as_str() == Some(revision))
            })
}

fn application_applied(application: &Value, revision: &str) -> bool {
    status_contains_revision(application, revision)
        && sync_result_contains_revision(application, revision)
        && application
            .pointer("/status/sync/status")
            .and_then(Value::as_str)
            == Some("Synced")
        && application
            .pointer("/status/operationState/phase")
            .and_then(Value::as_str)
            == Some("Succeeded")
}

fn application_ready(application: &Value, revision: &str) -> bool {
    application_applied(application, revision)
        && application
            .pointer("/status/health/status")
            .and_then(Value::as_str)
            == Some("Healthy")
}

fn validate_revision(argument: &str, revision: &str) -> Result<()> {
    ensure!(
        matches!(revision.len(), 40 | 64) && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{argument} must be one complete 40- or 64-character hexadecimal Git object ID"
    );
    Ok(())
}

impl DeploymentRef {
    fn parse(value: &str) -> Result<Self> {
        let (namespace, name) = value
            .split_once('/')
            .with_context(|| format!("Deployment `{value}` must use namespace/name"))?;
        ensure!(
            !namespace.is_empty() && !name.is_empty() && !name.contains('/'),
            "Deployment `{value}` must use exactly one non-empty namespace/name pair"
        );
        Ok(Self {
            namespace: namespace.into(),
            name: name.into(),
        })
    }
}

fn write_evidence(path: &Path, evidence: &ConvergenceEvidence<'_>) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn matches_named_desired_source_and_exact_applied_revision() {
        let application = json!({
            "spec": {"sources": [
                {"chart": "platform", "targetRevision": "1.2.3"},
                {"ref": "configuration", "targetRevision": REVISION}
            ]},
            "status": {
                "sync": {"status": "Synced", "revisions": ["1.2.3", REVISION]},
                "health": {"status": "Healthy"},
                "operationState": {
                    "phase": "Succeeded",
                    "syncResult": {"revisions": ["1.2.3", REVISION]}
                }
            }
        });
        assert!(desired_source_matches(
            &application,
            "configuration",
            REVISION
        ));
        assert!(application_applied(&application, REVISION));
        assert!(application_ready(&application, REVISION));
    }

    #[test]
    fn rejects_stale_operation_and_unhealthy_application() {
        let application = json!({
            "status": {
                "sync": {"status": "Synced", "revision": REVISION},
                "health": {"status": "Degraded"},
                "operationState": {
                    "phase": "Succeeded",
                    "syncResult": {"revision": "ffffffffffffffffffffffffffffffffffffffff"}
                }
            }
        });
        assert!(!application_applied(&application, REVISION));
        assert!(!application_ready(&application, REVISION));
    }

    #[test]
    fn parses_deployment_reference_and_requires_full_revision() {
        let deployment = DeploymentRef::parse("platform/console-bff").unwrap();
        assert_eq!(deployment.namespace, "platform");
        assert_eq!(deployment.name, "console-bff");
        assert!(DeploymentRef::parse("console-bff").is_err());
        assert!(validate_revision("--revision", REVISION).is_ok());
        assert!(validate_revision("--revision", "01234567").is_err());
    }

    #[test]
    fn application_watch_lists_before_watching_with_portable_arguments() {
        let arguments = application_watch_arguments("argocd", "bioma", Duration::from_secs(30));

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
