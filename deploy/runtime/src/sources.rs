use crate::{
    charts::validate_locked_charts,
    images::locked_image_digests,
    process::{output_checked, path_str, status_checked},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use url::Url;
use veoveo_deploy_contract::{DeploymentLock, DeploymentSource, LoadedProfile, SourceRepository};
// Immutable source checkouts leave unrelated LFS objects as pointers.
const GIT_SKIP_LFS_SMUDGE: &[(&str, &str)] = &[("GIT_LFS_SKIP_SMUDGE", "1")];

#[derive(Debug)]
pub(crate) struct ResolvedSource {
    pub(crate) definition: DeploymentSource,
    pub(crate) repository: PathBuf,
    pub(crate) revision: String,
    pub(crate) image_digests: BTreeMap<String, String>,
    pub(crate) deployment_image_digests: BTreeMap<String, String>,
    pub(crate) _checkout: tempfile::TempDir,
}

pub(crate) fn load_profile(path: &Path) -> Result<LoadedProfile> {
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let repository = repository_root(base).or_else(|_| {
        let current = env::current_dir().context("reading current working directory")?;
        repository_root(&current).context(
            "deployment profile is outside a Git worktree and the command was not run from the Veoveo repository",
        )
    })?;
    LoadedProfile::load(path, &repository)
}

pub(crate) fn load_deployment_lock(path: &Path) -> Result<DeploymentLock> {
    let bytes =
        fs::read(path).with_context(|| format!("reading deployment lock {}", path.display()))?;
    let lock = serde_json::from_slice::<DeploymentLock>(&bytes)
        .with_context(|| format!("decoding deployment lock {}", path.display()))?;
    lock.validate()
        .with_context(|| format!("validating deployment lock {}", path.display()))?;
    Ok(lock)
}

pub(crate) fn resolve_sources(profile: &LoadedProfile) -> Result<Vec<ResolvedSource>> {
    let mut resolved = Vec::with_capacity(profile.definition.sources.len());
    for source in &profile.definition.sources {
        let origin = match &source.repository {
            SourceRepository::Local { .. } => profile.local_source_root(source)?,
            SourceRepository::Git { url } => PathBuf::from(url),
        };
        let checkout = tempfile::Builder::new()
            .prefix(&format!("veoveo-deployment-{}-", source.name))
            .tempdir()
            .with_context(|| format!("creating checkout for source {}", source.name))?;
        let destination = checkout.path();
        let clone_args = [
            "clone",
            "--quiet",
            "--no-checkout",
            path_str(&origin)?,
            path_str(destination)?,
        ];
        status_checked("git", clone_args, GIT_SKIP_LFS_SMUDGE, None)
            .with_context(|| format!("cloning deployment source {}", source.name))?;
        let revision = resolve_revision(destination, &source.revision)?;
        status_checked(
            "git",
            ["checkout", "--quiet", "--detach", revision.as_str()],
            GIT_SKIP_LFS_SMUDGE,
            Some(destination),
        )
        .with_context(|| {
            format!(
                "checking out deployment source {} at {revision}",
                source.name
            )
        })?;
        resolved.push(ResolvedSource {
            definition: source.clone(),
            repository: destination.to_path_buf(),
            revision,
            image_digests: BTreeMap::new(),
            deployment_image_digests: BTreeMap::new(),
            _checkout: checkout,
        });
    }
    Ok(resolved)
}

pub(crate) fn validate_locked_profile(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
) -> Result<()> {
    ensure!(
        lock.profile == profile.definition.name,
        "deployment lock is for profile {}, expected {}",
        lock.profile,
        profile.definition.name
    );
    ensure!(
        lock.registry == profile.definition.registry.locked(),
        "deployment lock registry endpoints do not match the profile"
    );
    let profile_revision = resolve_revision(&profile.repository, "HEAD")?;
    ensure!(
        lock.profile_revision == profile_revision,
        "deployment lock installation revision {} does not match checked-out installation revision {}",
        lock.profile_revision,
        profile_revision
    );
    validate_installation_inputs(profile)?;
    ensure!(
        lock.platform == profile.resolved_platform()?,
        "deployment lock platform selection does not match the profile"
    );
    ensure!(
        lock.sources.len() == profile.definition.sources.len(),
        "deployment lock contains {} sources, profile declares {}",
        lock.sources.len(),
        profile.definition.sources.len()
    );
    for source in &profile.definition.sources {
        let locked = lock
            .sources
            .iter()
            .find(|candidate| candidate.name == source.name)
            .with_context(|| format!("deployment lock omits profile source {}", source.name))?;
        ensure!(
            locked.role == source.role,
            "deployment lock role for source {} does not match the profile",
            source.name
        );
    }
    Ok(())
}

pub(crate) fn validate_installation_inputs(profile: &LoadedProfile) -> Result<()> {
    for path in profile.installation_inputs()? {
        let relative = path.strip_prefix(&profile.repository).with_context(|| {
            format!(
                "installation input {} is outside installation repository {}",
                path.display(),
                profile.repository.display()
            )
        })?;
        let tracked = Command::new("git")
            .args(["ls-files", "--error-unmatch", "--"])
            .arg(relative)
            .current_dir(&profile.repository)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .with_context(|| format!("checking tracked installation input {}", path.display()))?;
        ensure!(
            tracked.success(),
            "installation input {} is not tracked at profile revision",
            path.display()
        );
        let unchanged = Command::new("git")
            .args(["diff", "--quiet", "HEAD", "--"])
            .arg(relative)
            .current_dir(&profile.repository)
            .status()
            .with_context(|| format!("checking installation input {}", path.display()))?;
        ensure!(
            unchanged.success(),
            "installation input {} differs from locked profile revision",
            path.display()
        );
    }
    Ok(())
}

pub(crate) fn resolve_locked_sources(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
) -> Result<Vec<ResolvedSource>> {
    let mut resolved = Vec::with_capacity(profile.definition.sources.len());
    let deployment_image_digests = locked_image_digests(profile, &lock.sources)?;
    for source in &profile.definition.sources {
        let locked = lock
            .sources
            .iter()
            .find(|candidate| candidate.name == source.name)
            .with_context(|| format!("deployment lock omits source {}", source.name))?;
        let (clone_origin, source_origin) = match &source.repository {
            SourceRepository::Local { .. } => {
                let root = profile.local_source_root(source)?;
                let origin =
                    output_checked("git", ["config", "--get", "remote.origin.url"], Some(&root))
                        .with_context(|| {
                            format!("reading Git origin for deployment source {}", source.name)
                        })?;
                (
                    path_str(&root)?.to_owned(),
                    normalize_origin(String::from_utf8(origin)?.trim())?,
                )
            }
            SourceRepository::Git { url } => (url.clone(), normalize_origin(url)?),
        };
        ensure!(
            source_origin == locked.repository,
            "deployment lock repository for source {} is {}, profile resolves {}",
            source.name,
            locked.repository,
            source_origin
        );

        let checkout = tempfile::Builder::new()
            .prefix(&format!("veoveo-deployment-{}-", source.name))
            .tempdir()
            .with_context(|| format!("creating checkout for source {}", source.name))?;
        let destination = checkout.path();
        status_checked(
            "git",
            [
                "clone",
                "--quiet",
                "--no-checkout",
                clone_origin.as_str(),
                path_str(destination)?,
            ],
            GIT_SKIP_LFS_SMUDGE,
            None,
        )
        .with_context(|| format!("cloning deployment source {}", source.name))?;
        let revision = resolve_revision(destination, &locked.revision)?;
        ensure!(
            revision == locked.revision,
            "deployment source {} resolved locked revision {} to {}",
            source.name,
            locked.revision,
            revision
        );
        status_checked(
            "git",
            ["checkout", "--quiet", "--detach", revision.as_str()],
            GIT_SKIP_LFS_SMUDGE,
            Some(destination),
        )
        .with_context(|| {
            format!(
                "checking out deployment source {} at locked revision {revision}",
                source.name
            )
        })?;
        validate_locked_charts(source, locked, destination)?;
        resolved.push(ResolvedSource {
            definition: source.clone(),
            repository: destination.to_path_buf(),
            revision,
            image_digests: locked_image_digests(profile, std::slice::from_ref(locked))?,
            deployment_image_digests: deployment_image_digests.clone(),
            _checkout: checkout,
        });
    }
    Ok(resolved)
}

pub(crate) fn resolve_revision(repository: &Path, candidate: &str) -> Result<String> {
    let expression = format!("{candidate}^{{commit}}");
    let output = output_checked(
        "git",
        ["rev-parse", "--verify", expression.as_str()],
        Some(repository),
    )?;
    let revision = String::from_utf8(output)?.trim().to_owned();
    ensure!(
        revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "Git revision did not resolve to a full commit SHA"
    );
    Ok(revision)
}

pub(crate) fn repository_root(directory: &Path) -> Result<PathBuf> {
    let output = output_checked("git", ["rev-parse", "--show-toplevel"], Some(directory))?;
    Ok(PathBuf::from(String::from_utf8(output)?.trim()))
}

pub(crate) fn normalize_origin(origin: &str) -> Result<String> {
    let origin = origin.trim().trim_end_matches('/');
    ensure!(!origin.is_empty(), "Git origin cannot be empty");
    let expanded = if !origin.contains("://") {
        if let Some((authority, path)) = origin.split_once(':') {
            if authority.contains('@') && !path.starts_with('/') {
                format!("ssh://{authority}/{}", path.trim_start_matches('/'))
            } else {
                origin.to_owned()
            }
        } else {
            origin.to_owned()
        }
    } else {
        origin.to_owned()
    };
    if let Ok(mut url) = Url::parse(&expanded) {
        url.set_query(None);
        url.set_fragment(None);
        let normalized_path = url
            .path()
            .trim_end_matches('/')
            .strip_suffix(".git")
            .unwrap_or_else(|| url.path().trim_end_matches('/'))
            .to_owned();
        url.set_path(&normalized_path);
        return Ok(url.to_string().trim_end_matches('/').to_owned());
    }
    let path =
        fs::canonicalize(&expanded).with_context(|| format!("normalizing Git origin {origin}"))?;
    Ok(format!("file://{}", path.display()))
}
