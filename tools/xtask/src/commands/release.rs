use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::{
    DEPLOYMENT_LOCK_SCHEMA, DeploymentLock, DeploymentSource, LoadedProfile, LockedChart,
    LockedImage, LockedSource, PlannedImage, SourceRepository,
};

const IMAGE_RELEASE_EVIDENCE_SCHEMA: &str = "veoveo.io/image-release-evidence/v1";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageReleaseEvidence<'a> {
    schema_version: &'static str,
    source_revision: &'a str,
    registry: &'a str,
    images: &'a [LockedImage],
}

struct PreparedImagePhase {
    name: String,
    plan: image::PreparedPlan,
}

struct PreparedSourceRelease {
    definition: DeploymentSource,
    origin: String,
    revision: String,
    repository: RepositoryContext,
    phases: Vec<PreparedImagePhase>,
    images: Vec<PlannedImage>,
    charts: Vec<LockedChart>,
    _publication: Arc<PublicationSource>,
}

use crate::{
    ReleaseCompatibilityArgs, ReleaseHelmChartsArgs, ReleaseImagesArgs, ReleasePythonSdkArgs,
    ReleaseSimulationRuntimeArgs,
    commands::{
        builder, compatibility as compatibility_release, helm,
        image::{self, OutputMode, Selection},
        python, simulation,
        source::PublicationSource,
    },
    context::RepositoryContext,
};

pub(crate) fn simulation_runtime(
    repository: &RepositoryContext,
    args: &ReleaseSimulationRuntimeArgs,
) -> Result<()> {
    builder::ensure(repository)?;
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let output_root = if args.output_dir.is_absolute() {
        args.output_dir.clone()
    } else {
        repository.root().join(&args.output_dir)
    };
    let output = output_root.join(publication.revision()).join(&args.version);
    simulation::publish(
        repository,
        publication.path(),
        publication.revision(),
        args,
        &output,
    )?;
    println!("Simulation-runtime release bundle: {}", output.display());
    Ok(())
}

pub(crate) fn compatibility(
    repository: &RepositoryContext,
    args: &ReleaseCompatibilityArgs,
) -> Result<()> {
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let output_root = if args.output_dir.is_absolute() {
        args.output_dir.clone()
    } else {
        repository.root().join(&args.output_dir)
    };
    let output = output_root.join(publication.revision()).join(&args.release);
    compatibility_release::generate(
        repository.root(),
        publication.path(),
        publication.revision(),
        args,
        &output,
    )?;
    println!("Compatibility release bundle: {}", output.display());
    Ok(())
}

pub(crate) fn helm_charts(
    repository: &RepositoryContext,
    args: &ReleaseHelmChartsArgs,
) -> Result<()> {
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let output_root = if args.output_dir.is_absolute() {
        args.output_dir.clone()
    } else {
        repository.root().join(&args.output_dir)
    };
    let output = output_root.join(publication.revision()).join(&args.version);
    let mut release = helm::build(
        publication.path(),
        &output,
        &args.version,
        publication.revision(),
    )?;
    if let Some(registry) = &args.registry {
        helm::push(
            &mut release,
            registry,
            args.plain_http,
            &args.version,
            publication.revision(),
        )?;
    }
    println!("Helm chart release bundle: {}", output.display());
    Ok(())
}

pub(crate) fn python_sdk(
    repository: &RepositoryContext,
    args: &ReleasePythonSdkArgs,
) -> Result<()> {
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let artifacts = python::build_and_verify(publication.path())?;
    let output_root = if args.output_dir.is_absolute() {
        args.output_dir.clone()
    } else {
        repository.root().join(&args.output_dir)
    };
    let output = output_root.join(publication.revision());
    let published = python::write_release_bundle(&artifacts, publication.revision(), &output)?;

    if let Some(publish_url) = &args.publish_url {
        python::validate_publish_url(publish_url)?;
        if let Some(check_url) = &args.check_url {
            python::validate_index_url(check_url)?;
        }
        let mut publish_args = vec![
            OsString::from("publish"),
            OsString::from("--publish-url"),
            OsString::from(publish_url),
        ];
        if let Some(check_url) = &args.check_url {
            publish_args.push(OsString::from("--check-url"));
            publish_args.push(OsString::from(check_url));
        }
        if args.dry_run {
            publish_args.push(OsString::from("--dry-run"));
        }
        publish_args.extend(published.distributions.iter().map(OsString::from));
        crate::process::status("uv", publish_args, Some(repository.root()))?;
    }

    println!("Python SDK release bundle: {}", output.display());
    Ok(())
}

pub(crate) fn images(repository: &RepositoryContext, args: &ReleaseImagesArgs) -> Result<()> {
    builder::ensure(repository)?;
    match &args.profile {
        Some(profile_path) => release_profile_images(repository, profile_path, args),
        None => release_direct_images(repository, args),
    }
}

fn release_direct_images(repository: &RepositoryContext, args: &ReleaseImagesArgs) -> Result<()> {
    ensure!(
        args.profile_revision.is_none() && args.lock_output.is_none(),
        "direct image release does not accept profile-only arguments"
    );
    let revision = args
        .revision
        .as_deref()
        .context("a direct image release requires --revision")?;
    let publication = PublicationSource::prepare(repository, revision)?;
    let selected_repository = RepositoryContext::discover(publication.path())?;
    let selection = match (&args.target, &args.group) {
        (Some(target), None) => Selection::target(target)?,
        (None, Some(group)) => Selection::group(group)?,
        _ => bail!("release images requires --profile or exactly one --target/--group"),
    };
    let registry = args
        .registry
        .as_deref()
        .context("a direct target or group release requires --registry")?;
    validate_registry(registry)?;
    let images = publish_image_selections(
        repository,
        &selected_repository,
        publication.revision(),
        registry,
        vec![selection],
    )?;
    let output = args.evidence_output.clone().unwrap_or_else(|| {
        repository
            .root()
            .join("output/releases/images")
            .join(publication.revision())
            .join(format!(
                "{}.release-evidence.json",
                args.group
                    .as_deref()
                    .or(args.target.as_deref())
                    .expect("selection was validated")
            ))
    });
    write_json(
        &absolute_output(repository, &output),
        &ImageReleaseEvidence {
            schema_version: IMAGE_RELEASE_EVIDENCE_SCHEMA,
            source_revision: publication.revision(),
            registry,
            images: &images,
        },
    )?;
    println!(
        "Image release evidence: {}",
        absolute_output(repository, &output).display()
    );
    println!(
        "Published immutable revision {} to {}",
        publication.revision(),
        registry
    );
    Ok(())
}

fn release_profile_images(
    repository: &RepositoryContext,
    profile_path: &Path,
    args: &ReleaseImagesArgs,
) -> Result<()> {
    ensure!(
        args.target.is_none()
            && args.group.is_none()
            && args.registry.is_none()
            && args.revision.is_none()
            && args.evidence_output.is_none(),
        "--profile cannot be combined with direct image release arguments"
    );
    let profile_revision = args
        .profile_revision
        .as_deref()
        .context("a profile image release requires --profile-revision")?;
    let relative = profile_relative_path(repository, &profile_path.to_path_buf())?;
    let working_profile =
        LoadedProfile::load(&repository.root().join(&relative), repository.root())?;
    let committed_profile = {
        let publication = PublicationSource::prepare(repository, profile_revision)?;
        let selected = publication.path().join(&relative);
        let loaded = LoadedProfile::load(&selected, publication.path())?;
        ensure!(
            loaded.definition == working_profile.definition,
            "working deployment profile differs from committed profile revision {}; commit the profile before publication",
            publication.revision()
        );
        loaded
    };
    let registry = committed_profile.definition.registry.address.clone();
    validate_registry(&registry)?;
    let mut prepared_sources = Vec::new();
    let mut publications = BTreeMap::new();
    for source in &committed_profile.definition.sources {
        prepared_sources.push(prepare_profile_source(
            repository,
            &working_profile,
            source,
            &registry,
            &mut publications,
        )?);
    }
    let planned_images = prepared_sources
        .iter()
        .flat_map(|source| source.images.iter().cloned())
        .collect::<Vec<_>>();
    committed_profile.validate_image_plan(&planned_images)?;
    let locked_sources = prepared_sources
        .into_iter()
        .map(|source| publish_prepared_source(repository, &registry, source))
        .collect::<Result<Vec<_>>>()?;
    let lock = DeploymentLock {
        schema_version: DEPLOYMENT_LOCK_SCHEMA.to_owned(),
        profile: committed_profile.definition.name.clone(),
        registry,
        sources: locked_sources,
        platform: committed_profile.resolved_platform()?,
    };
    lock.validate()?;
    let output = args.lock_output.clone().unwrap_or_else(|| {
        repository
            .root()
            .join("output/releases/deployment")
            .join(&lock.profile)
            .join("deployment.lock.json")
    });
    let output = absolute_output(repository, &output);
    write_json(&output, &lock)?;
    println!("Deployment lock: {}", output.display());
    Ok(())
}

fn prepare_profile_source(
    repository: &RepositoryContext,
    working_profile: &LoadedProfile,
    source: &DeploymentSource,
    registry: &str,
    publications: &mut BTreeMap<(PathBuf, String), Arc<PublicationSource>>,
) -> Result<PreparedSourceRelease> {
    let source_root = match &source.repository {
        SourceRepository::Local { .. } => working_profile.local_source_root(source)?,
        SourceRepository::Git { url } => prepare_remote_repository(repository, url)?,
    };
    let source_repository = RepositoryContext::discover(&source_root).with_context(|| {
        format!(
            "discovering repository for deployment source {}",
            source.name
        )
    })?;
    let origin = source_repository.origin()?;
    let revision = super::source::resolve_revision(source_repository.root(), &source.revision)?;
    let identity = (
        fs::canonicalize(source_repository.root()).with_context(|| {
            format!(
                "resolving deployment source repository {}",
                source_repository.root().display()
            )
        })?,
        revision.clone(),
    );
    let publication = if let Some(publication) = publications.get(&identity) {
        publication.clone()
    } else {
        let publication = Arc::new(PublicationSource::prepare(&source_repository, &revision)?);
        publications.insert(identity, publication.clone());
        publication
    };
    let revision = publication.revision().to_owned();
    let selected_repository = RepositoryContext::discover(publication.path())?;
    let environment = publication_environment(registry, &revision);
    let mut phases = Vec::with_capacity(source.image_groups.len());
    let mut images = Vec::new();
    for group in &source.image_groups {
        let plan = image::prepare(&selected_repository, Selection::group(group)?, &environment)
            .with_context(|| {
                format!(
                    "resolving image group {group} from deployment source {}",
                    source.name
                )
            })?;
        for (target, reference) in plan.image_references() {
            validate_revision_reference(&reference, registry, &revision)?;
            images.push(PlannedImage {
                source: source.name.clone(),
                target,
                reference,
            });
        }
        phases.push(PreparedImagePhase {
            name: group.clone(),
            plan,
        });
    }
    let charts = lock_source_charts(source, publication.path(), &revision)?;
    Ok(PreparedSourceRelease {
        definition: source.clone(),
        origin,
        revision,
        repository: selected_repository,
        phases,
        images,
        charts,
        _publication: publication,
    })
}

fn publish_prepared_source(
    evidence_repository: &RepositoryContext,
    registry: &str,
    source: PreparedSourceRelease,
) -> Result<LockedSource> {
    let environment = publication_environment(registry, &source.revision);
    let phase_count = source.phases.len();
    let mut references = BTreeMap::new();
    for (index, phase) in source.phases.iter().enumerate() {
        println!(
            "Publishing source {} image phase {}/{}: {}",
            source.definition.name,
            index + 1,
            phase_count,
            phase.name
        );
        let evidence = image::evidence_run(evidence_repository, &phase.plan.plan, "release")?;
        image::execute(
            &source.repository,
            &phase.plan,
            &environment,
            OutputMode::Push,
            &evidence,
        )?;
        let digests = evidence.published_image_digests(&phase.plan)?;
        for (name, reference) in phase.plan.image_references() {
            let digest = digests
                .get(&name)
                .with_context(|| format!("Buildx metadata omitted published image {name}"))?;
            ensure!(
                references
                    .insert(name.clone(), (reference, digest.clone()))
                    .is_none(),
                "deployment source {} published image target {} more than once",
                source.definition.name,
                name
            );
        }
        println!("Release evidence: {}", evidence.directory().display());
    }
    let images = lock_published_images(references, &source.revision)?;
    Ok(LockedSource {
        name: source.definition.name,
        role: source.definition.role,
        repository: source.origin,
        revision: source.revision,
        images,
        charts: source.charts,
    })
}

fn publish_image_selections(
    evidence_repository: &RepositoryContext,
    source_repository: &RepositoryContext,
    revision: &str,
    registry: &str,
    selections: Vec<Selection>,
) -> Result<Vec<LockedImage>> {
    let environment = publication_environment(registry, revision);
    let phase_count = selections.len();
    let phases = selections
        .into_iter()
        .map(|selection| {
            let name = selection.name.clone();
            let plan = image::prepare(source_repository, selection, &environment)?;
            for (_, reference) in plan.image_references() {
                validate_revision_reference(&reference, registry, revision)?;
            }
            Ok(PreparedImagePhase { name, plan })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut references = BTreeMap::new();
    let mut published_references = BTreeSet::new();
    for phase in &phases {
        for (name, reference) in phase.plan.image_references() {
            ensure!(
                !references.contains_key(&name),
                "image target {name} is selected more than once"
            );
            ensure!(
                published_references.insert(reference.clone()),
                "image reference {reference} is selected more than once"
            );
            references.insert(name, (reference, String::new()));
        }
    }
    references.clear();
    for (index, phase) in phases.iter().enumerate() {
        println!(
            "Publishing image phase {}/{}: {}",
            index + 1,
            phase_count,
            phase.name
        );
        let evidence = image::evidence_run(evidence_repository, &phase.plan.plan, "release")?;
        image::execute(
            source_repository,
            &phase.plan,
            &environment,
            OutputMode::Push,
            &evidence,
        )?;
        let digests = evidence.published_image_digests(&phase.plan)?;
        for (name, reference) in phase.plan.image_references() {
            let digest = digests
                .get(&name)
                .with_context(|| format!("Buildx metadata omitted published image {name}"))?;
            ensure!(
                references
                    .insert(name.clone(), (reference, digest.clone()))
                    .is_none(),
                "image target {name} was published more than once"
            );
        }
        println!("Release evidence: {}", evidence.directory().display());
    }
    lock_published_images(references, revision)
}

fn lock_published_images(
    references: BTreeMap<String, (String, String)>,
    revision: &str,
) -> Result<Vec<LockedImage>> {
    references
        .into_iter()
        .map(|(name, (reference, digest))| {
            let repository = reference
                .strip_suffix(&format!(":{revision}"))
                .with_context(|| {
                    format!("published image {reference} does not use source revision tag")
                })?
                .to_owned();
            Ok(LockedImage {
                name,
                repository,
                digest,
            })
        })
        .collect()
}

fn publication_environment(registry: &str, revision: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("VEOVEO_REGISTRY".to_owned(), registry.to_owned()),
        ("VEOVEO_IMAGE_TAG".to_owned(), revision.to_owned()),
    ])
}

fn validate_revision_reference(reference: &str, registry: &str, revision: &str) -> Result<()> {
    ensure!(
        reference.starts_with(&format!("{registry}/")),
        "published image reference {reference} is outside selected registry {registry}"
    );
    ensure!(
        reference.ends_with(&format!(":{revision}")),
        "published image reference {reference} does not use source revision tag {revision}"
    );
    Ok(())
}

fn profile_relative_path(repository: &RepositoryContext, profile: &PathBuf) -> Result<PathBuf> {
    let candidate = if profile.is_absolute() {
        profile.clone()
    } else {
        repository.root().join(profile)
    };
    let candidate = fs::canonicalize(&candidate)
        .with_context(|| format!("resolving deployment profile {}", candidate.display()))?;
    candidate
        .strip_prefix(repository.root())
        .map(PathBuf::from)
        .with_context(|| {
            format!(
                "deployment profile {} is outside repository {}",
                candidate.display(),
                repository.root().display()
            )
        })
}

fn prepare_remote_repository(repository: &RepositoryContext, url: &str) -> Result<PathBuf> {
    let normalized = crate::commands::source::normalize_origin(url)?;
    let identity = hex::encode(Sha256::digest(normalized.as_bytes()));
    let directory = repository
        .root()
        .join("target/veoveo-xtask/remotes")
        .join(identity);
    let checkout = directory.join("repository");
    fs::create_dir_all(&directory)
        .with_context(|| format!("creating remote source cache {}", directory.display()))?;
    if checkout.exists() {
        ensure!(
            checkout.join(".git").is_dir(),
            "remote source cache {} is not a Git repository",
            checkout.display()
        );
        let context = RepositoryContext::discover(&checkout)?;
        ensure!(
            context.origin()? == normalized,
            "remote source cache origin differs from profile source {normalized}"
        );
        crate::process::status("git", ["fetch", "--prune", "origin"], Some(&checkout))?;
    } else {
        crate::process::status(
            "git",
            [
                "clone",
                "--no-checkout",
                normalized.as_str(),
                path_text(&checkout)?,
            ],
            Some(repository.root()),
        )?;
    }
    Ok(checkout)
}

fn lock_source_charts(
    source: &DeploymentSource,
    repository: &Path,
    revision: &str,
) -> Result<Vec<LockedChart>> {
    let mut releases = BTreeSet::new();
    source
        .releases
        .iter()
        .map(|release| {
            ensure!(
                releases.insert(release.name.clone()),
                "duplicate Helm release {}",
                release.name
            );
            let archive = Command::new("git")
                .args([
                    "archive",
                    "--format=tar",
                    revision,
                    path_text(&release.chart)?,
                ])
                .current_dir(repository)
                .output()
                .with_context(|| {
                    format!(
                        "archiving chart {} from source {}",
                        release.chart.display(),
                        source.name
                    )
                })?;
            ensure!(
                archive.status.success(),
                "git archive failed for chart {}:\n{}",
                release.chart.display(),
                String::from_utf8_lossy(&archive.stderr)
            );
            Ok(LockedChart {
                release: release.name.clone(),
                coordinate: format!(
                    "source://{}/{}",
                    source.name,
                    release.chart.to_string_lossy()
                ),
                digest: format!("sha256:{}", hex::encode(Sha256::digest(&archive.stdout))),
            })
        })
        .collect()
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path
        .parent()
        .context("deployment lock output has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating deployment lock directory {}", parent.display()))?;
    let temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating deployment lock beside {}", path.display()))?;
    let mut file = temporary;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing deployment lock {}", path.display()))?;
    Ok(())
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn absolute_output(repository: &RepositoryContext, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repository.root().join(path)
    }
}

fn validate_registry(registry: &str) -> Result<()> {
    ensure!(!registry.trim().is_empty(), "registry cannot be empty");
    ensure!(
        !registry.contains("://"),
        "registry must not include a URL scheme"
    );
    ensure!(
        !registry.ends_with('/'),
        "registry must not end with a slash"
    );
    ensure!(
        !registry.chars().any(char::is_whitespace),
        "registry must not contain whitespace"
    );
    Ok(())
}
