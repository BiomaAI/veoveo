use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use veoveo_deploy_contract::RegistryTransport;

pub const BUILDER_NAME: &str = "veoveo";
pub const BUILDX_VERSION: &str = "v0.35.0";
pub const CERTIFICATION_CACHE_REPOSITORY: &str = "veoveo-simulation-certify-cache";
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

struct BuilderConfiguration {
    path: PathBuf,
    digest: String,
}

pub struct BuilderLease {
    _lock: File,
}

pub fn status(repository: &Path) -> Result<()> {
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
            validate_identity(repository, &inspection)?;
            println!(
                "Configuration: sha256:{}",
                active_config_digest(repository)?
            );
            Ok(())
        }
    }
}

pub fn ensure(repository: &Path) -> Result<BuilderLease> {
    let configuration = base_configuration(repository)?;
    ensure_configuration(repository, configuration)
}

pub fn ensure_for_registry(
    repository: &Path,
    registry: &str,
    transport: RegistryTransport,
) -> Result<BuilderLease> {
    let configuration = registry_configuration(repository, registry, transport)?;
    ensure_configuration(repository, configuration)
}

fn ensure_configuration(
    repository: &Path,
    configuration: BuilderConfiguration,
) -> Result<BuilderLease> {
    let lease = acquire_lease(repository)?;
    ensure_buildx(repository)?;
    if inspect(repository)?.is_none() {
        create(repository, &configuration)?;
    }
    buildx_status(repository, ["inspect", "--bootstrap", BUILDER_NAME])?;
    let mut inspection =
        inspect(repository)?.context("managed builder disappeared after creation or bootstrap")?;
    validate_identity(repository, &inspection)?;
    if active_config_digest(repository)? != configuration.digest {
        buildx_status(repository, ["rm", "--keep-state", BUILDER_NAME])?;
        create(repository, &configuration)?;
        buildx_status(repository, ["inspect", "--bootstrap", BUILDER_NAME])?;
        inspection = inspect(repository)?
            .context("managed builder disappeared after registry reconfiguration")?;
    }
    validate(repository, &inspection, &configuration)?;
    println!(
        "Builder {BUILDER_NAME} is ready with Buildx {BUILDX_VERSION} and BuildKit {BUILDKIT_VERSION}"
    );
    Ok(lease)
}

pub fn reconfigure(repository: &Path, confirmation: &str) -> Result<()> {
    ensure!(
        confirmation == BUILDER_NAME,
        "refusing to reconfigure builder: pass --confirm {BUILDER_NAME}"
    );
    let lease = acquire_lease(repository)?;
    ensure_buildx(repository)?;
    if let Some(inspection) = inspect(repository)? {
        validate_identity(repository, &inspection)?;
        buildx_status(repository, ["rm", "--keep-state", BUILDER_NAME])?;
    }
    let configuration = base_configuration(repository)?;
    create(repository, &configuration)?;
    buildx_status(repository, ["inspect", "--bootstrap", BUILDER_NAME])?;
    let inspection =
        inspect(repository)?.context("managed builder disappeared after reconfiguration")?;
    validate(repository, &inspection, &configuration)?;
    drop(lease);
    Ok(())
}

pub fn recreate(repository: &Path, confirmation: &str) -> Result<()> {
    ensure!(
        confirmation == BUILDER_NAME,
        "refusing to remove builder: pass --confirm {BUILDER_NAME}"
    );
    let lease = acquire_lease(repository)?;
    ensure_buildx(repository)?;
    if inspect(repository)?.is_some() {
        buildx_status(repository, ["rm", BUILDER_NAME])?;
    }
    let configuration = base_configuration(repository)?;
    create(repository, &configuration)?;
    buildx_status(repository, ["inspect", "--bootstrap", BUILDER_NAME])?;
    let inspection =
        inspect(repository)?.context("managed builder disappeared after recreation")?;
    validate(repository, &inspection, &configuration)?;
    drop(lease);
    Ok(())
}

pub fn prune_certification_cache(repository: &Path, confirmation: &str) -> Result<()> {
    ensure!(
        confirmation == CERTIFICATION_CACHE_REPOSITORY,
        "refusing to remove certification materializations: pass --confirm {CERTIFICATION_CACHE_REPOSITORY}"
    );
    let filter = format!("reference={CERTIFICATION_CACHE_REPOSITORY}:*");
    let output = Command::new("docker")
        .current_dir(repository)
        .args([
            "image",
            "ls",
            "--filter",
            &filter,
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ])
        .output()
        .context("listing simulation certification materializations")?;
    ensure!(
        output.status.success(),
        "listing simulation certification materializations failed with {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let references = String::from_utf8(output.stdout)
        .context("certification materialization list is not UTF-8")?
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if references.is_empty() {
        println!("Simulation certification materialization cache is empty");
        return Ok(());
    }
    let status = Command::new("docker")
        .current_dir(repository)
        .args(["image", "rm"])
        .args(&references)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("removing simulation certification materializations")?;
    ensure!(
        status.success(),
        "removing simulation certification materializations failed with {status}"
    );
    println!(
        "Removed {} simulation certification materialization(s)",
        references.len()
    );
    Ok(())
}

pub fn installed_buildx_version(repository: &Path) -> Result<String> {
    let output = buildx_command(repository)?
        .arg("version")
        .current_dir(repository)
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

fn require_buildx(repository: &Path) -> Result<()> {
    let installed = installed_buildx_version(repository)?;
    ensure!(
        installed == BUILDX_VERSION,
        "Docker Buildx {installed} is installed; Veoveo requires {BUILDX_VERSION}"
    );
    Ok(())
}

pub fn buildx_command(repository: &Path) -> Result<Command> {
    let managed = managed_buildx(repository)?;
    if managed.binary.exists() {
        validate_managed_buildx(&managed)?;
        let mut command = Command::new(&managed.binary);
        command.env("BUILDX_CONFIG", &managed.config);
        return Ok(command);
    }

    let output = Command::new("docker")
        .args(["buildx", "version"])
        .current_dir(repository)
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

fn ensure_buildx(repository: &Path) -> Result<()> {
    let managed = managed_buildx(repository)?;
    if managed.binary.exists() {
        return validate_managed_buildx(&managed);
    }
    if let Ok(output) = Command::new("docker")
        .args(["buildx", "version"])
        .current_dir(repository)
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

fn create(repository: &Path, configuration: &BuilderConfiguration) -> Result<()> {
    let config_arg = configuration
        .path
        .to_str()
        .context("BuildKit configuration path is not UTF-8")?;
    let identity = format!("env.VEOVEO_BUILDER_CONFIG_SHA256={}", configuration.digest);
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

fn inspect(repository: &Path) -> Result<Option<BuilderInspection>> {
    let output = buildx_command(repository)?
        .current_dir(repository)
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

fn buildx_status<const N: usize>(repository: &Path, args: [&str; N]) -> Result<()> {
    let status = buildx_command(repository)?
        .current_dir(repository)
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

fn managed_buildx(repository: &Path) -> Result<ManagedBuildx> {
    let (release_name, sha256) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("buildx-v0.35.0.linux-amd64", BUILDX_LINUX_AMD64_SHA256),
        ("linux", "aarch64") => ("buildx-v0.35.0.linux-arm64", BUILDX_LINUX_ARM64_SHA256),
        _ => ("", ""),
    };
    let root = managed_root(repository)?.join("docker-config");
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

fn validate(
    repository: &Path,
    inspection: &BuilderInspection,
    configuration: &BuilderConfiguration,
) -> Result<()> {
    validate_identity(repository, inspection)?;
    let active_digest = active_config_digest(repository)?;
    ensure!(
        active_digest == configuration.digest,
        "builder {BUILDER_NAME} was created with BuildKit configuration sha256:{active_digest}; expected sha256:{}",
        configuration.digest
    );
    Ok(())
}

fn active_config_digest(repository: &Path) -> Result<String> {
    let environment = output_text(
        "docker",
        [
            "inspect",
            BUILDER_CONTAINER,
            "--format",
            "{{range .Config.Env}}{{println .}}{{end}}",
        ],
        Some(repository),
    )?;
    environment
        .lines()
        .find_map(|line| line.strip_prefix("VEOVEO_BUILDER_CONFIG_SHA256="))
        .map(ToOwned::to_owned)
        .context("managed builder does not declare its BuildKit configuration digest")
}

fn validate_identity(repository: &Path, inspection: &BuilderInspection) -> Result<()> {
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

    let image = output_text(
        "docker",
        [
            "inspect",
            BUILDER_CONTAINER,
            "--format",
            "{{.Config.Image}}",
        ],
        Some(repository),
    )?;
    ensure!(
        image.trim() == BUILDKIT_IMAGE,
        "builder {BUILDER_NAME} uses image {}; expected {BUILDKIT_IMAGE}",
        image.trim()
    );
    Ok(())
}

fn config_digest(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading BuildKit configuration {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn base_configuration(repository: &Path) -> Result<BuilderConfiguration> {
    let path = repository.join("tools/image-build/buildkitd.toml");
    let digest = config_digest(&path)?;
    Ok(BuilderConfiguration { path, digest })
}

fn registry_configuration(
    repository: &Path,
    registry: &str,
    transport: RegistryTransport,
) -> Result<BuilderConfiguration> {
    let base = base_configuration(repository)?;
    if transport == RegistryTransport::Tls {
        return Ok(base);
    }
    ensure!(
        !registry
            .chars()
            .any(|character| character.is_control() || matches!(character, '"' | '\\')),
        "registry address contains characters that cannot be represented in BuildKit configuration"
    );
    let mut bytes = fs::read(&base.path)
        .with_context(|| format!("reading BuildKit configuration {}", base.path.display()))?;
    ensure!(
        bytes.last().is_none_or(|byte| *byte == b'\n'),
        "BuildKit base configuration must end in a newline"
    );
    write!(
        bytes,
        "\n[registry.\"{registry}\"]\n  http = true\n  insecure = true\n"
    )
    .context("rendering registry-specific BuildKit configuration")?;
    let digest = hex::encode(Sha256::digest(&bytes));
    let root = managed_root(repository)?;
    let directory = root.join("builder-config");
    fs::create_dir_all(&directory).with_context(|| {
        format!(
            "creating generated BuildKit configuration directory {}",
            directory.display()
        )
    })?;
    let path = directory.join(format!("{digest}.toml"));
    if path.exists() {
        ensure!(
            fs::read(&path)
                .with_context(|| format!("reading generated configuration {}", path.display()))?
                == bytes,
            "generated BuildKit configuration {} does not match its content digest",
            path.display()
        );
    } else {
        let mut temporary = NamedTempFile::new_in(&directory).with_context(|| {
            format!(
                "creating generated BuildKit configuration in {}",
                directory.display()
            )
        })?;
        temporary
            .write_all(&bytes)
            .context("writing generated BuildKit configuration")?;
        temporary
            .as_file()
            .sync_all()
            .context("syncing generated BuildKit configuration")?;
        temporary
            .persist(&path)
            .map_err(|error| error.error)
            .with_context(|| {
                format!(
                    "publishing generated BuildKit configuration {}",
                    path.display()
                )
            })?;
    }
    Ok(BuilderConfiguration { path, digest })
}

fn acquire_lease(repository: &Path) -> Result<BuilderLease> {
    let root = managed_root(repository)?;
    fs::create_dir_all(&root)
        .with_context(|| format!("creating managed builder directory {}", root.display()))?;
    let path = root.join("builder.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening managed builder lock {}", path.display()))?;
    File::lock(&lock).with_context(|| format!("locking managed builder {}", path.display()))?;
    Ok(BuilderLease { _lock: lock })
}

fn managed_root(repository: &Path) -> Result<PathBuf> {
    let common = output_text(
        "git",
        ["rev-parse", "--path-format=absolute", "--git-common-dir"],
        Some(repository),
    )?;
    let common = fs::canonicalize(common.trim())
        .with_context(|| format!("resolving Git common directory {}", common.trim()))?;
    let worktree = common
        .parent()
        .context("Git common directory has no parent worktree")?;
    Ok(worktree.join("target/veoveo-xtask"))
}

fn output_text<const N: usize>(
    program: &str,
    args: [&str; N],
    directory: Option<&Path>,
) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command
        .output()
        .with_context(|| format!("running {program}"))?;
    ensure!(
        output.status.success(),
        "{program} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("command output is not UTF-8")
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

    use super::{
        BuilderInspection, managed_buildx, parse_buildx_version, parse_inspection,
        prune_certification_cache, registry_configuration,
    };
    use veoveo_deploy_contract::RegistryTransport;

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

        let main = managed_buildx(&repository).expect("resolve main managed Buildx");
        let linked = managed_buildx(&publication).expect("resolve linked managed Buildx");

        assert_eq!(main.binary, linked.binary);
        assert_eq!(main.config, linked.config);
        assert_eq!(
            main.binary,
            repository.join("target/veoveo-xtask/docker-config/cli-plugins/docker-buildx")
        );
    }

    #[test]
    fn insecure_registry_configuration_uses_the_selected_profile_address() {
        let temporary = tempdir().expect("create temporary repository");
        let repository = temporary.path().join("repository");
        fs::create_dir_all(repository.join("tools/image-build")).expect("create config directory");
        git(&repository, &["init", "--quiet"]);
        fs::write(
            repository.join("tools/image-build/buildkitd.toml"),
            "[worker.oci]\n  gc = true\n",
        )
        .expect("write base configuration");
        let generated = registry_configuration(
            &repository,
            "registry.private.internal:5002",
            RegistryTransport::InsecureHttp,
        )
        .expect("generate registry configuration");
        let contents = fs::read_to_string(&generated.path).expect("read generated configuration");

        assert!(contents.contains("[registry.\"registry.private.internal:5002\"]"));
        assert!(contents.contains("http = true"));
        assert!(contents.contains("insecure = true"));
        assert!(!contents.contains(":5001"));
        assert_eq!(
            generated.path.file_stem().and_then(std::ffi::OsStr::to_str),
            Some(generated.digest.as_str())
        );
    }

    #[test]
    fn certification_cache_removal_requires_exact_scope_confirmation() {
        let temporary = tempdir().expect("create temporary repository");
        let error = prune_certification_cache(temporary.path(), "veoveo")
            .expect_err("reject broad confirmation");
        assert!(error.to_string().contains("refusing to remove"));
    }
}
