//! Process-level command and evidence acceptance. The test executable also serves
//! as a deterministic kubectl fixture, keeping every subprocess in Rust.
use std::{
    env,
    ffi::OsStr,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: String,
    reconciliation_mode: String,
    started_at_unix_millis: u128,
    observed_at_unix_millis: u128,
    expected_revision: String,
    outcome: String,
    phases: Vec<Phase>,
}

#[derive(Debug, Deserialize)]
struct Phase {
    phase: String,
    status: String,
    diagnostic: Option<String>,
}

fn main() -> Result<()> {
    if env::args_os()
        .next()
        .is_some_and(|path| Path::new(&path).file_name() == Some(OsStr::new("kubectl")))
    {
        return kubectl_fixture();
    }

    let revision = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    ensure!(revision.status.success(), "resolve fixture source revision");
    let revision = String::from_utf8(revision.stdout)?.trim().to_owned();
    for (scenario, mode, succeeds) in [
        ("ready", None, true),
        ("source-watch", Some("observe"), true),
        ("ready", Some("request"), true),
        ("wrong-revision", None, false),
        ("stale-generation", Some("observe"), false),
        ("stalled-release", Some("observe"), false),
    ] {
        run_case(&revision, scenario, mode, succeeds)?;
    }
    println!("6 GitOps command/evidence scenarios passed");
    Ok(())
}

fn run_case(revision: &str, scenario: &str, mode: Option<&str>, succeeds: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let executable = directory.path().join("kubectl");
    std::os::unix::fs::symlink(env::current_exe()?, &executable)?;
    let mut paths = vec![directory.path().to_path_buf()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let evidence_path = directory.path().join("evidence.json");
    let mut command = Command::new(env!("CARGO_BIN_EXE_deployment-smoke"));
    command
        .env("PATH", env::join_paths(paths)?)
        .env("VEOVEO_GITOPS_FIXTURE_ROOT", directory.path())
        .env("VEOVEO_GITOPS_FIXTURE_REVISION", revision)
        .env("VEOVEO_GITOPS_FIXTURE_SCENARIO", scenario)
        .env("VEOVEO_GITOPS_FIXTURE_MODE", mode.unwrap_or("observe"))
        .args([
            "gitops-converge",
            "--context",
            "fixture",
            "--source",
            "flux-system/source",
            "--root",
            "flux-system/root",
            "--release",
            "platform/platform",
            "--release",
            "extension/extension",
            "--deployment",
            "platform/gateway",
            "--deployment",
            "extension/simulator",
            "--revision",
            revision,
            "--timeout-seconds",
            "5",
            "--evidence-output",
        ])
        .arg(&evidence_path);
    if let Some(mode) = mode {
        command.args(["--reconciliation", mode]);
    }
    let output = command.output()?;
    ensure!(
        output.status.success() == succeeds,
        "{scenario} {mode:?}: unexpected status {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence: Evidence = serde_json::from_slice(&fs::read(evidence_path)?)?;
    ensure!(evidence.schema_version == "veoveo.io/gitops-convergence-evidence/v3");
    ensure!(evidence.reconciliation_mode == mode.unwrap_or("observe"));
    ensure!(evidence.expected_revision == revision);
    ensure!(evidence.started_at_unix_millis > 0);
    ensure!(evidence.observed_at_unix_millis >= evidence.started_at_unix_millis);
    ensure!(evidence.outcome == if succeeds { "succeeded" } else { "failed" });
    let calls = fs::read_to_string(directory.path().join("calls.jsonl"))?
        .lines()
        .map(serde_json::from_str::<Vec<String>>)
        .collect::<Result<Vec<_>, _>>()?;
    let annotations = calls
        .iter()
        .filter(|args| args[4] == "annotate")
        .collect::<Vec<_>>();
    if mode == Some("request") {
        ensure!(
            annotations.len() == 4,
            "request every declared Flux owner exactly once"
        );
        let objects = annotations
            .iter()
            .map(|args| (args[3].as_str(), args[6].as_str()))
            .collect::<Vec<_>>();
        ensure!(
            objects
                == [
                    ("flux-system", "source"),
                    ("flux-system", "root"),
                    ("platform", "platform"),
                    ("extension", "extension")
                ]
        );
    } else {
        ensure!(
            annotations.is_empty(),
            "observation must not request reconciliation"
        );
        ensure!(
            calls
                .iter()
                .all(|args| matches!(args[4].as_str(), "get" | "rollout" | "wait"))
        );
    }
    if succeeds {
        let phases = evidence
            .phases
            .iter()
            .map(|phase| phase.phase.as_str())
            .collect::<Vec<_>>();
        ensure!(
            phases
                == [
                    "source_fetch",
                    "desired_state_apply",
                    "helm_release",
                    "rollout",
                    "readiness"
                ]
        );
        ensure!(
            evidence
                .phases
                .iter()
                .all(|phase| phase.status == "succeeded")
        );
        for name in ["deployment/gateway", "deployment/simulator"] {
            ensure!(
                calls
                    .iter()
                    .any(|args| args[4] == "rollout" && args[5] == "status" && args[6] == name)
            );
            ensure!(
                calls
                    .iter()
                    .any(|args| args[4] == "wait" && args[6] == name)
            );
        }
    } else {
        let failed = evidence.phases.last().context("missing failed phase")?;
        ensure!(failed.status == "failed");
        ensure!(
            failed
                .diagnostic
                .as_ref()
                .is_some_and(|message| !message.is_empty())
        );
        ensure!(
            !calls
                .iter()
                .any(|args| matches!(args[4].as_str(), "rollout" | "wait"))
        );
    }
    if matches!(
        scenario,
        "source-watch" | "wrong-revision" | "stale-generation"
    ) {
        ensure!(
            calls
                .iter()
                .any(|args| args.iter().any(|arg| arg == "--watch"))
        );
    }
    Ok(())
}

fn kubectl_fixture() -> Result<()> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        arguments.len() >= 7,
        "unexpected kubectl arguments {arguments:?}"
    );
    ensure!(arguments[0..3] == ["--context", "fixture", "--namespace"]);
    let directory =
        PathBuf::from(env::var_os("VEOVEO_GITOPS_FIXTURE_ROOT").context("fixture root")?);
    let mut calls = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("calls.jsonl"))?;
    writeln!(calls, "{}", serde_json::to_string(&arguments)?)?;
    match arguments[4].as_str() {
        "annotate" => {
            ensure!(
                env::var("VEOVEO_GITOPS_FIXTURE_MODE")? == "request",
                "unexpected mutation in observation mode"
            );
            ensure!(arguments[7].starts_with("reconcile.fluxcd.io/requestedAt="));
            ensure!(arguments[8] == "--overwrite");
        }
        "rollout" => ensure!(arguments[5] == "status" && arguments[7] == "--watch=true"),
        "wait" => ensure!(arguments[5] == "--for=condition=Available"),
        "get" => {
            let scenario = env::var("VEOVEO_GITOPS_FIXTURE_SCENARIO")?;
            let revision = env::var("VEOVEO_GITOPS_FIXTURE_REVISION")?;
            let watching = arguments.iter().any(|argument| argument == "--watch");
            let mut resource = resource_fixture(&arguments[5], &scenario, &revision, watching)?;
            if watching {
                resource = json!({"type": "MODIFIED", "object": resource});
            }
            println!("{}", serde_json::to_string(&resource)?);
        }
        verb => bail!("unexpected Kubernetes command {verb}"),
    }
    Ok(())
}

fn resource_fixture(kind: &str, scenario: &str, revision: &str, watching: bool) -> Result<Value> {
    let conditions = json!([{"type": "Ready", "status": "True"}]);
    let mut resource = json!({"metadata": {"generation": 1}, "status": {"observedGeneration": 1, "conditions": conditions}});
    match kind {
        "gitrepositories.source.toolkit.fluxcd.io" => {
            let observed =
                if scenario == "wrong-revision" || (scenario == "source-watch" && !watching) {
                    "ffffffffffffffffffffffffffffffffffffffff"
                } else {
                    revision
                };
            resource["status"]["artifact"] = json!({"revision": format!("main@sha1:{observed}")});
            if scenario == "stale-generation" {
                resource["metadata"]["generation"] = json!(2);
            }
        }
        "kustomizations.kustomize.toolkit.fluxcd.io" => {
            resource["status"]["lastAppliedRevision"] = json!(format!("main@sha1:{revision}"));
            if scenario == "stalled-release" {
                resource["status"]["conditions"] = json!([{"type": "Ready", "status": "False"}]);
            }
        }
        "helmreleases.helm.toolkit.fluxcd.io" => {
            resource["status"]["lastAttemptedRevision"] = json!("0.1.0-fixture");
            resource["status"]["inventory"] =
                json!({"entries": [{"id": "fixture_apps_Deployment"}]});
            if scenario == "stalled-release" {
                resource["status"]["conditions"] = json!([{"type": "Stalled", "status": "True", "reason": "RetriesExceeded", "message": "fixture failure"}]);
            }
        }
        kind => bail!("unexpected resource {kind}"),
    }
    Ok(resource)
}
