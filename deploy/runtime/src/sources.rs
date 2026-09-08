use crate::{
    charts::lock_source_charts,
    process::{output_checked, path_str, status_checked},
    snapshot::SnapshotInputs,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};
use url::Url;
use veoveo_deploy_contract::components::{
    ComponentId, ComponentOwner, ComponentSource, selected_source_releases,
};
use veoveo_deploy_contract::{DeploymentLock, DeploymentSource, LoadedProfile, SourceRepository};
// Immutable source checkouts leave unrelated LFS objects as pointers.
const GIT_SKIP_LFS_SMUDGE: &[(&str, &str)] = &[("GIT_LFS_SKIP_SMUDGE", "1")];

#[derive(Debug)]
pub(crate) enum SourceCheckout {
    Temporary {
        _directory: tempfile::TempDir,
    },
    /// The synchronous lock compiler borrows a publisher-owned immutable snapshot.
    Publication,
}

#[derive(Debug)]
pub(crate) struct ResolvedSource {
    pub(crate) definition: DeploymentSource,
    pub(crate) repository: PathBuf,
    pub(crate) revision: String,
    pub(crate) _checkout: SourceCheckout,
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
        lock_source_charts(source, destination)?;
        resolved.push(ResolvedSource {
            definition: source.clone(),
            repository: destination.to_path_buf(),
            revision,
            _checkout: SourceCheckout::Temporary {
                _directory: checkout,
            },
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
    veoveo_deploy_contract::components::validate_profile_component_bindings(
        &profile.definition,
        lock,
    )?;
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
    let revision = resolve_revision(&profile.repository, "HEAD")?;
    let snapshot = SnapshotInputs::new(&profile.repository, &revision)?;
    for path in profile.installation_inputs()? {
        snapshot.file(&path)?;
    }
    Ok(())
}

pub(crate) fn resolve_locked_sources(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    selected: &BTreeSet<ComponentId>,
) -> Result<BTreeMap<ComponentSource, ResolvedSource>> {
    selected_source_releases(&profile.definition, selected)?;
    resolve_component_sources(profile, lock, selected, &BTreeMap::new())
}

/// Publication retains dependencies unchanged. Its caller validates the complete
/// catalog and exact requested IDs before opening these source repositories.
pub(crate) fn resolve_component_sources(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    selected: &BTreeSet<ComponentId>,
    revisions: &BTreeMap<String, veoveo_extension_contract::SourceRevision>,
) -> Result<BTreeMap<ComponentSource, ResolvedSource>> {
    let mut selected_releases = BTreeMap::<ComponentSource, BTreeSet<String>>::new();
    for spec in profile
        .definition
        .components
        .iter()
        .filter(|spec| selected.contains(&spec.id))
    {
        let ComponentOwner::Source { name } = &spec.owner else {
            continue;
        };
        let component = lock
            .components
            .iter()
            .find(|component| component.declaration.id == spec.id)
            .context("selected component has no locked source identity")?;
        ensure!(
            &component.declaration.source.name == name,
            "selected component source differs from the profile"
        );
        let mut identity = component.declaration.source.clone();
        if let Some(revision) = revisions.get(name) {
            identity.revision = revision.clone();
        }
        selected_releases
            .entry(identity)
            .or_default()
            .extend(spec.releases.iter().cloned());
    }
    let mut resolved = BTreeMap::new();
    for (identity, releases) in selected_releases {
        let source = profile
            .definition
            .sources
            .iter()
            .find(|source| source.name == identity.name)
            .context("selected component source is outside the profile")?;
        let mut source = source.clone();
        source
            .releases
            .retain(|release| releases.contains(&release.name));
        let locked = lock
            .sources
            .iter()
            .find(|candidate| candidate.name == source.name)
            .with_context(|| format!("deployment lock omits source {}", source.name))?;
        let (clone_origin, source_origin) = match &source.repository {
            SourceRepository::Local { .. } => {
                let root = profile.local_source_root(&source)?;
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
            source_origin == locked.repository && source_origin == identity.repository,
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
        let revision = resolve_revision(destination, identity.revision.as_str())?;
        ensure!(
            revision == identity.revision.as_str(),
            "deployment source {} resolved locked revision {} to {}",
            source.name,
            identity.revision,
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
        resolved.insert(
            identity,
            ResolvedSource {
                definition: source,
                repository: destination.to_path_buf(),
                revision,
                _checkout: SourceCheckout::Temporary {
                    _directory: checkout,
                },
            },
        );
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
