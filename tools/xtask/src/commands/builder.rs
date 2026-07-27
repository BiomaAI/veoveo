use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::{context::RepositoryContext, process};

pub(crate) const BUILDER_NAME: &str = "veoveo";
pub(crate) const BUILDX_VERSION: &str = "v0.35.0";
const BUILDX_LINUX_AMD64_SHA256: &str =
    "d41ece72044243b4f58b343441ae37446d9c29a7d6b5e11c61847bbcf8f7dfda";
const BUILDX_LINUX_ARM64_SHA256: &str =
    "c4248d6cbc4a619a7e0b4609c11e509ad4ac0b475e1c64817c0ac20c5d90c766";
const BUILDKIT_VERSION: &str = "v0.31.2";
const BUILDKIT_IMAGE: &str = "docker.io/moby/buildkit:v0.31.2@sha256:2f5adac4ecd194d9f8c10b7b5d7bceb5186853db1b26e5abd3a657af0b7e26ec";
const BUILDER_CONTAINER: &str = "buildx_buildkit_veoveo0";

#[derive(Debug, Eq, PartialEq)]
struct BuilderInspection {
    driver: String,
    buildkit_version: Option<String>,
}

pub(crate) fn status(repository: &RepositoryContext) -> Result<()> {
    let buildx = installed_buildx_version(repository)?;
    println!("Docker Buildx: {buildx} (required {})", BUILDX_VERSION);
    match inspect(repository)? {
        None => {
            println!("Builder {BUILDER_NAME}: missing");
            bail!("managed builder {BUILDER_NAME} does not exist")
        }
        Some(inspection) => {
            println!("Builder {BUILDER_NAME}: present");
            println!("Driver: {}", inspection.driver);
            println!(
                "BuildKit: {}",
                inspection.buildkit_version.as_deref().unwrap_or("inactive")
            );
            validate(repository, &inspection)
        }
    }
}

pub(crate) fn ensure(repository: &RepositoryContext) -> Result<()> {
    ensure_buildx(repository)?;
    if inspect(repository)?.is_none() {
        create(repository)?;
    }
    buildx_status(repository, ["inspect", "--bootstrap", BUILDER_NAME])?;
    let inspection =
        inspect(repository)?.context("managed builder disappeared after creation or bootstrap")?;
    validate(repository, &inspection)?;
    println!(
        "Builder {BUILDER_NAME} is ready with Buildx {BUILDX_VERSION} and BuildKit {BUILDKIT_VERSION}"
    );
    Ok(())
}

pub(crate) fn recreate(repository: &RepositoryContext, confirmation: &str) -> Result<()> {
    ensure!(
        confirmation == BUILDER_NAME,
        "refusing to remove builder: pass --confirm {BUILDER_NAME}"
    );
    ensure_buildx(repository)?;
    if inspect(repository)?.is_some() {
        buildx_status(repository, ["rm", BUILDER_NAME])?;
    }
    ensure(repository)
}

pub(crate) fn installed_buildx_version(repository: &RepositoryContext) -> Result<String> {
    let output = buildx_command(repository)?
        .arg("version")
        .current_dir(repository.root())
        .output()
        .context("running Docker Buildx")?;
    ensure!(
        output.status.success(),
        "Docker Buildx failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_buildx_version(&String::from_utf8_lossy(&output.stdout))
        .context("decoding Docker Buildx version")
}

fn require_buildx(repository: &RepositoryContext) -> Result<()> {
    let installed = installed_buildx_version(repository)?;
    ensure!(
        installed == BUILDX_VERSION,
        "Docker Buildx {installed} is installed; Veoveo requires {BUILDX_VERSION}"
    );
    Ok(())
}

pub(crate) fn buildx_command(repository: &RepositoryContext) -> Result<Command> {
    let managed = managed_buildx(repository)?;
    if managed.binary.exists() {
        validate_managed_buildx(&managed)?;
        let mut command = Command::new(&managed.binary);
        command.env("BUILDX_CONFIG", &managed.config);
        return Ok(command);
    }

    let output = Command::new("docker")
        .args(["buildx", "version"])
        .current_dir(repository.root())
        .output()
        .context("running host Docker Buildx")?;
    ensure!(
        output.status.success(),
        "Docker Buildx is unavailable; run `cargo xtask image builder ensure`"
    );
    let version = parse_buildx_version(&String::from_utf8_lossy(&output.stdout))
        .context("decoding host Docker Buildx version")?;
    ensure!(
        version == BUILDX_VERSION,
        "Docker Buildx {version} is installed; run `cargo xtask image builder ensure` to install managed {BUILDX_VERSION}"
    );
    let mut command = Command::new("docker");
    command.arg("buildx");
    Ok(command)
}

fn ensure_buildx(repository: &RepositoryContext) -> Result<()> {
    let managed = managed_buildx(repository)?;
    if managed.binary.exists() {
        return validate_managed_buildx(&managed);
    }
    if let Ok(output) = Command::new("docker")
        .args(["buildx", "version"])
        .current_dir(repository.root())
        .output()
        && output.status.success()
        && parse_buildx_version(&String::from_utf8_lossy(&output.stdout)).as_deref()
            == Some(BUILDX_VERSION)
    {
        return Ok(());
    }
    install_managed_buildx(&managed)?;
    require_buildx(repository)
}

fn create(repository: &RepositoryContext) -> Result<()> {
    let config = repository.root().join("tools/image-build/buildkitd.toml");
    let config_digest = config_digest(&config)?;
    let config_arg = config
        .to_str()
        .context("BuildKit configuration path is not UTF-8")?;
    let identity = format!("env.VEOVEO_BUILDER_CONFIG_SHA256={config_digest}");
    let image = format!("image={BUILDKIT_IMAGE}");
    buildx_status(
        repository,
        [
            "create",
            "--name",
            BUILDER_NAME,
            "--driver",
            "docker-container",
            "--driver-opt",
            image.as_str(),
            "--driver-opt",
            "network=host",
            "--driver-opt",
            identity.as_str(),
            "--buildkitd-config",
            config_arg,
        ],
    )
}

fn inspect(repository: &RepositoryContext) -> Result<Option<BuilderInspection>> {
    let output = buildx_command(repository)?
        .current_dir(repository.root())
        .args(["inspect", BUILDER_NAME])
        .output()
        .context("inspecting the managed Buildx builder")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no builder") {
            return Ok(None);
        }
        bail!(
            "docker buildx inspect failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            stderr
        );
    }
    let text = String::from_utf8(output.stdout).context("builder inspection is not UTF-8")?;
    Ok(Some(parse_inspection(&text)?))
}

fn buildx_status<const N: usize>(repository: &RepositoryContext, args: [&str; N]) -> Result<()> {
    let status = buildx_command(repository)?
        .current_dir(repository.root())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running Docker Buildx")?;
    ensure!(status.success(), "Docker Buildx failed with {status}");
    Ok(())
}

struct ManagedBuildx {
    binary: PathBuf,
    config: PathBuf,
    release_name: &'static str,
    sha256: &'static str,
}

fn managed_buildx(repository: &RepositoryContext) -> Result<ManagedBuildx> {
    let (release_name, sha256) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("buildx-v0.35.0.linux-amd64", BUILDX_LINUX_AMD64_SHA256),
        ("linux", "aarch64") => ("buildx-v0.35.0.linux-arm64", BUILDX_LINUX_ARM64_SHA256),
        _ => ("", ""),
    };
    let common = process::output_text(
        "git",
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
        Some(repository.root()),
    )?;
    let common = fs::canonicalize(common.trim())
        .with_context(|| format!("resolving Git common directory {}", common.trim()))?;
    let worktree = common
        .parent()
        .context("Git common directory has no parent worktree")?;
    let root = worktree.join("target/veoveo-xtask/docker-config");
    Ok(ManagedBuildx {
        binary: root.join("cli-plugins/docker-buildx"),
        config: root.join("buildx"),
        release_name,
        sha256,
    })
}

fn validate_managed_buildx(managed: &ManagedBuildx) -> Result<()> {
    ensure!(
        !managed.release_name.is_empty(),
        "managed Docker Buildx supports Linux amd64 and arm64; install {BUILDX_VERSION} through Docker on this host"
    );
    let bytes = fs::read(&managed.binary)
        .with_context(|| format!("reading managed Docker Buildx {}", managed.binary.display()))?;
    ensure!(
        hex::encode(Sha256::digest(bytes)) == managed.sha256,
        "managed Docker Buildx {} failed its SHA-256 check; remove it and run `cargo xtask image builder ensure`",
        managed.binary.display()
    );
    Ok(())
}

fn install_managed_buildx(managed: &ManagedBuildx) -> Result<()> {
    ensure!(
        !managed.release_name.is_empty(),
        "managed Docker Buildx supports Linux amd64 and arm64; install {BUILDX_VERSION} through Docker on this host"
    );
    let parent = managed
        .binary
        .parent()
        .context("managed Buildx path has no parent")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating managed tool directory {}", parent.display()))?;
    fs::create_dir_all(&managed.config).with_context(|| {
        format!(
            "creating Buildx state directory {}",
            managed.config.display()
        )
    })?;
    let download = NamedTempFile::new_in(parent).context("creating Buildx download")?;
    let url = format!(
        "https://github.com/docker/buildx/releases/download/{BUILDX_VERSION}/{}",
        managed.release_name
    );
    let status = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "--fail", "--location"])
        .arg("--output")
        .arg(download.path())
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("downloading managed Docker Buildx")?;
    ensure!(
        status.success(),
        "downloading Docker Buildx failed with {status}"
    );
    let digest = hex::encode(Sha256::digest(
        fs::read(download.path()).context("reading downloaded Docker Buildx")?,
    ));
    ensure!(
        digest == managed.sha256,
        "downloaded Docker Buildx failed its SHA-256 check"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        download
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o755))
            .context("making managed Docker Buildx executable")?;
    }
    download
        .persist(&managed.binary)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "installing managed Docker Buildx {}",
                managed.binary.display()
            )
        })?;
    println!(
        "Installed Docker Buildx {BUILDX_VERSION} at {}",
        managed.binary.display()
    );
    Ok(())
}

fn validate(repository: &RepositoryContext, inspection: &BuilderInspection) -> Result<()> {
    require_buildx(repository)?;
    ensure!(
        inspection.driver == "docker-container",
        "builder {BUILDER_NAME} uses driver {}; expected docker-container",
        inspection.driver
    );
    ensure!(
        inspection.buildkit_version.as_deref() == Some(BUILDKIT_VERSION),
        "builder {BUILDER_NAME} runs BuildKit {}; expected {BUILDKIT_VERSION}",
        inspection.buildkit_version.as_deref().unwrap_or("inactive")
    );

    let image = process::output_text(
        "docker",
        [
            "inspect",
            BUILDER_CONTAINER,
            "--format",
            "{{.Config.Image}}",
        ],
        Some(repository.root()),
    )?;
    ensure!(
        image.trim() == BUILDKIT_IMAGE,
        "builder {BUILDER_NAME} uses image {}; expected {BUILDKIT_IMAGE}",
        image.trim()
    );

    let expected_digest =
        config_digest(&repository.root().join("tools/image-build/buildkitd.toml"))?;
    let environment = process::output_text(
        "docker",
        [
            "inspect",
            BUILDER_CONTAINER,
            "--format",
            "{{range .Config.Env}}{{println .}}{{end}}",
        ],
        Some(repository.root()),
    )?;
    ensure!(
        environment
            .lines()
            .any(|line| line == format!("VEOVEO_BUILDER_CONFIG_SHA256={expected_digest}")),
        "builder {BUILDER_NAME} was created with a different BuildKit configuration"
    );
    Ok(())
}

fn config_digest(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading BuildKit configuration {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn parse_buildx_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| {
            part.strip_prefix('v')
                .and_then(|version| version.chars().next())
                .is_some_and(|value| value.is_ascii_digit())
        })
        .map(ToOwned::to_owned)
}

fn parse_inspection(output: &str) -> Result<BuilderInspection> {
    let driver = field(output, "Driver:").context("builder inspection has no Driver field")?;
    Ok(BuilderInspection {
        driver: driver.to_owned(),
        buildkit_version: field(output, "BuildKit version:").map(ToOwned::to_owned),
    })
}

fn field<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix(name).map(str::trim))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use tempfile::tempdir;

    use super::{BuilderInspection, managed_buildx, parse_buildx_version, parse_inspection};
    use crate::context::RepositoryContext;

    fn git(repository: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .current_dir(repository)
            .args(arguments)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {arguments:?} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn reads_buildx_release() {
        assert_eq!(
            parse_buildx_version(
                "github.com/docker/buildx v0.35.0 1707acde5c8b6a2e8b4b62c4613b1d7e5f4de154"
            ),
            Some("v0.35.0".to_owned())
        );
    }

    #[test]
    fn reads_builder_contract() {
        let inspection = parse_inspection(
            "Name: veoveo\nDriver: docker-container\n\nNodes:\nName: veoveo0\nBuildKit version: v0.31.2\n",
        )
        .expect("parse builder");
        assert_eq!(
            inspection,
            BuilderInspection {
                driver: "docker-container".to_owned(),
                buildkit_version: Some("v0.31.2".to_owned()),
            }
        );
    }

    #[test]
    fn reads_inactive_builder_contract() {
        let inspection =
            parse_inspection("Name: veoveo\nDriver: docker-container\n\nNodes:\nName: veoveo0\n")
                .expect("parse inactive builder");
        assert_eq!(
            inspection,
            BuilderInspection {
                driver: "docker-container".to_owned(),
                buildkit_version: None,
            }
        );
    }

    #[test]
    fn main_and_linked_worktrees_share_managed_buildx_state() {
        let temporary = tempdir().expect("create temporary repository");
        let repository = temporary.path().join("repository");
        let publication = temporary.path().join("publication");
        fs::create_dir(&repository).expect("create repository");
        git(&repository, &["init", "--quiet"]);
        git(&repository, &["config", "user.name", "Veoveo Test"]);
        git(
            &repository,
            &["config", "user.email", "veoveo-test@example.invalid"],
        );
        git(&repository, &["config", "commit.gpgsign", "false"]);
        fs::write(repository.join("tracked"), "content\n").expect("write fixture");
        git(&repository, &["add", "tracked"]);
        git(&repository, &["commit", "--quiet", "-m", "fixture"]);
        git(
            &repository,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                publication.to_str().expect("UTF-8 publication path"),
                "HEAD",
            ],
        );

        let main_context = RepositoryContext::discover(&repository).expect("discover main");
        let publication_context =
            RepositoryContext::discover(&publication).expect("discover publication");
        let main = managed_buildx(&main_context).expect("resolve main managed Buildx");
        let linked = managed_buildx(&publication_context).expect("resolve linked managed Buildx");

        assert_eq!(main.binary, linked.binary);
        assert_eq!(main.config, linked.config);
        assert_eq!(
            main.binary,
            repository.join("target/veoveo-xtask/docker-config/cli-plugins/docker-buildx")
        );
    }
}
