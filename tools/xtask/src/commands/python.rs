use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use url::Url;

use crate::{context::RepositoryContext, process};

const PACKAGE_NAME: &str = "veoveo-mcp";
const IMPORT_NAME: &str = "veoveo_mcp";
const PYTHON_VERSION: &str = "3.13";
const EVIDENCE_SCHEMA: &str = "veoveo.io/python-sdk-release-evidence/v1";

pub(crate) struct BuiltPythonSdk {
    _workspace: TempDir,
    pub(crate) version: String,
    distributions: Vec<Distribution>,
}

struct Distribution {
    path: PathBuf,
    filename: String,
    sha256: String,
    media_type: &'static str,
}

pub(crate) struct PublishedPythonSdk {
    pub(crate) distributions: Vec<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseEvidence<'a> {
    schema_version: &'static str,
    package: &'static str,
    version: &'a str,
    source_revision: &'a str,
    artifacts: Vec<ArtifactEvidence<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactEvidence<'a> {
    filename: &'a str,
    sha256: &'a str,
    media_type: &'static str,
}

pub(crate) fn enforce(repository: &RepositoryContext) -> Result<()> {
    let root = repository.root();
    process::status(
        "uv",
        ["sync", "--project", "sdk", "--all-extras", "--locked"],
        Some(root),
    )?;
    process::status(
        "uv",
        [
            "run",
            "--project",
            "sdk",
            "--locked",
            "pytest",
            "sdk/python/tests",
        ],
        Some(root),
    )?;

    let artifacts = build_and_verify(root)?;
    let isolated = TempDir::new().context("creating isolated Python template workspace")?;
    let template = isolated.path().join("python-mcp");
    copy_tree(&root.join("templates/python-mcp"), &template)?;
    let links = artifacts
        .distributions
        .first()
        .and_then(|artifact| artifact.path.parent())
        .context("Python SDK build produced no distribution directory")?;
    let project = path_text(&template)?;
    let links = path_text(links)?;
    process::status(
        "uv",
        [
            "lock",
            "--project",
            project,
            "--find-links",
            links,
            "--no-sources",
        ],
        Some(root),
    )?;
    process::status(
        "uv",
        [
            "sync",
            "--project",
            project,
            "--locked",
            "--all-extras",
            "--find-links",
            links,
            "--no-sources",
        ],
        Some(root),
    )?;
    let pytest = template.join(".venv/bin/pytest");
    process::status(
        path_text(&pytest)?,
        [path_text(&template.join("tests"))?],
        Some(&template),
    )?;
    println!(
        "Python SDK {} and isolated released-package template passed",
        artifacts.version
    );
    Ok(())
}

pub(crate) fn build_and_verify(source_root: &Path) -> Result<BuiltPythonSdk> {
    let package = source_root.join("sdk/python");
    let version = project_version(&package.join("pyproject.toml"))?;
    let workspace = TempDir::new().context("creating Python SDK build workspace")?;
    let dist = workspace.path().join("dist");
    process::status(
        "uv",
        [
            "build",
            "--project",
            path_text(&package)?,
            "--out-dir",
            path_text(&dist)?,
            "--clear",
            "--no-create-gitignore",
        ],
        Some(source_root),
    )?;

    let mut distributions = Vec::new();
    for entry in fs::read_dir(&dist)
        .with_context(|| format!("reading Python distributions in {}", dist.display()))?
    {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("Python distribution filename is not UTF-8")?
            .to_owned();
        let media_type = if filename.ends_with(".whl") {
            "application/vnd.pypi.wheel"
        } else if filename.ends_with(".tar.gz") {
            "application/vnd.pypi.sdist"
        } else {
            bail!("unexpected Python distribution {filename}");
        };
        distributions.push(Distribution {
            sha256: sha256_file(&path)?,
            path,
            filename,
            media_type,
        });
    }
    distributions.sort_by(|left, right| left.filename.cmp(&right.filename));
    ensure!(
        distributions.len() == 2
            && distributions
                .iter()
                .filter(|artifact| artifact.filename.ends_with(".whl"))
                .count()
                == 1
            && distributions
                .iter()
                .filter(|artifact| artifact.filename.ends_with(".tar.gz"))
                .count()
                == 1,
        "Python SDK must build exactly one wheel and one source distribution"
    );

    let sdist = distributions
        .iter()
        .find(|artifact| artifact.filename.ends_with(".tar.gz"))
        .context("missing Python source distribution")?;
    let rebuilt = workspace.path().join("rebuilt");
    process::status(
        "uv",
        [
            "build",
            path_text(&sdist.path)?,
            "--wheel",
            "--out-dir",
            path_text(&rebuilt)?,
            "--clear",
            "--no-create-gitignore",
        ],
        Some(source_root),
    )?;
    let rebuilt_wheel = only_file_with_suffix(&rebuilt, ".whl")?;
    verify_wheel(workspace.path(), &rebuilt_wheel, &version)?;

    Ok(BuiltPythonSdk {
        _workspace: workspace,
        version,
        distributions,
    })
}

pub(crate) fn write_release_bundle(
    artifacts: &BuiltPythonSdk,
    revision: &str,
    output: &Path,
) -> Result<PublishedPythonSdk> {
    fs::create_dir_all(output)
        .with_context(|| format!("creating Python SDK release directory {}", output.display()))?;
    let mut published = Vec::new();
    for distribution in &artifacts.distributions {
        let target = output.join(&distribution.filename);
        copy_immutable(&distribution.path, &target, &distribution.sha256)?;
        published.push(target);
    }
    let evidence = ReleaseEvidence {
        schema_version: EVIDENCE_SCHEMA,
        package: PACKAGE_NAME,
        version: &artifacts.version,
        source_revision: revision,
        artifacts: artifacts
            .distributions
            .iter()
            .map(|artifact| ArtifactEvidence {
                filename: &artifact.filename,
                sha256: &artifact.sha256,
                media_type: artifact.media_type,
            })
            .collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&evidence)?;
    bytes.push(b'\n');
    let evidence_path = output.join("release-evidence.json");
    if evidence_path.exists() {
        ensure!(
            fs::read(&evidence_path)? == bytes,
            "release evidence {} already exists with different content",
            evidence_path.display()
        );
    } else {
        fs::write(&evidence_path, bytes)
            .with_context(|| format!("writing release evidence {}", evidence_path.display()))?;
    }
    Ok(PublishedPythonSdk {
        distributions: published,
    })
}

pub(crate) fn validate_publish_url(value: &str) -> Result<()> {
    validate_https_url(value, "publish URL")
}

pub(crate) fn validate_index_url(value: &str) -> Result<()> {
    validate_https_url(value, "index URL")
}

fn validate_https_url(value: &str, kind: &'static str) -> Result<()> {
    let url = Url::parse(value).with_context(|| format!("parsing Python {kind}"))?;
    ensure!(url.scheme() == "https", "Python {kind} must use HTTPS");
    ensure!(
        url.username().is_empty() && url.password().is_none(),
        "Python {kind} must not embed credentials"
    );
    ensure!(
        url.query().is_none(),
        "Python {kind} must not contain a query"
    );
    ensure!(
        url.fragment().is_none(),
        "Python {kind} must not contain a fragment"
    );
    Ok(())
}

fn project_version(path: &Path) -> Result<String> {
    let manifest =
        fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut in_project = false;
    let mut name = None;
    let mut version = None;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_project = line == "[project]";
            continue;
        }
        if !in_project {
            continue;
        }
        if let Some(value) = quoted_value(line, "name") {
            name = Some(value);
        } else if let Some(value) = quoted_value(line, "version") {
            version = Some(value);
        }
    }
    ensure!(
        name.as_deref() == Some(PACKAGE_NAME),
        "{} must declare project name {PACKAGE_NAME}",
        path.display()
    );
    version.with_context(|| format!("{} has no project version", path.display()))
}

fn quoted_value(line: &str, key: &str) -> Option<String> {
    let value = line
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim();
    value
        .strip_prefix('"')?
        .strip_suffix('"')
        .map(ToOwned::to_owned)
}

fn verify_wheel(workspace: &Path, wheel: &Path, version: &str) -> Result<()> {
    let environment = workspace.join("verify-venv");
    process::status(
        "uv",
        ["venv", "--python", PYTHON_VERSION, path_text(&environment)?],
        None,
    )?;
    let python = environment.join("bin/python");
    process::status(
        "uv",
        [
            "pip",
            "install",
            "--python",
            path_text(&python)?,
            "--no-deps",
            path_text(wheel)?,
        ],
        None,
    )?;
    let check = format!(
        "import importlib.metadata; import {IMPORT_NAME}; assert importlib.metadata.version({PACKAGE_NAME:?}) == {version:?}"
    );
    process::status(path_text(&python)?, ["-c", check.as_str()], None)
}

fn only_file_with_suffix(directory: &Path, suffix: &str) -> Result<PathBuf> {
    let files = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect::<Vec<_>>();
    ensure!(
        files.len() == 1,
        "{} must contain exactly one {suffix} file",
        directory.display()
    );
    Ok(files[0].clone())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn copy_immutable(source: &Path, target: &Path, expected_sha256: &str) -> Result<()> {
    if target.exists() {
        ensure!(
            sha256_file(target)? == expected_sha256,
            "immutable release artifact {} already exists with different bytes",
            target.display()
        );
        return Ok(());
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "copying Python distribution {} to {}",
            source.display(),
            target.display()
        )
    })?;
    ensure!(
        sha256_file(target)? == expected_sha256,
        "copied Python distribution {} failed its SHA-256 check",
        target.display()
    );
    Ok(())
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if matches!(
            name_text.as_ref(),
            ".venv" | ".pytest_cache" | "__pycache__" | "uv.lock"
        ) {
            continue;
        }
        let destination = target.join(name);
        if path.is_dir() {
            copy_tree(&path, &destination)?;
        } else if path.is_file() {
            fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{quoted_value, validate_index_url};

    #[test]
    fn reads_exact_toml_string_values() {
        assert_eq!(
            quoted_value("version = \"0.1.0\"", "version").as_deref(),
            Some("0.1.0")
        );
        assert!(quoted_value("version = '0.1.0'", "version").is_none());
    }

    #[test]
    fn private_index_urls_are_https_and_credential_free() {
        assert!(validate_index_url("https://packages.example/simple").is_ok());
        assert!(validate_index_url("http://packages.example/simple").is_err());
        assert!(validate_index_url("https://token@packages.example/simple").is_err());
    }
}
