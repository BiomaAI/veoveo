use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use veoveo_deploy_contract::{ArtifactCoordinate, ArtifactDigest, ReleaseVersion};

use crate::process;

#[cfg(test)]
mod rollout_tests;

const EVIDENCE_SCHEMA: &str = "veoveo.ai/helm-chart-release-evidence/v1";
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, clap::ValueEnum)]
pub(crate) enum Chart {
    Veoveo,
    UavSim,
}

impl Chart {
    fn definition(self) -> (&'static str, &'static str) {
        match self {
            Self::Veoveo => ("veoveo", "deploy/helm/veoveo"),
            Self::UavSim => ("uav-sim", "showcase/uav-sim/deploy/helm"),
        }
    }
}

fn selected_charts(selection: &[Chart]) -> BTreeSet<Chart> {
    if selection.is_empty() {
        [Chart::Veoveo, Chart::UavSim].into_iter().collect()
    } else {
        selection.iter().copied().collect()
    }
}

pub(crate) fn selection_name(selection: &[Chart]) -> String {
    selected_charts(selection)
        .into_iter()
        .map(|chart| chart.definition().0)
        .collect::<Vec<_>>()
        .join("+")
}

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
    sha256: ArtifactDigest,
    oci: Option<OciPublication>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OciPublication {
    coordinate: ArtifactCoordinate,
    digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HelmReleaseEvidence {
    schema_version: String,
    version: String,
    source_revision: String,
    helm_version: String,
    artifacts: Vec<HelmArtifactEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HelmArtifactEvidence {
    name: String,
    filename: String,
    sha256: ArtifactDigest,
    media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    oci: Option<OciPublication>,
}

impl HelmReleaseEvidence {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == EVIDENCE_SCHEMA,
            "unsupported Helm evidence format"
        );
        ReleaseVersion::parse(&self.version).context("invalid Helm evidence release")?;
        ensure!(
            !self.source_revision.is_empty() && !self.helm_version.is_empty(),
            "incomplete Helm evidence producer identity"
        );
        ensure!(
            !self.artifacts.is_empty() && self.artifacts.len() <= 2,
            "invalid Helm artifact selection"
        );
        let mut names = BTreeSet::new();
        for artifact in &self.artifacts {
            ensure!(
                matches!(artifact.name.as_str(), "veoveo" | "uav-sim")
                    && names.insert(&artifact.name),
                "invalid or repeated Helm chart identity"
            );
            ensure!(
                artifact.filename == format!("{}-{}.tgz", artifact.name, self.version),
                "Helm archive does not match selected chart/release"
            );
            ensure!(
                artifact.media_type == "application/vnd.cncf.helm.chart.content.v1.tar+gzip",
                "unsupported Helm archive media type"
            );
            if let Some(oci) = &artifact.oci {
                ensure!(
                    oci.digest
                        .strip_prefix("sha256:")
                        .is_some_and(|d| d.len() == 64
                            && d.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
                    "invalid Helm OCI publication"
                );
                admit_chart_coordinate(&oci.coordinate, &artifact.name, &self.version)?;
            }
        }
        Ok(())
    }

    fn admit_enrichment(&self, next: &Self) -> Result<()> {
        self.validate()?;
        next.validate()?;
        let mut comparable = next.clone();
        let mut added = false;
        ensure!(
            self.artifacts.len() == comparable.artifacts.len(),
            "Helm evidence changed selected charts"
        );
        for (before, after) in self.artifacts.iter().zip(&mut comparable.artifacts) {
            match (&before.oci, &after.oci) {
                (None, Some(_)) => {
                    added = true;
                    after.oci = None;
                }
                (a, b) => ensure!(a == b, "Helm evidence changed existing OCI publication"),
            }
        }
        ensure!(
            added && self == &comparable,
            "immutable Helm evidence differs beyond OCI enrichment"
        );
        Ok(())
    }
}

fn chart_coordinate(registry: &str, name: &str, version: &str) -> Result<ArtifactCoordinate> {
    validate_registry(registry)?;
    ReleaseVersion::parse(version)?;
    let registry_url = format!("oci://{registry}/");
    let mut url = url::Url::parse(&registry_url)?;
    ensure!(
        url.as_str() == registry_url,
        "noncanonical OCI registry prefix"
    );
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "invalid OCI registry prefix"
    );
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("OCI registry is not hierarchical"))?
        .pop_if_empty()
        .push(&format!("{name}:{version}"));
    let coordinate = ArtifactCoordinate::new(url.as_str())?;
    admit_chart_coordinate(&coordinate, name, version)?;
    Ok(coordinate)
}

fn admit_chart_coordinate(
    coordinate: &ArtifactCoordinate,
    name: &str,
    version: &str,
) -> Result<()> {
    let url = url::Url::parse(coordinate.as_str())?;
    ensure!(
        url.scheme() == "oci"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.as_str() == coordinate.as_str(),
        "noncanonical Helm OCI coordinate"
    );
    let segments = url
        .path_segments()
        .context("OCI coordinate has no chart path")?
        .collect::<Vec<_>>();
    ensure!(
        segments.last().copied() == Some(format!("{name}:{version}").as_str())
            && segments
                .iter()
                .all(|segment| !segment.is_empty() && !segment.contains('%')),
        "OCI coordinate does not match selected chart/release"
    );
    Ok(())
}

fn evidence_for_release(
    release: &HelmRelease,
    version: &str,
    revision: &str,
    helm_version: &str,
) -> HelmReleaseEvidence {
    HelmReleaseEvidence {
        schema_version: EVIDENCE_SCHEMA.to_owned(),
        version: version.to_owned(),
        source_revision: revision.to_owned(),
        helm_version: helm_version.to_owned(),
        artifacts: release
            .artifacts
            .iter()
            .map(|artifact| HelmArtifactEvidence {
                name: artifact.name.to_owned(),
                filename: artifact.filename.clone(),
                sha256: artifact.sha256.clone(),
                media_type: "application/vnd.cncf.helm.chart.content.v1.tar+gzip".to_owned(),
                oci: artifact.oci.clone(),
            })
            .collect(),
    }
}

fn read_evidence(target: &Path) -> Result<HelmReleaseEvidence> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    fs::File::open(target)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 64 * 1024,
        "Helm evidence exceeds its 64KiB limit"
    );
    let evidence: HelmReleaseEvidence =
        serde_json::from_slice(&bytes).context("admitting existing Helm release evidence")?;
    evidence.validate()?;
    Ok(evidence)
}

// This runs before even the version probe: refused prior evidence cannot invoke
// Helm or authorize another registry mutation.
fn preflight_push(
    release: &HelmRelease,
    registry: &str,
    version: &str,
    revision: &str,
) -> Result<HelmReleaseEvidence> {
    validate_registry(registry)?;
    let prior = read_evidence(&release.output.join("release-evidence.json"))?;
    let selected = evidence_for_release(release, version, revision, &prior.helm_version);
    selected.validate()?;
    ensure!(
        prior == selected,
        "Helm publication differs from selected prior release"
    );
    for artifact in &selected.artifacts {
        chart_coordinate(registry, &artifact.name, version)?;
        ensure!(
            artifact.oci.is_none(),
            "Helm publication already recorded; repeat push is refused"
        );
    }
    for artifact in &release.artifacts {
        ensure!(
            sha256_file(&artifact.archive)? == artifact.sha256,
            "selected Helm archive content changed before push"
        );
    }
    Ok(prior)
}

pub(crate) fn build(
    source: &Path,
    output: &Path,
    version: &str,
    revision: &str,
    selection: &[Chart],
) -> Result<HelmRelease> {
    ReleaseVersion::parse(version).context("validating Helm release version")?;
    let helm_version = process::output_text("helm", ["version", "--short"], Some(source))?;
    let helm_version = helm_version.trim();
    ensure!(!helm_version.is_empty(), "Helm did not report a version");

    let workspace = TempDir::new().context("creating Helm release workspace")?;
    let selected = selected_charts(selection);
    let mut artifacts = Vec::with_capacity(selected.len());
    for selected in selected {
        let (name, relative) = selected.definition();
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
    let prior = preflight_push(release, registry, version, revision)?;
    let helm_version = process::output_text("helm", ["version", "--short"], None)?;
    ensure!(
        helm_version.trim() == prior.helm_version,
        "Helm producer version changed before publication"
    );
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
            coordinate: chart_coordinate(registry, artifact.name, version)?,
            digest: digest.to_owned(),
        });
        print!("{stdout}");
        eprint!("{stderr}");
    }
    write_evidence(release, version, revision, helm_version.trim())
}

fn write_evidence(
    release: &HelmRelease,
    version: &str,
    revision: &str,
    helm_version: &str,
) -> Result<()> {
    let evidence = evidence_for_release(release, version, revision, helm_version);
    evidence.validate()?;
    let mut bytes = serde_json::to_vec_pretty(&evidence)?;
    bytes.push(b'\n');
    ensure!(
        bytes.len() <= 64 * 1024,
        "Helm evidence exceeds its 64KiB limit"
    );
    let target = release.output.join("release-evidence.json");
    if target.exists() {
        use std::io::Read as _;
        let mut existing = Vec::new();
        fs::File::open(&target)?
            .take(64 * 1024 + 1)
            .read_to_end(&mut existing)?;
        if existing == bytes {
            return Ok(());
        }
        ensure!(
            existing.len() <= 64 * 1024,
            "Helm evidence exceeds its 64KiB limit"
        );
        let existing_evidence: HelmReleaseEvidence = serde_json::from_slice(&existing)
            .context("admitting existing Helm release evidence")?;
        existing_evidence.admit_enrichment(&evidence)?;
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

fn sha256_file(path: &Path) -> Result<ArtifactDigest> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    ArtifactDigest::parse(&format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
        .map_err(Into::into)
}

fn copy_immutable(source: &Path, target: &Path, expected_sha256: &ArtifactDigest) -> Result<()> {
    if target.exists() {
        ensure!(
            &sha256_file(target)? == expected_sha256,
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
        &sha256_file(target)? == expected_sha256,
        "copied Helm archive {} failed its SHA-256 check",
        target.display()
    );
    Ok(())
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not UTF-8: {}", path.display()))
}
