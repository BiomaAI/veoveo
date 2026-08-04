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
    DEPLOYMENT_LOCK_SCHEMA, DEVELOPMENT_IMAGE_LOCK_SCHEMA, DeploymentLock, DeploymentSource,
    DeploymentSourceRole, DevelopmentImageLock, DevelopmentImageOrigin, DevelopmentLockedImage,
    LoadedProfile, LockedChart, LockedImage, LockedSource, PlannedImage, RegistryTransport,
    SourceRepository,
};

const IMAGE_RELEASE_EVIDENCE_SCHEMA: &str = "veoveo.io/image-release-evidence/v1";
const IMAGE_STAGE_EVIDENCE_SCHEMA: &str = "veoveo.io/image-stage-evidence/v1";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageReleaseEvidence<'a> {
    schema_version: &'static str,
    source_revision: &'a str,
    registry: &'a str,
    images: &'a [LockedImage],
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageStageEvidence<'a> {
    schema_version: &'static str,
    source_revision: &'a str,
    registry: &'a str,
    release_eligible: bool,
    images: &'a [StagedImage],
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedImage {
    target: String,
    repository: String,
    runtime_digest: String,
    staging_index_digest: String,
    platform: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedImageInput {
    target: String,
    repository: String,
    runtime_digest: String,
    staging_index_digest: String,
    platform: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageStageEvidenceInput {
    schema_version: String,
    source_revision: String,
    registry: String,
    release_eligible: bool,
    images: Vec<StagedImageInput>,
}

struct PreparedImagePhase {
    name: String,
    plan: image::PreparedPlan,
}

struct PublishedImageOutput {
    reference: String,
    publication_digest: String,
    platform: String,
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
    ImageDevelopmentLockArgs, ImageStageArgs, ReleaseCompatibilityArgs, ReleaseHelmChartsArgs,
    ReleaseImagesArgs, ReleasePythonSdkArgs, ReleaseSimulationRuntimeArgs,
    commands::{
        builder, compatibility as compatibility_release, helm,
        image::{self, OutputMode, Selection},
        image_manifest, python, simulation,
        source::PublicationSource,
    },
    context::RepositoryContext,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DevelopmentHelmValues {
    global: DevelopmentGlobalValues,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DevelopmentGlobalValues {
    veoveo_registry: String,
    image_digests: BTreeMap<String, String>,
}

pub(crate) fn simulation_runtime(
    repository: &RepositoryContext,
    args: &ReleaseSimulationRuntimeArgs,
) -> Result<()> {
    let lock_path = if args.deployment_lock.is_absolute() {
        args.deployment_lock.clone()
    } else {
        repository.root().join(&args.deployment_lock)
    };
    let lock_bytes = fs::read(&lock_path)
        .with_context(|| format!("reading deployment lock {}", lock_path.display()))?;
    let lock: DeploymentLock = serde_json::from_slice(&lock_bytes)
        .with_context(|| format!("decoding deployment lock {}", lock_path.display()))?;
    lock.validate()?;
    let _builder =
        builder::ensure_for_registry(repository, &lock.registry, lock.registry_transport)?;
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let platform_source = lock
        .sources
        .iter()
        .find(|source| source.role == DeploymentSourceRole::Platform)
        .context("deployment lock has no platform source")?;
    ensure!(
        platform_source.revision == publication.revision(),
        "deployment lock platform revision {} does not match selected simulation revision {}",
        platform_source.revision,
        publication.revision()
    );
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
        &lock.registry,
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
    match &args.profile {
        Some(profile_path) => release_profile_images(repository, profile_path, args),
        None => release_direct_images(repository, args),
    }
}

pub(crate) fn stage_images(repository: &RepositoryContext, args: &ImageStageArgs) -> Result<()> {
    let selection = Selection::from_args(&args.selection)?;
    validate_registry(&args.registry)?;
    let allow_insecure_registry = registry_is_loopback(&args.registry)?;
    let transport = if allow_insecure_registry {
        RegistryTransport::InsecureHttp
    } else {
        RegistryTransport::Tls
    };
    let _builder = builder::ensure_for_registry(repository, &args.registry, transport)?;
    let publication = PublicationSource::prepare(repository, &args.revision)?;
    let source_repository = RepositoryContext::discover(publication.path())?;
    let environment = publication_environment(&args.registry, publication.revision());
    let prepared =
        image::prepare_with_builder(repository, &source_repository, selection, &environment)?;
    for (_, reference) in prepared.image_references() {
        validate_revision_reference(&reference, &args.registry, publication.revision())?;
    }
    let evidence = image::evidence_run(repository, &prepared.plan, "stage")?;
    image::execute(
        repository,
        &source_repository,
        &prepared,
        &environment,
        OutputMode::Staged,
        &evidence,
    )?;
    let staging_digests = evidence.publication_index_digests(&prepared)?;
    let staged = prepared
        .image_outputs()
        .into_iter()
        .map(|output| {
            let staging_index_digest = staging_digests
                .get(&output.target)
                .with_context(|| format!("Buildx metadata omitted staged image {}", output.target))?
                .clone();
            let repository = output
                .reference
                .strip_suffix(&format!(":{}", publication.revision()))
                .with_context(|| {
                    format!(
                        "staged image {} does not use the source revision tag",
                        output.reference
                    )
                })?
                .to_owned();
            let digests = image_manifest::inspect_staged(
                &repository,
                &staging_index_digest,
                &output.platform,
                allow_insecure_registry,
            )?;
            Ok(StagedImage {
                target: output.target,
                repository,
                runtime_digest: digests.runtime,
                staging_index_digest,
                platform: output.platform,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let output = args
        .evidence_output
        .as_ref()
        .map(|path| absolute_output(repository, path))
        .unwrap_or_else(|| evidence.directory().join("staging.json"));
    write_create_only_json(
        &output,
        &ImageStageEvidence {
            schema_version: IMAGE_STAGE_EVIDENCE_SCHEMA,
            source_revision: publication.revision(),
            registry: &args.registry,
            release_eligible: false,
            images: &staged,
        },
    )?;
    println!("Staged image evidence: {}", output.display());
    println!(
        "Staged runtime images are immutable development inputs and are not release-qualified"
    );
    Ok(())
}

pub(crate) fn development_image_lock(
    repository: &RepositoryContext,
    args: &ImageDevelopmentLockArgs,
) -> Result<()> {
    let base_path = absolute_output(repository, &args.base_lock);
    let base_bytes = fs::read(&base_path)
        .with_context(|| format!("reading qualified deployment lock {}", base_path.display()))?;
    let base: DeploymentLock = serde_json::from_slice(&base_bytes)
        .with_context(|| format!("decoding qualified deployment lock {}", base_path.display()))?;
    base.validate()?;

    let mut images = base
        .sources
        .iter()
        .flat_map(|source| {
            source.images.iter().map(|image| DevelopmentLockedImage {
                source: source.name.clone(),
                target: image.name.clone(),
                repository: image.repository.clone(),
                source_revision: source.revision.clone(),
                runtime_digest: image.digest.clone(),
                origin: DevelopmentImageOrigin::Qualified {
                    publication_digest: image.publication_digest.clone(),
                },
            })
        })
        .collect::<Vec<_>>();
    let mut replaced = BTreeSet::new();
    for evidence_path in &args.stage_evidence {
        let evidence_path = absolute_output(repository, evidence_path);
        let evidence_bytes = fs::read(&evidence_path)
            .with_context(|| format!("reading stage evidence {}", evidence_path.display()))?;
        let evidence: ImageStageEvidenceInput = serde_json::from_slice(&evidence_bytes)
            .with_context(|| format!("decoding stage evidence {}", evidence_path.display()))?;
        ensure!(
            evidence.schema_version == IMAGE_STAGE_EVIDENCE_SCHEMA,
            "stage evidence {} uses unsupported schema {}",
            evidence_path.display(),
            evidence.schema_version
        );
        ensure!(
            !evidence.release_eligible,
            "stage evidence {} must declare releaseEligible=false",
            evidence_path.display()
        );
        ensure!(
            evidence.registry == base.registry,
            "stage evidence registry {} does not match qualified base registry {}",
            evidence.registry,
            base.registry
        );
        ensure!(
            !evidence.images.is_empty(),
            "stage evidence {} contains no images",
            evidence_path.display()
        );
        let evidence_digest = format!("sha256:{}", hex::encode(Sha256::digest(&evidence_bytes)));
        for staged in evidence.images {
            ensure!(
                staged.platform == "linux/amd64",
                "staged image {} uses unsupported platform {}",
                staged.target,
                staged.platform
            );
            validate_oci_digest(&staged.runtime_digest, "staged runtime")?;
            validate_oci_digest(&staged.staging_index_digest, "staging index")?;
            let index = images
                .iter()
                .position(|base_image| {
                    base_image.target == staged.target && base_image.repository == staged.repository
                })
                .with_context(|| {
                    format!(
                        "staged image {} at {} is absent from the qualified base closure",
                        staged.target, staged.repository
                    )
                })?;
            ensure!(
                replaced.insert(staged.repository.clone()),
                "staged repository {} is supplied more than once",
                staged.repository
            );
            images[index].source_revision = evidence.source_revision.clone();
            images[index].runtime_digest = staged.runtime_digest;
            images[index].origin = DevelopmentImageOrigin::Staged {
                staging_index_digest: staged.staging_index_digest,
                stage_evidence_digest: evidence_digest.clone(),
            };
        }
    }
    ensure!(
        !replaced.is_empty(),
        "development image lock requires at least one staged replacement"
    );
    images.sort_by(|left, right| left.repository.cmp(&right.repository));
    let lock = DevelopmentImageLock {
        schema_version: DEVELOPMENT_IMAGE_LOCK_SCHEMA.to_owned(),
        release_eligible: false,
        base_deployment_lock_digest: format!("sha256:{}", hex::encode(Sha256::digest(&base_bytes))),
        registry: base.registry.clone(),
        images,
    };
    lock.validate()?;

    let registry_prefix = format!("{}/", lock.registry);
    let image_digests = lock
        .images
        .iter()
        .map(|image| {
            let repository = image
                .repository
                .strip_prefix(&registry_prefix)
                .expect("development lock validation checked registry ownership")
                .to_owned();
            (repository, image.runtime_digest.clone())
        })
        .collect::<BTreeMap<_, _>>();
    let values = DevelopmentHelmValues {
        global: DevelopmentGlobalValues {
            veoveo_registry: lock.registry.clone(),
            image_digests,
        },
    };
    let lock_output = absolute_output(repository, &args.output);
    let values_output = absolute_output(repository, &args.values_output);
    ensure!(
        lock_output != values_output,
        "development lock and Helm values outputs must be distinct"
    );
    write_json(&lock_output, &lock)?;
    write_json(&values_output, &values)?;
    println!("Development image lock: {}", lock_output.display());
    println!("Development Helm values: {}", values_output.display());
    println!(
        "Merged {} staged repositories into a {}-image non-release closure",
        replaced.len(),
        lock.images.len()
    );
    Ok(())
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
    let allow_insecure_registry = registry_is_loopback(registry)?;
    let transport = if allow_insecure_registry {
        RegistryTransport::InsecureHttp
    } else {
        RegistryTransport::Tls
    };
    let _builder = builder::ensure_for_registry(repository, registry, transport)?;
    let publication = PublicationSource::prepare(repository, revision)?;
    let selected_repository = RepositoryContext::discover(publication.path())?;
    let images = publish_image_selections(
        repository,
        &selected_repository,
        publication.revision(),
        registry,
        vec![selection],
        allow_insecure_registry,
    )?;
    if let Some(stage_evidence) = &args.stage_evidence {
        validate_qualified_runtime_identity(
            repository,
            stage_evidence,
            publication.revision(),
            registry,
            &images,
        )?;
    }
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
            && args.evidence_output.is_none()
            && args.stage_evidence.is_none(),
        "--profile cannot be combined with direct image release arguments"
    );
    let profile_revision = args
        .profile_revision
        .as_deref()
        .context("a profile image release requires --profile-revision")?;
    let (profile_repository, profile_path, relative) = profile_location(repository, profile_path)?;
    let working_profile = LoadedProfile::load(&profile_path, profile_repository.root())?;
    let working_registry = &working_profile.definition.registry;
    validate_registry(&working_registry.address)?;
    let _builder = builder::ensure_for_registry(
        repository,
        &working_registry.address,
        working_registry.transport,
    )?;
    let profile_publication = Arc::new(PublicationSource::prepare(
        &profile_repository,
        profile_revision,
    )?);
    let committed_profile = {
        let publication = &profile_publication;
        let selected = publication.path().join(&relative);
        let loaded = LoadedProfile::load(&selected, publication.path())?;
        ensure!(
            loaded.definition == working_profile.definition,
            "working deployment profile differs from committed profile revision {}; commit the profile before publication",
            publication.revision()
        );
        loaded
    };
    validate_installation_inputs(
        &working_profile,
        &committed_profile,
        profile_publication.revision(),
    )?;
    let registry = committed_profile.definition.registry.address.clone();
    validate_registry(&registry)?;
    let registry_transport = committed_profile.definition.registry.transport;
    let allow_insecure_registry = registry_transport.is_insecure();
    let platform_targets = committed_profile.required_platform_images()?;
    let mut prepared_sources = Vec::new();
    let mut publications = BTreeMap::new();
    let profile_identity = (
        fs::canonicalize(profile_repository.root()).with_context(|| {
            format!(
                "resolving installation repository {}",
                profile_repository.root().display()
            )
        })?,
        profile_publication.revision().to_owned(),
    );
    publications.insert(profile_identity, profile_publication.clone());
    for source in &committed_profile.definition.sources {
        prepared_sources.push(prepare_profile_source(
            repository,
            &working_profile,
            source,
            &registry,
            &platform_targets,
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
        .map(|source| {
            publish_prepared_source(repository, &registry, source, allow_insecure_registry)
        })
        .collect::<Result<Vec<_>>>()?;
    let lock = DeploymentLock {
        schema_version: DEPLOYMENT_LOCK_SCHEMA.to_owned(),
        profile: committed_profile.definition.name.clone(),
        profile_revision: profile_publication.revision().to_owned(),
        registry,
        registry_transport,
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
    platform_targets: &BTreeSet<String>,
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
        ensure!(
            !publications
                .keys()
                .any(|(repository, _)| repository == &identity.0),
            "deployment source {} selects a different revision of repository {} already used by this profile; one publication may pin a repository only once",
            source.name,
            source_repository.root().display()
        );
        let publication = Arc::new(PublicationSource::prepare(&source_repository, &revision)?);
        publications.insert(identity, publication.clone());
        publication
    };
    let revision = publication.revision().to_owned();
    let selected_repository = RepositoryContext::discover(publication.path())?;
    let environment = publication_environment(registry, &revision);
    let selections = match source.role {
        DeploymentSourceRole::Platform => vec![(
            "exact-platform".to_owned(),
            Selection::exact("exact-platform", platform_targets.iter().cloned())?,
        )],
        DeploymentSourceRole::Extension | DeploymentSourceRole::Workload => source
            .image_groups
            .iter()
            .map(|group| Ok((group.clone(), Selection::group(group)?)))
            .collect::<Result<Vec<_>>>()?,
    };
    let mut phases = Vec::with_capacity(selections.len());
    let mut images = Vec::new();
    for (name, selection) in selections {
        let plan =
            image::prepare_with_builder(repository, &selected_repository, selection, &environment)
                .with_context(|| {
                    format!(
                        "resolving image phase {name} from deployment source {}",
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
        phases.push(PreparedImagePhase { name, plan });
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
    allow_insecure_registry: bool,
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
            evidence_repository,
            &source.repository,
            &phase.plan,
            &environment,
            OutputMode::Qualified,
            &evidence,
        )?;
        let digests = evidence.publication_index_digests(&phase.plan)?;
        for output in phase.plan.image_outputs() {
            let digest = digests.get(&output.target).with_context(|| {
                format!("Buildx metadata omitted published image {}", output.target)
            })?;
            ensure!(
                references
                    .insert(
                        output.target.clone(),
                        PublishedImageOutput {
                            reference: output.reference,
                            publication_digest: digest.clone(),
                            platform: output.platform,
                        },
                    )
                    .is_none(),
                "deployment source {} published image target {} more than once",
                source.definition.name,
                output.target
            );
        }
        println!("Release evidence: {}", evidence.directory().display());
    }
    let images = lock_published_images(references, &source.revision, allow_insecure_registry)?;
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
    allow_insecure_registry: bool,
) -> Result<Vec<LockedImage>> {
    let environment = publication_environment(registry, revision);
    let phase_count = selections.len();
    let phases = selections
        .into_iter()
        .map(|selection| {
            let name = selection.name.clone();
            let plan = image::prepare_with_builder(
                evidence_repository,
                source_repository,
                selection,
                &environment,
            )?;
            for (_, reference) in plan.image_references() {
                validate_revision_reference(&reference, registry, revision)?;
            }
            Ok(PreparedImagePhase { name, plan })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut selected_targets = BTreeSet::new();
    let mut published_references = BTreeSet::new();
    for phase in &phases {
        for output in phase.plan.image_outputs() {
            ensure!(
                selected_targets.insert(output.target.clone()),
                "image target {} is selected more than once",
                output.target
            );
            ensure!(
                published_references.insert(output.reference.clone()),
                "image reference {} is selected more than once",
                output.reference
            );
        }
    }
    let mut references = BTreeMap::new();
    for (index, phase) in phases.iter().enumerate() {
        println!(
            "Publishing image phase {}/{}: {}",
            index + 1,
            phase_count,
            phase.name
        );
        let evidence = image::evidence_run(evidence_repository, &phase.plan.plan, "release")?;
        image::execute(
            evidence_repository,
            source_repository,
            &phase.plan,
            &environment,
            OutputMode::Qualified,
            &evidence,
        )?;
        let digests = evidence.publication_index_digests(&phase.plan)?;
        for output in phase.plan.image_outputs() {
            let digest = digests.get(&output.target).with_context(|| {
                format!("Buildx metadata omitted published image {}", output.target)
            })?;
            ensure!(
                references
                    .insert(
                        output.target.clone(),
                        PublishedImageOutput {
                            reference: output.reference,
                            publication_digest: digest.clone(),
                            platform: output.platform,
                        },
                    )
                    .is_none(),
                "image target {} was published more than once",
                output.target
            );
        }
        println!("Release evidence: {}", evidence.directory().display());
    }
    lock_published_images(references, revision, allow_insecure_registry)
}

fn lock_published_images(
    references: BTreeMap<String, PublishedImageOutput>,
    revision: &str,
    allow_insecure_registry: bool,
) -> Result<Vec<LockedImage>> {
    references
        .into_iter()
        .map(|(name, output)| {
            let PublishedImageOutput {
                reference,
                publication_digest,
                platform,
            } = output;
            let repository = reference
                .strip_suffix(&format!(":{revision}"))
                .with_context(|| {
                    format!("published image {reference} does not use source revision tag")
                })?
                .to_owned();
            let digests = image_manifest::inspect(
                &repository,
                &publication_digest,
                &platform,
                allow_insecure_registry,
            )?;
            Ok(LockedImage {
                name,
                repository,
                digest: digests.runtime,
                publication_digest: digests.publication,
            })
        })
        .collect()
}

fn validate_qualified_runtime_identity(
    repository: &RepositoryContext,
    stage_evidence: &Path,
    revision: &str,
    registry: &str,
    qualified: &[LockedImage],
) -> Result<()> {
    let path = absolute_output(repository, stage_evidence);
    let bytes = fs::read(&path)
        .with_context(|| format!("reading staged image evidence {}", path.display()))?;
    let staged: ImageStageEvidenceInput = serde_json::from_slice(&bytes)
        .with_context(|| format!("decoding staged image evidence {}", path.display()))?;
    ensure!(
        staged.schema_version == IMAGE_STAGE_EVIDENCE_SCHEMA,
        "staged image evidence uses unsupported schema {}",
        staged.schema_version
    );
    ensure!(
        !staged.release_eligible,
        "staged image evidence must declare releaseEligible=false"
    );
    ensure!(
        staged.source_revision == revision,
        "staged source revision {} does not match qualified revision {revision}",
        staged.source_revision
    );
    ensure!(
        staged.registry == registry,
        "staged registry {} does not match qualified registry {registry}",
        staged.registry
    );
    let staged = staged
        .images
        .into_iter()
        .map(|image| {
            ensure!(
                image.platform == "linux/amd64",
                "staged image {} uses unsupported platform {}",
                image.target,
                image.platform
            );
            validate_oci_digest(&image.runtime_digest, "staged runtime")?;
            validate_oci_digest(&image.staging_index_digest, "staging index")?;
            Ok((image.target.clone(), image))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    ensure!(
        staged.len() == qualified.len(),
        "staged evidence contains {} images while qualification produced {}",
        staged.len(),
        qualified.len()
    );
    for image in qualified {
        let prior = staged
            .get(&image.name)
            .with_context(|| format!("staged evidence omits qualified image {}", image.name))?;
        ensure!(
            prior.repository == image.repository,
            "qualified image {} changed repository from {} to {}",
            image.name,
            prior.repository,
            image.repository
        );
        ensure!(
            prior.runtime_digest == image.digest,
            "qualified image {} changed runnable digest from {} to {}; qualification may attach attestations but must not rebuild runtime identity",
            image.name,
            prior.runtime_digest,
            image.digest
        );
    }
    Ok(())
}

fn validate_oci_digest(value: &str, kind: &str) -> Result<()> {
    let digest = value
        .strip_prefix("sha256:")
        .with_context(|| format!("{kind} digest must start with sha256:"))?;
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{kind} digest must contain 64 lowercase hexadecimal digits"
    );
    Ok(())
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

fn validate_installation_inputs(
    working: &LoadedProfile,
    committed: &LoadedProfile,
    revision: &str,
) -> Result<()> {
    let fingerprints = |profile: &LoadedProfile| -> Result<BTreeMap<PathBuf, String>> {
        profile
            .installation_inputs()?
            .into_iter()
            .map(|path| {
                let relative = path
                    .strip_prefix(&profile.repository)
                    .map(PathBuf::from)
                    .with_context(|| {
                        format!(
                            "installation input {} is outside installation repository {}",
                            path.display(),
                            profile.repository.display()
                        )
                    })?;
                let digest =
                    hex::encode(Sha256::digest(fs::read(&path).with_context(|| {
                        format!("reading installation input {}", path.display())
                    })?));
                Ok((relative, digest))
            })
            .collect()
    };
    ensure!(
        fingerprints(working)? == fingerprints(committed)?,
        "working installation inputs differ from committed profile revision {}; commit the profile and its referenced installation files before publication",
        revision
    );
    Ok(())
}

fn profile_location(
    invocation_repository: &RepositoryContext,
    profile: &Path,
) -> Result<(RepositoryContext, PathBuf, PathBuf)> {
    let candidate = if profile.is_absolute() {
        profile.to_path_buf()
    } else {
        invocation_repository.root().join(profile)
    };
    let candidate = fs::canonicalize(&candidate)
        .with_context(|| format!("resolving deployment profile {}", candidate.display()))?;
    let parent = candidate
        .parent()
        .context("deployment profile has no parent directory")?;
    let repository = RepositoryContext::discover(parent).with_context(|| {
        format!(
            "discovering installation repository for deployment profile {}",
            candidate.display()
        )
    })?;
    let relative = candidate
        .strip_prefix(repository.root())
        .map(PathBuf::from)
        .with_context(|| {
            format!(
                "deployment profile {} is outside repository {}",
                candidate.display(),
                repository.root().display()
            )
        })?;
    Ok((repository, candidate, relative))
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
            for values in &release.source_values {
                ensure!(
                    repository.join(values).is_file(),
                    "source-owned Helm values for release {} do not exist at {}",
                    release.name,
                    repository.join(values).display()
                );
            }
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

fn write_create_only_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path
        .parent()
        .context("evidence output has no parent directory")?;
    fs::create_dir_all(parent)
        .with_context(|| format!("creating evidence directory {}", parent.display()))?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("creating immutable evidence {}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
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
    let url = url::Url::parse(&format!("https://{registry}"))
        .with_context(|| format!("invalid registry address {registry}"))?;
    ensure!(
        url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "registry must contain only a host and optional port"
    );
    Ok(())
}

fn registry_is_loopback(registry: &str) -> Result<bool> {
    let parsed = url::Url::parse(&format!("http://{registry}"))
        .with_context(|| format!("parsing registry address {registry}"))?;
    let host = parsed
        .host_str()
        .context("registry address must contain a host")?;
    Ok(host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback()))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use tempfile::tempdir;
    use veoveo_deploy_contract::{DevelopmentImageLock, DevelopmentImageOrigin};

    use super::{development_image_lock, profile_location};
    use crate::{ImageDevelopmentLockArgs, context::RepositoryContext};

    const STAGED_RUNTIME: &str =
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const STAGED_INDEX: &str =
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn git_init(path: &Path) {
        fs::create_dir(path).expect("create repository");
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .expect("initialize repository");
        assert!(status.success());
    }

    #[test]
    fn profile_location_discovers_a_separate_installation_repository() {
        let temporary = tempdir().expect("create temporary repositories");
        let invocation = temporary.path().join("veoveo");
        let installation = temporary.path().join("installation");
        git_init(&invocation);
        git_init(&installation);
        let profile = installation.join("environments/example/deployment.json");
        fs::create_dir_all(profile.parent().expect("profile parent"))
            .expect("create profile directory");
        fs::write(&profile, "{}\n").expect("write profile");
        let invocation =
            RepositoryContext::discover(&invocation).expect("discover invocation repository");

        let (repository, resolved, relative) =
            profile_location(&invocation, &profile).expect("resolve external profile");

        assert_eq!(
            fs::canonicalize(repository.root()).unwrap(),
            fs::canonicalize(&installation).unwrap()
        );
        assert_eq!(resolved, fs::canonicalize(&profile).unwrap());
        assert_eq!(relative, Path::new("environments/example/deployment.json"));
    }

    #[test]
    fn development_lock_replaces_only_staged_runtime_identity() {
        let repository = RepositoryContext::discover(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("discover repository");
        let temporary = tempdir().expect("create output directory");
        let base = repository
            .root()
            .join("testing/fixtures/external-simulation-installation/deployment.lock.json");
        let stage = temporary.path().join("stage.json");
        fs::write(
            &stage,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": "veoveo.io/image-stage-evidence/v1",
                "sourceRevision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "registry": "k3d-veoveo-registry.localhost:5001",
                "releaseEligible": false,
                "images": [{
                    "target": "simulation-view-mcp",
                    "repository": "k3d-veoveo-registry.localhost:5001/veoveo/simulation-view-mcp",
                    "runtimeDigest": STAGED_RUNTIME,
                    "stagingIndexDigest": STAGED_INDEX,
                    "platform": "linux/amd64"
                }]
            }))
            .unwrap(),
        )
        .expect("write stage evidence");
        let output = temporary.path().join("development.lock.json");
        let values = temporary.path().join("development.values.json");
        development_image_lock(
            &repository,
            &ImageDevelopmentLockArgs {
                base_lock: base,
                stage_evidence: vec![stage],
                output: output.clone(),
                values_output: values.clone(),
            },
        )
        .expect("build development closure");

        let lock: DevelopmentImageLock =
            serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        lock.validate().unwrap();
        let replaced = lock
            .images
            .iter()
            .find(|image| image.target == "simulation-view-mcp")
            .unwrap();
        assert_eq!(replaced.runtime_digest, STAGED_RUNTIME);
        assert!(matches!(
            replaced.origin,
            DevelopmentImageOrigin::Staged { .. }
        ));
        let unchanged = lock
            .images
            .iter()
            .find(|image| image.target == "artifact-mcp")
            .unwrap();
        assert!(matches!(
            unchanged.origin,
            DevelopmentImageOrigin::Qualified { .. }
        ));
        let values: serde_json::Value = serde_json::from_slice(&fs::read(values).unwrap()).unwrap();
        assert_eq!(
            values["global"]["imageDigests"]["veoveo/simulation-view-mcp"],
            STAGED_RUNTIME
        );
    }
}
