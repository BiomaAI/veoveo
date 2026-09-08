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

fn command(chart: &Path, release: &str, settings: &[&str]) -> Command {
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
    for setting in settings {
        command.args(["--set-string", setting]);
    }
    command
}

fn render(chart: &Path, release: &str, settings: &[&str]) -> Result<BTreeMap<String, Job>> {
    let command = command(chart, release, settings);
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
            "installation-bootstrap" => "veoveo.ai/bootstrap-revision",
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
                component.into(),
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
    ensure!(
        jobs.len() == 2,
        "platform render must contain both initialization Jobs"
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
        for template in ["platform-bootstrap.yaml", "object-store-init.yaml"] {
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

    for (setting, expected) in [
        (
            format!(
                "global.imageDigests.veoveo/console-bff=sha256:{}",
                "a".repeat(64)
            ),
            Vec::new(),
        ),
        (
            "objectStore.rustfs.bucket=changed-artifact-bucket".into(),
            vec!["object-store-init"],
        ),
        (
            "surrealdb.database=changed-database".into(),
            vec!["installation-bootstrap"],
        ),
        (
            "gateway.resources.limits.memory=2Gi".into(),
            vec!["installation-bootstrap"],
        ),
        (
            format!("gateway.controlPlaneRevision={}", "b".repeat(64)),
            vec!["installation-bootstrap"],
        ),
        (
            "serviceAccount.name=changed-service-account".into(),
            vec!["installation-bootstrap", "object-store-init"],
        ),
    ] {
        let after = render(chart, "bioma", &[&setting])?;
        let changed = before
            .iter()
            .filter_map(|(component, job)| (job != &after[component]).then_some(component.as_str()))
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
        "Helm initialization Jobs use stable complete input revisions; gateway rendering requires its explicit bundle digest."
    );
    Ok(())
}
