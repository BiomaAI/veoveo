use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use veoveo_extension_contract::{EXTENSION_HELM_LIBRARY_API, ReleaseVersion};

use crate::process;

const EVIDENCE_SCHEMA: &str = "veoveo.io/helm-chart-release-evidence/v1";
const CHARTS: [(&str, &str); 3] = [
    ("veoveo-extension", "deploy/helm/veoveo-extension"),
    ("veoveo", "deploy/helm/veoveo"),
    ("uav-sim", "showcase/uav-sim/deploy/helm"),
];

#[derive(Debug)]
pub(crate) struct HelmRelease {
    pub(crate) output: PathBuf,
    pub(crate) artifacts: Vec<HelmArtifact>,
}

#[derive(Debug)]
pub(crate) struct HelmArtifact {
    name: &'static str,
    archive: PathBuf,
    filename: String,
    sha256: String,
    oci: Option<OciPublication>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OciPublication {
    coordinate: String,
    digest: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HelmReleaseEvidence<'a> {
    schema_version: &'static str,
    version: &'a str,
    source_revision: &'a str,
    helm_version: &'a str,
    artifacts: Vec<HelmArtifactEvidence<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HelmArtifactEvidence<'a> {
    name: &'static str,
    filename: &'a str,
    sha256: &'a str,
    media_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    oci: Option<&'a OciPublication>,
}

pub(crate) fn build(
    source: &Path,
    output: &Path,
    version: &str,
    revision: &str,
) -> Result<HelmRelease> {
    ReleaseVersion::new(version).context("validating Helm release version")?;
    let helm_version = process::output_text("helm", ["version", "--short"], Some(source))?;
    let helm_version = helm_version.trim();
    ensure!(!helm_version.is_empty(), "Helm did not report a version");

    let workspace = TempDir::new().context("creating Helm release workspace")?;
    let library_chart = fs::read_to_string(source.join("deploy/helm/veoveo-extension/Chart.yaml"))?;
    ensure!(
        library_chart.contains(&format!(
            "veoveo.ai/library-api: {EXTENSION_HELM_LIBRARY_API}"
        )),
        "extension Helm chart must declare library API {EXTENSION_HELM_LIBRARY_API}"
    );
    let mut artifacts = Vec::with_capacity(CHARTS.len());
    for (name, relative) in CHARTS {
        let chart = source.join(relative);
        ensure!(chart.is_dir(), "missing Helm chart {}", chart.display());
        process::status("helm", ["lint", path_text(&chart)?], Some(source))?;
        process::status(
            "helm",
            [
                "package",
                path_text(&chart)?,
                "--version",
                version,
                "--app-version",
                revision,
                "--destination",
                path_text(workspace.path())?,
            ],
            Some(source),
        )?;
        let filename = format!("{name}-{version}.tgz");
        let staged = workspace.path().join(&filename);
        ensure!(
            staged.is_file(),
            "Helm did not produce expected archive {}",
            staged.display()
        );
        let sha256 = sha256_file(&staged)?;
        fs::create_dir_all(output)
            .with_context(|| format!("creating Helm release directory {}", output.display()))?;
        let archive = output.join(&filename);
        copy_immutable(&staged, &archive, &sha256)?;
        artifacts.push(HelmArtifact {
            name,
            archive,
            filename,
            sha256,
            oci: None,
        });
    }

    let release = HelmRelease {
        output: output.to_path_buf(),
        artifacts,
    };
    write_evidence(&release, version, revision, helm_version)?;
    Ok(release)
}

pub(crate) fn push(
    release: &mut HelmRelease,
    registry: &str,
    plain_http: bool,
    version: &str,
    revision: &str,
) -> Result<()> {
    validate_registry(registry)?;
    let destination = format!("oci://{registry}");
    for artifact in &mut release.artifacts {
        let mut arguments = vec![
            OsString::from("push"),
            artifact.archive.as_os_str().to_owned(),
            OsString::from(&destination),
        ];
        if plain_http {
            arguments.push(OsString::from("--plain-http"));
        }
        let output = process::output("helm", arguments, None)?;
        let stdout = String::from_utf8(output.stdout).context("Helm push stdout is not UTF-8")?;
        let stderr = String::from_utf8(output.stderr).context("Helm push stderr is not UTF-8")?;
        let combined = format!("{stdout}\n{stderr}");
        let digest = combined
            .lines()
            .find_map(|line| line.trim().strip_prefix("Digest: "))
            .context("Helm push did not report the OCI manifest digest")?;
        validate_digest(digest)?;
        artifact.oci = Some(OciPublication {
            coordinate: format!("oci://{registry}/{}:{version}", artifact.name),
            digest: digest.to_owned(),
        });
        print!("{stdout}");
        eprint!("{stderr}");
    }
    let helm_version = process::output_text("helm", ["version", "--short"], None)?;
    write_evidence(release, version, revision, helm_version.trim())
}

fn write_evidence(
    release: &HelmRelease,
    version: &str,
    revision: &str,
    helm_version: &str,
) -> Result<()> {
    let evidence = HelmReleaseEvidence {
        schema_version: EVIDENCE_SCHEMA,
        version,
        source_revision: revision,
        helm_version,
        artifacts: release
            .artifacts
            .iter()
            .map(|artifact| HelmArtifactEvidence {
                name: artifact.name,
                filename: &artifact.filename,
                sha256: &artifact.sha256,
                media_type: "application/vnd.cncf.helm.chart.content.v1.tar+gzip",
                oci: artifact.oci.as_ref(),
            })
            .collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&evidence)?;
    bytes.push(b'\n');
    let target = release.output.join("release-evidence.json");
    if target.exists() {
        let existing = fs::read(&target)?;
        if existing == bytes {
            return Ok(());
        }
        let existing_value: serde_json::Value = serde_json::from_slice(&existing)
            .with_context(|| format!("decoding existing evidence {}", target.display()))?;
        let existing_has_oci = existing_value.pointer("/artifacts/0/oci").is_some();
        let new_has_oci = release
            .artifacts
            .iter()
            .any(|artifact| artifact.oci.is_some());
        ensure!(
            new_has_oci && !existing_has_oci,
            "immutable Helm evidence {} already exists with different content",
            target.display()
        );
    }
    fs::write(&target, bytes)
        .with_context(|| format!("writing Helm release evidence {}", target.display()))
}

fn validate_registry(registry: &str) -> Result<()> {
    ensure!(!registry.trim().is_empty(), "registry cannot be empty");
    ensure!(
        !registry.contains("://"),
        "registry must be a host and repository prefix without a URL scheme"
    );
    ensure!(
        !registry.ends_with('/'),
        "registry must not end with a slash"
    );
    ensure!(
        !registry.chars().any(char::is_whitespace),
        "registry must not contain whitespace"
    );
    ensure!(
        !registry.contains('@'),
        "registry must not contain credentials"
    );
    Ok(())
}

fn validate_digest(digest: &str) -> Result<()> {
    let hex = digest
        .strip_prefix("sha256:")
        .context("OCI digest must start with sha256:")?;
    ensure!(
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "OCI digest must contain 64 lowercase hexadecimal digits"
    );
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

fn copy_immutable(source: &Path, target: &Path, expected_sha256: &str) -> Result<()> {
    if target.exists() {
        ensure!(
            sha256_file(target)? == expected_sha256,
            "immutable Helm artifact {} already exists with different bytes",
            target.display()
        );
        return Ok(());
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "copying Helm archive {} to {}",
            source.display(),
            target.display()
        )
    })?;
    ensure!(
        sha256_file(target)? == expected_sha256,
        "copied Helm archive {} failed its SHA-256 check",
        target.display()
    );
    Ok(())
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not UTF-8: {}", path.display()))
}
