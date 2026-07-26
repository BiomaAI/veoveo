use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::{
    DEPLOYMENT_LOCK_SCHEMA, DeploymentLock, DeploymentSource, LoadedProfile, LockedChart,
    LockedImage, LockedSource, SourceRepository,
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
    validate_profile_image_closure(repository, &working_profile, &committed_profile, &registry)?;
    let mut locked_sources = Vec::new();
    for source in &committed_profile.definition.sources {
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
        let publication = PublicationSource::prepare(&source_repository, &source.revision)?;
        let selected_repository = RepositoryContext::discover(publication.path())?;
        let selections = source
            .image_groups
            .iter()
            .map(|group| Selection::group(group))
            .collect::<Result<Vec<_>>>()?;
        let images = publish_image_selections(
            repository,
            &selected_repository,
            publication.revision(),
            &registry,
            selections,
        )?;
        let charts = lock_source_charts(source, publication.path(), publication.revision())?;
        locked_sources.push(LockedSource {
            name: source.name.clone(),
            repository: source_repository.origin()?,
            revision: publication.revision().to_owned(),
            images,
            charts,
        });
    }
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

fn validate_profile_image_closure(
    repository: &RepositoryContext,
    working_profile: &LoadedProfile,
    committed_profile: &LoadedProfile,
    registry: &str,
) -> Result<()> {
    let environment = BTreeMap::from([
        ("VEOVEO_REGISTRY".to_owned(), registry.to_owned()),
        (
            "VEOVEO_IMAGE_TAG".to_owned(),
            "closure-validation".to_owned(),
        ),
    ]);
    let mut selected = BTreeSet::new();
    for source in &committed_profile.definition.sources {
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
        let publication = PublicationSource::prepare(&source_repository, &source.revision)?;
        let selected_repository = RepositoryContext::discover(publication.path())?;
        for group in &source.image_groups {
            let prepared =
                image::prepare(&selected_repository, Selection::group(group)?, &environment)
                    .with_context(|| {
                        format!(
                            "resolving image group {group} from deployment source {}",
                            source.name
                        )
                    })?;
            selected.extend(
                prepared
                    .image_references()
                    .into_iter()
                    .map(|(name, _)| name),
            );
        }
    }

    let required = committed_profile.required_platform_images()?;
    let missing = required.difference(&selected).cloned().collect::<Vec<_>>();
    ensure!(
        missing.is_empty(),
        "deployment profile {} omits required Veoveo image targets from its Bake groups: {}",
        committed_profile.definition.name,
        missing.join(", ")
    );
    Ok(())
}

fn publish_image_selections(
    evidence_repository: &RepositoryContext,
    source_repository: &RepositoryContext,
    revision: &str,
    registry: &str,
    selections: Vec<Selection>,
) -> Result<Vec<LockedImage>> {
    let environment = BTreeMap::from([
        ("VEOVEO_REGISTRY".to_owned(), registry.to_owned()),
        ("VEOVEO_IMAGE_TAG".to_owned(), revision.to_owned()),
    ]);
    let phase_count = selections.len();
    let mut references = BTreeMap::new();
    for (index, selection) in selections.into_iter().enumerate() {
        println!(
            "Publishing image phase {}/{}: {}",
            index + 1,
            phase_count,
            selection.name
        );
        let prepared = image::prepare(source_repository, selection, &environment)?;
        let evidence = image::evidence_run(evidence_repository, &prepared.plan, "release")?;
        image::execute(
            source_repository,
            &prepared,
            &environment,
            OutputMode::Push,
            &evidence,
        )?;
        for (name, reference) in prepared.image_references() {
            references.insert(name, reference);
        }
        println!("Release evidence: {}", evidence.directory().display());
    }
    references
        .into_iter()
        .map(|(name, reference)| {
            let repository = reference
                .strip_suffix(&format!(":{revision}"))
                .with_context(|| {
                    format!("published image {reference} does not use source revision tag")
                })?
                .to_owned();
            let digest = inspect_manifest_digest(&reference)?;
            Ok(LockedImage {
                name,
                repository,
                digest,
            })
        })
        .collect()
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

fn inspect_manifest_digest(reference: &str) -> Result<String> {
    let output = crate::process::output_text(
        "docker",
        [
            "buildx",
            "imagetools",
            "inspect",
            "--format",
            "{{json .Manifest.Digest}}",
            reference,
        ],
        None,
    )
    .with_context(|| format!("resolving OCI manifest digest for {reference}"))?;
    let digest = serde_json::from_str::<String>(output.trim())
        .with_context(|| format!("decoding OCI manifest digest for {reference}"))?;
    Ok(digest)
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
