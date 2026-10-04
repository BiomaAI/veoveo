//! Stable Job identities across release metadata, with complete immutable spec inputs.

use std::{collections::BTreeMap, fs, path::Path, process::Command};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;

use super::commands::{copy_fixture, run_checked};

#[derive(Debug, PartialEq)]
struct Job {
    name: String,
    revision: String,
    spec: Value,
}

fn command_with_plan(chart: &Path, release: &str, settings: &[&str], plan: &Path) -> Command {
    let mut command = Command::new("helm");
    command.args(["template", release]).arg(chart).args([
        "--namespace",
        "veoveo",
        "--values",
        "examples/bioma/values.yaml",
        "--values",
        "examples/bioma/k3d-values.yaml",
        "--values",
        "examples/bioma/images/veoveo.lock.yaml",
    ]);
    command.args([
        "--set-file",
        &format!("moduleInstallation.planJson={}", plan.display()),
    ]);
    for setting in settings {
        command.args(["--set-string", setting]);
    }
    command
}

fn command(chart: &Path, release: &str, settings: &[&str]) -> Command {
    command_with_plan(
        chart,
        release,
        settings,
        Path::new("testing/fixtures/module-schema-consumer/module-plan.json"),
    )
}

fn render(chart: &Path, release: &str, settings: &[&str]) -> Result<BTreeMap<String, Job>> {
    render_with_plan(
        chart,
        release,
        settings,
        Path::new("testing/fixtures/module-schema-consumer/module-plan.json"),
    )
}
fn render_with_plan(
    chart: &Path,
    release: &str,
    settings: &[&str],
    plan: &Path,
) -> Result<BTreeMap<String, Job>> {
    let command = command_with_plan(chart, release, settings, plan);
    let rendered = run_checked(Path::new("helm"), command.get_args().map(Into::into), [])?;
    let mut jobs = BTreeMap::new();
    for document in serde_yaml_ng::Deserializer::from_str(&rendered) {
        let value = Value::deserialize(document)?;
        if value["kind"] != "Job" {
            continue;
        }
        let component = value["metadata"]["labels"]["app.kubernetes.io/component"]
            .as_str()
            .context("Job has no component label")?;
        let annotation = match component {
            "installation-prepare" | "module-migration" | "control-plane-publication" => {
                "veoveo.ai/module-job-revision"
            }
            "object-store-init" => "veoveo.ai/object-init-revision",
            _ => anyhow::bail!("unexpected platform Job component {component}"),
        };
        let name = value["metadata"]["name"]
            .as_str()
            .context("Job has no name")?
            .to_owned();
        let revision = value["metadata"]["annotations"][annotation]
            .as_str()
            .context("Job has no complete input revision")?
            .to_owned();
        ensure!(
            revision.len() == 64 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "invalid Job input revision"
        );
        ensure!(
            name.len() <= 63 && name.ends_with(&revision[..12]),
            "Job name must preserve its digest suffix within the Kubernetes length limit"
        );
        ensure!(
            jobs.insert(
                if component == "module-migration" {
                    format!(
                        "module:{}",
                        value["metadata"]["labels"]["veoveo.ai/module"]
                            .as_str()
                            .context("lane Job has no module label")?
                    )
                } else {
                    component.into()
                },
                Job {
                    name,
                    revision,
                    spec: value["spec"].clone()
                }
            )
            .is_none(),
            "duplicate Job component"
        );
    }
    let plan: veoveo_modules::ModulePlanDocument = serde_json::from_slice(&fs::read(plan)?)?;
    let expected: std::collections::BTreeSet<String> = plan
        .lanes()
        .iter()
        .map(|lane| format!("module:{}", lane.module))
        .chain(
            [
                "installation-prepare",
                "control-plane-publication",
                "object-store-init",
            ]
            .map(str::to_owned),
        )
        .collect();
    ensure!(
        jobs.keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            == expected,
        "rendered Jobs differ from the actual generated composition plan"
    );
    Ok(jobs)
}

pub(super) fn check() -> Result<()> {
    let chart = Path::new("deploy/helm/veoveo");
    let before = render(chart, "bioma", &[])?;
    let temporary = tempfile::tempdir()?;
    for revision in [2, 29] {
        let variant = temporary.path().join(format!("revision-{revision}"));
        copy_fixture(chart, &variant)?;
        let chart_path = variant.join("Chart.yaml");
        let mut metadata: Value = serde_yaml_ng::from_slice(&fs::read(&chart_path)?)?;
        metadata["version"] = format!("0.1.0-job-check.{revision}").into();
        metadata["appVersion"] = format!("source-{revision}").into();
        fs::write(&chart_path, serde_yaml_ng::to_string(&metadata)?)?;
        // The CLI does not expose Release.Revision. Set the actual Helm engine
        // context in the disposable fixture before evaluating each Job template.
        for template in [
            "platform-bootstrap.yaml",
            "module-migrations.yaml",
            "object-store-init.yaml",
        ] {
            let path = variant.join("templates").join(template);
            let source = fs::read_to_string(&path)?;
            fs::write(
                path,
                format!("{{{{- $_ := set .Release \"Revision\" {revision} -}}}}\n{source}"),
            )?;
        }
        ensure!(
            before == render(&variant, "bioma", &[])?,
            "release or chart metadata changed an initialization Job"
        );
    }

    for (setting, expected_kind) in [
        (
            format!(
                "global.imageDigests.veoveo/console-bff=sha256:{}",
                "a".repeat(64)
            ),
            "none",
        ),
        (
            "objectStore.rustfs.bucket=changed-artifact-bucket".into(),
            "object-store-init",
        ),
        ("surrealdb.database=changed-database".into(), "module-jobs"),
        ("gateway.resources.limits.memory=2Gi".into(), "module-jobs"),
        (
            format!("gateway.controlPlaneRevision={}", "b".repeat(64)),
            "control-plane-publication",
        ),
        ("serviceAccount.name=changed-service-account".into(), "all"),
    ] {
        let after = render(chart, "bioma", &[&setting])?;
        let changed = before
            .iter()
            .filter_map(|(component, job)| (job != &after[component]).then_some(component.as_str()))
            .collect::<Vec<_>>();
        let expected = before
            .keys()
            .filter(|key| match expected_kind {
                "none" => false,
                "all" => true,
                "module-jobs" => key.as_str() != "object-store-init",
                kind => key.as_str() == kind,
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        ensure!(
            changed == expected,
            "Job changes for {setting} were {changed:?}, expected {expected:?}"
        );
        for component in expected {
            ensure!(
                before[component].name != after[component].name,
                "changed immutable Job spec retained its old name"
            );
        }
    }
    credential_rotation(chart, &before, temporary.path())?;
    let long_first = format!("{}a", "v".repeat(52));
    let long_second = format!("{}b", "v".repeat(52));
    let first = render(chart, &long_first, &[])?;
    let second = render(chart, &long_second, &[])?;
    for (component, job) in first {
        ensure!(
            job.name != second[&component].name,
            "long release names collide after truncation"
        );
    }

    for revision in ["", "invalid"] {
        let output = command(
            chart,
            "bioma",
            &[&format!("gateway.controlPlaneRevision={revision}")],
        )
        .output()?;
        ensure!(
            !output.status.success(),
            "gateway rendering accepted an absent or malformed control-plane revision"
        );
        ensure!(
            String::from_utf8_lossy(&output.stderr).contains("controlPlaneRevision"),
            "render failed outside the expected revision gate"
        );
    }
    println!(
        "Helm renders the generated lane catalog, preserves no-op Job identities, and confines migration credentials to preparation/lane Jobs."
    );
    Ok(())
}

fn documents(chart: &Path, plan: &Path) -> Result<Vec<Value>> {
    let command = command_with_plan(chart, "bioma", &[], plan);
    let rendered = run_checked(Path::new("helm"), command.get_args().map(Into::into), [])?;
    serde_yaml_ng::Deserializer::from_str(&rendered)
        .map(|document| Value::deserialize(document).map_err(Into::into))
        .collect()
}

fn references_secret(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields
                .get("secretKeyRef")
                .is_some_and(|reference| reference["name"] == name)
                || fields
                    .get("secretRef")
                    .is_some_and(|reference| reference["name"] == name)
                || fields
                    .get("secret")
                    .is_some_and(|reference| reference["secretName"] == name)
                || fields.values().any(|value| references_secret(value, name))
        }
        Value::Array(values) => values.iter().any(|value| references_secret(value, name)),
        _ => false,
    }
}

fn client_templates(documents: &[Value]) -> Result<BTreeMap<String, Value>> {
    let mut clients = BTreeMap::new();
    for document in documents {
        let kind = document["kind"].as_str().unwrap_or("");
        if !matches!(kind, "Deployment" | "StatefulSet" | "DaemonSet" | "Job") {
            continue;
        }
        let template = &document["spec"]["template"];
        let spec = &template["spec"];
        let component = document["metadata"]["labels"]["app.kubernetes.io/component"]
            .as_str()
            .or_else(|| template["metadata"]["labels"]["app.kubernetes.io/component"].as_str())
            .unwrap_or("");
        if references_secret(spec, "veoveo-surreal-admin") {
            ensure!(
                (kind == "Job" && matches!(component, "installation-prepare" | "module-migration"))
                    || (kind == "StatefulSet" && component == "surrealdb"),
                "migration credential escaped its Job or database provisioning pod"
            );
        }
        if references_secret(spec, "veoveo-surreal-runtime") {
            let key = format!(
                "{kind}/{}/{}",
                document["metadata"]["namespace"]
                    .as_str()
                    .unwrap_or("veoveo"),
                document["metadata"]["name"]
                    .as_str()
                    .context("database client has no name")?
            );
            clients.insert(key, template.clone());
        }
        for container in spec["containers"].as_array().into_iter().flatten() {
            let env = container["env"].as_array();
            if env.is_some_and(|env| {
                env.iter()
                    .any(|value| value["name"] == "VEOVEO_MODULE_PLAN")
            }) {
                ensure!(
                    env.unwrap()
                        .iter()
                        .any(|value| value["name"] == "VEOVEO_SURREAL_RUNTIME_USERNAME"),
                    "PlanArgs consumer omitted expected runtime username"
                );
            }
        }
    }
    Ok(clients)
}

fn credential_rotation(
    chart: &Path,
    before_jobs: &BTreeMap<String, Job>,
    directory: &Path,
) -> Result<()> {
    let original = Path::new("testing/fixtures/module-schema-consumer/module-plan.json");
    let mut next: Value = serde_json::from_slice(&fs::read(original)?)?;
    next["generation"] = "2".into();
    next["credentialRevision"] = "fixture-rotated".into();
    let _: veoveo_modules::ModulePlanDocument = serde_json::from_value(next.clone())?;
    let path = directory.join("credential-plan.json");
    fs::write(&path, serde_json::to_vec(&next)?)?;
    let after_jobs = render_with_plan(chart, "bioma", &[], &path)?;
    for (key, job) in before_jobs {
        if key == "object-store-init" {
            ensure!(
                job == &after_jobs[key],
                "unrelated object-store Job changed during database rotation"
            );
        } else {
            ensure!(
                job.name != after_jobs[key].name,
                "database rotation retained stale Job identity"
            );
        }
    }
    let before = client_templates(&documents(chart, original)?)?;
    let after = client_templates(&documents(chart, &path)?)?;
    for (key, template) in before.iter().filter(|(key, _)| !key.starts_with("Job/")) {
        let next = after
            .get(key)
            .context("rotation lost an existing database client")?;
        ensure!(
            template["metadata"]["annotations"]["veoveo.ai/database-credential-revision"]
                != next["metadata"]["annotations"]["veoveo.ai/database-credential-revision"],
            "credential rotation did not update database client {key}"
        );
        ensure!(
            next["metadata"]["annotations"]["veoveo.ai/database-credential-revision"]
                == "fixture-rotated",
            "database client has an incorrect credential revision"
        );
    }
    ensure!(
        !before.is_empty(),
        "credential fixture rendered no actual database clients"
    );
    Ok(())
}
