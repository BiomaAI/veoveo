use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    Args,
    observe::{Resource, Snapshot},
};

const CHART: &str = "veoveo-cancel-witness";

#[derive(Debug, Clone, Serialize)]
pub(super) struct Publications {
    pub initial: String,
    pub broken: String,
    pub fixed: String,
    chart: String,
}

pub(super) struct Fixture<'a> {
    args: &'a Args,
    namespace: String,
    directory: tempfile::TempDir,
}

pub(super) fn validate_inputs(args: &Args) -> Result<()> {
    for registry in [&args.push_registry, &args.pull_registry] {
        ensure!(
            !registry.is_empty()
                && registry
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".:-[]".contains(&b)),
            "registry must be a host and optional port"
        );
    }
    let (repository, digest) = args
        .image
        .split_once("@sha256:")
        .context("witness image must select a SHA-256 digest")?;
    ensure!(
        !repository.is_empty()
            && repository
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"./:_-".contains(&b)),
        "invalid witness image repository"
    );
    validate_digest(&format!("sha256:{digest}"))?;
    Ok(())
}

fn validate_digest(digest: &str) -> Result<()> {
    let hash = digest
        .strip_prefix("sha256:")
        .context("expected SHA-256 manifest")?;
    ensure!(
        hash.len() == 64
            && hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid SHA-256 manifest"
    );
    Ok(())
}

pub(super) fn tool_versions() -> Result<(String, String)> {
    let flux = run(Command::new("flux").args(["version", "--client"]))?;
    let helm = run(Command::new("helm").args(["version", "--short"]))?;
    ensure!(
        flux.trim() == "flux: v2.9.5",
        "cancellation verification requires Flux CLI 2.9.5"
    );
    ensure!(
        helm.trim().starts_with("v4.2.4+"),
        "cancellation verification requires Helm 4.2.4"
    );
    Ok((flux.trim().into(), helm.trim().into()))
}

impl<'a> Fixture<'a> {
    pub fn new(args: &'a Args, namespace: String) -> Result<Self> {
        Ok(Self {
            args,
            namespace,
            directory: tempfile::tempdir()?,
        })
    }

    fn kubectl(&self) -> Command {
        let mut command = Command::new("kubectl");
        command.args([
            "--context",
            &self.args.context,
            "--namespace",
            &self.namespace,
            "--request-timeout=10s",
        ]);
        command
    }

    pub fn create_namespace(&self) -> Result<()> {
        run(self
            .kubectl()
            .args(["create", "namespace", &self.namespace]))?;
        Ok(())
    }

    pub fn cleanup(&self) -> Result<()> {
        println!("Flux cancellation: removing fixture {}", self.namespace);
        // Removing the root lets Flux uninstall its Helm release before the namespace.
        // A missing root is valid when publication/bootstrap failed before creation.
        let root = run(self.kubectl().args([
            "delete",
            "kustomization",
            "configuration",
            "--ignore-not-found",
            "--wait=true",
            "--timeout=60s",
        ]));
        let namespace = run(self.kubectl().args([
            "delete",
            "namespace",
            &self.namespace,
            "--wait=true",
            "--timeout=60s",
        ]));
        root?;
        namespace?;
        Ok(())
    }

    fn apply(&self, name: &str, yaml: &str) -> Result<()> {
        let path = self.directory.path().join(name);
        fs::write(&path, yaml)?;
        run(self.kubectl().args(["apply", "-f"]).arg(path))?;
        Ok(())
    }

    pub fn read(&self, kind: &str, name: &str) -> Result<Resource> {
        let output =
            run(self
                .kubectl()
                .args(["get", kind, name, "--ignore-not-found", "-o", "json"]))?;
        if output.trim().is_empty() {
            return Ok(Resource::default());
        }
        serde_json::from_str(&output).with_context(|| format!("decoding {kind}/{name}"))
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        Snapshot::read(self)
    }

    pub fn publish(&self) -> Result<Publications> {
        let chart_path = self.directory.path().join(CHART);
        fs::create_dir_all(chart_path.join("templates"))?;
        fs::write(
            chart_path.join("Chart.yaml"),
            format!("apiVersion: v2\nname: {CHART}\nversion: 0.1.0\n"),
        )?;
        fs::write(
            chart_path.join("templates/deployment.yaml"),
            include_str!("witness.yaml"),
        )?;
        run(Command::new("helm")
            .arg("package")
            .arg(&chart_path)
            .arg("--destination")
            .arg(self.directory.path()))?;
        let repository = format!("veoveo-smoke/{}/charts", self.namespace);
        let mut push = Command::new("helm");
        push.arg("push")
            .arg(self.directory.path().join(format!("{CHART}-0.1.0.tgz")))
            .arg(format!("oci://{}/{repository}", self.args.push_registry));
        if self.args.registry_transport.is_insecure() {
            push.arg("--plain-http");
        }
        let output = run(&mut push)?;
        let chart = output
            .lines()
            .find_map(|line| line.strip_prefix("Digest: "))
            .context("Helm push did not report the chart digest")?
            .trim()
            .to_owned();
        validate_digest(&chart)?;
        let initial = self.publish_configuration("initial", true, &chart)?;
        let broken = self.publish_configuration("broken", false, &chart)?;
        let fixed = self.publish_configuration("fixed", true, &chart)?;
        ensure!(
            initial != broken && broken != fixed && initial != fixed,
            "fixture revisions must be distinct"
        );
        Ok(Publications {
            initial,
            broken,
            fixed,
            chart,
        })
    }

    fn publish_configuration(&self, phase: &str, ready: bool, chart: &str) -> Result<String> {
        let directory = self.directory.path().join(phase);
        fs::create_dir_all(&directory)?;
        fs::write(
            directory.join("kustomization.yaml"),
            "apiVersion: kustomize.config.k8s.io/v1beta1\nkind: Kustomization\nresources:\n- release.yaml\n",
        )?;
        fs::write(
            directory.join("release.yaml"),
            format!(
                r#"apiVersion: helm.toolkit.fluxcd.io/v2
kind: HelmRelease
metadata:
  name: witness
  namespace: {namespace}
spec:
  interval: 1m
  timeout: 5m
  releaseName: witness
  chartRef:
    kind: OCIRepository
    name: chart
  install:
    createNamespace: false
    remediation:
      retries: 5
  upgrade:
    remediation:
      retries: 5
      strategy: rollback
  values:
    image: {image:?}
    ready: {ready}
    phase: {phase:?}
    chartDigest: {chart:?}
"#,
                image = self.args.image,
                namespace = self.namespace
            ),
        )?;
        let destination = format!(
            "oci://{}/veoveo-smoke/{}/configuration:{phase}",
            self.args.push_registry, self.namespace
        );
        let mut push = Command::new("flux");
        push.args(["push", "artifact", &destination, "--path"])
            .arg(&directory)
            .args([
                "--source=https://github.com/BiomaAI/veoveo",
                &format!("--revision={phase}"),
                "--reproducible",
                "--output=json",
                "--timeout=60s",
            ]);
        if self.args.registry_transport.is_insecure() {
            push.arg("--insecure-registry");
        }
        // Flux emits human progress on stderr and its structured receipt on stdout.
        let output = run_stdout(&mut push)?;
        #[derive(Deserialize)]
        struct Receipt {
            digest: String,
        }
        let receipt: Receipt = serde_json::from_str(&output)?;
        validate_digest(&receipt.digest)?;
        Ok(receipt.digest)
    }

    pub fn bootstrap(&self, publications: &Publications) -> Result<()> {
        self.apply(
            "chart-source.yaml",
            &self.source_yaml(
                "chart",
                &format!("charts/{CHART}"),
                &publications.chart,
                true,
            ),
        )?;
        self.select(&publications.initial)?;
        self.apply(
            "root.yaml",
            r#"apiVersion: kustomize.toolkit.fluxcd.io/v1
kind: Kustomization
metadata:
  name: configuration
spec:
  interval: 1m
  timeout: 5m
  path: ./
  prune: true
  wait: true
  deletionPolicy: WaitForTermination
  sourceRef:
    kind: OCIRepository
    name: configuration
"#,
        )?;
        Ok(())
    }

    fn source_yaml(&self, name: &str, repository: &str, digest: &str, chart: bool) -> String {
        let mut yaml = format!(
            r#"apiVersion: source.toolkit.fluxcd.io/v1
kind: OCIRepository
metadata:
  name: {name}
spec:
  interval: 1m
  timeout: 1m
  url: "oci://{}/veoveo-smoke/{}/{repository}"
  insecure: {}
  ref:
    digest: {digest}
"#,
            self.args.pull_registry,
            self.namespace,
            self.args.registry_transport.is_insecure()
        );
        if chart {
            yaml.push_str("  layerSelector:\n    mediaType: application/vnd.cncf.helm.chart.content.v1.tar+gzip\n    operation: copy\n");
        }
        yaml
    }

    pub fn select(&self, digest: &str) -> Result<()> {
        self.apply(
            "source.yaml",
            &self.source_yaml("configuration", "configuration", digest, false),
        )
    }
}

fn run_stdout(command: &mut Command) -> Result<String> {
    output(command).map(|value| value.0)
}
fn run(command: &mut Command) -> Result<String> {
    output(command).map(|(stdout, stderr)| stdout + &stderr)
}

fn output(command: &mut Command) -> Result<(String, String)> {
    // Files avoid pipe backpressure while the parent enforces the command deadline.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?)
        .spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(90) {
            child.kill()?;
            child.wait()?;
            anyhow::bail!("fixture command exceeded 90 seconds: {command:?}");
        }
        thread::sleep(Duration::from_millis(50));
    };
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;
    let mut stdout_text = String::new();
    let mut stderr_text = String::new();
    stdout.read_to_string(&mut stdout_text)?;
    stderr.read_to_string(&mut stderr_text)?;
    ensure!(
        status.success(),
        "fixture command {command:?} failed: {stdout_text}{stderr_text}"
    );
    Ok((stdout_text, stderr_text))
}
