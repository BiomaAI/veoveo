use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use veoveo_deploy_contract::{DeploymentLock, LoadedProfile, LockedSource, components::*};
use veoveo_extension_contract::{ArtifactDigest, SourceRevision};

use crate::{
    charts::helm_render_locked,
    configuration::{
        append_yaml_bytes, append_yaml_objects, config_map_manifest, gateway_activation_manifest,
        prepare_gateway_activation,
    },
    gpu::{prepare_gpu_allocator_objects, prepare_gpu_placement},
    helm_bundle::{ChartMetadata, CompiledHelmRelease, preserve_crds},
    snapshot::SnapshotInputs,
    sources::{normalize_origin, resolve_revision},
};

mod configuration;
mod images;
mod inputs;
mod objects;
#[cfg(test)]
mod tests;
use objects::{ObjectScopes, bytes_digest};

struct UnitDraft {
    helm: Option<(ChartMetadata, u64)>,
    installation_input: Option<InstallationInput>,
    target: AtomicTarget,
    inputs: BTreeSet<ComponentInput>,
    objects: Vec<Value>,
}

/// Exact render bytes remain available after the lock inventory is sealed.
/// Execution must consume these objects rather than reopening a mutable input.
pub(crate) struct CompiledComponent {
    pub(crate) locked: LockedComponent,
    pub(crate) units: Vec<CompiledUnit>,
}

pub(crate) struct CompiledUnit {
    pub(crate) helm: Option<CompiledHelmRelease>,
    pub(crate) installation_input: Option<InstallationInput>,
    pub(crate) prepared: PreparedAtomicUnit,
    pub(crate) objects: Vec<Value>,
}

/// Compiles the complete component lock from publisher-owned immutable checkouts.
/// The caller retains the source snapshots through this synchronous call. Compilation
/// uses no cluster connection and performs no installation mutations.
pub fn compile_component_lock(
    profile: &LoadedProfile,
    profile_revision: &str,
    sources: &[LockedSource],
    source_roots: &BTreeMap<String, PathBuf>,
) -> Result<Vec<LockedComponent>> {
    let selected = profile
        .definition
        .components
        .iter()
        .map(|component| component.id.clone())
        .collect();
    let catalog = compile_components(profile, profile_revision, sources, source_roots, &selected)?
        .into_iter()
        .map(|component| component.locked)
        .collect::<Vec<_>>();
    validate_component_catalog(&catalog)?;
    Ok(catalog)
}

/// Renders only the expanded selection. The caller retains and validates the
/// complete lock catalog, including every unselected owner's existing inventory.
pub(crate) fn compile_components(
    profile: &LoadedProfile,
    profile_revision: &str,
    sources: &[LockedSource],
    source_roots: &BTreeMap<String, PathBuf>,
    selected: &BTreeSet<ComponentId>,
) -> Result<Vec<CompiledComponent>> {
    let inputs = inputs::publication(profile, profile_revision, sources, source_roots, selected)?;
    compile_with_inputs(profile, profile_revision, sources, selected, inputs)
}

pub(crate) fn compile_locked_components(
    profile: &LoadedProfile,
    lock: &DeploymentLock,
    source_roots: &BTreeMap<ComponentSource, PathBuf>,
    selected: &BTreeSet<ComponentId>,
) -> Result<Vec<CompiledComponent>> {
    let inputs = inputs::locked(profile, lock, source_roots, selected)?;
    compile_with_inputs(
        profile,
        &lock.profile_revision,
        &lock.sources,
        selected,
        inputs,
    )
}

fn compile_with_inputs(
    profile: &LoadedProfile,
    profile_revision: &str,
    sources: &[LockedSource],
    selected: &BTreeSet<ComponentId>,
    resolved: inputs::CompilationInputs,
) -> Result<Vec<CompiledComponent>> {
    ensure!(
        resolve_revision(&profile.repository, "HEAD")? == profile_revision,
        "installation snapshot differs from the publication revision"
    );
    let snapshot = SnapshotInputs::new(&profile.repository, profile_revision)?;
    for path in profile.installation_inputs()? {
        snapshot.file(&path)?;
    }
    let definition: veoveo_deploy_contract::DeploymentProfile =
        serde_json::from_slice(&fs::read(&profile.path)?)?;
    ensure!(
        definition == profile.definition,
        "compiled profile differs from its immutable input document"
    );
    let mut drafted = Vec::new();
    for spec in profile
        .definition
        .components
        .iter()
        .filter(|spec| selected.contains(&spec.id))
    {
        let configuration = &resolved.configurations[&spec.id];
        let component_profile = resolved.installation.get(configuration)?;
        let installation = &configuration.source;
        let platform = component_profile.resolved_platform()?;
        let (owner, drafts) = match &spec.owner {
            ComponentOwner::Source { name } => {
                let source = sources
                    .iter()
                    .find(|source| &source.name == name)
                    .context("component source has no qualified artifacts")?;
                let snapshot = resolved
                    .snapshots
                    .get(&spec.id)
                    .context("component source has no immutable snapshot")?;
                let owner = resolved.owners[&spec.id].clone();
                let mut drafts = Vec::new();
                for release_name in &spec.releases {
                    let release = snapshot
                        .definition
                        .releases
                        .iter()
                        .find(|release| &release.name == release_name)
                        .context("component release is outside its source")?;
                    let target = AtomicTarget::HelmRelease {
                        namespace: profile.definition.namespace.clone(),
                        name: release_name.clone(),
                    };
                    let images = resolved
                        .images
                        .get(&target)
                        .context("release has no image selection")?;
                    let rendered = helm_render_locked(
                        component_profile,
                        snapshot,
                        release,
                        &images.digests,
                        &platform.components,
                        &platform.mcp_servers,
                    )?;
                    let mut objects = Vec::new();
                    append_yaml_bytes(rendered.as_bytes(), "component Helm release", &mut objects)?;
                    let mut inputs =
                        images.rendered(&profile.definition.registry.pull_address, &objects)?;
                    if let Some(exact) =
                        images.narrow(&profile.definition.registry.pull_address, &inputs)?
                    {
                        let rendered = helm_render_locked(
                            component_profile,
                            snapshot,
                            release,
                            &exact.digests,
                            &platform.components,
                            &platform.mcp_servers,
                        )?;
                        objects.clear();
                        append_yaml_bytes(
                            rendered.as_bytes(),
                            "component Helm release",
                            &mut objects,
                        )?;
                        inputs =
                            exact.rendered(&profile.definition.registry.pull_address, &objects)?;
                    }
                    let chart = source
                        .charts
                        .iter()
                        .find(|chart| &chart.release == release_name)
                        .context("component release has no locked chart")?;
                    inputs.insert(ComponentInput::Chart {
                        source: owner.clone(),
                        coordinate: chart.coordinate.clone(),
                        digest: ArtifactDigest::new(&chart.digest)?,
                    });
                    for path in &release.source_values {
                        inputs.insert(file_input(
                            &owner,
                            &snapshot.repository,
                            &snapshot.repository.join(path),
                        )?);
                    }
                    for path in &release.installation_values {
                        inputs.insert(file_input(
                            installation,
                            &component_profile.repository,
                            &component_profile.resolve(path),
                        )?);
                    }
                    drafts.push(UnitDraft {
                        helm: Some((
                            ChartMetadata::load(
                                &snapshot.repository.join(&release.chart).join("Chart.yaml"),
                            )?,
                            release.timeout_seconds,
                        )),
                        installation_input: None,
                        target,
                        inputs,
                        objects,
                    });
                }
                (owner, drafts)
            }
            ComponentOwner::Installation => {
                let owner = &resolved.owners[&spec.id];
                let drafts = spec
                    .installation_inputs
                    .iter()
                    .map(|input| installation_unit(component_profile, owner, *input))
                    .collect::<Result<Vec<_>>>()?;
                (owner.clone(), drafts)
            }
        };
        drafted.push((spec, owner, drafts));
    }
    // Register every CRD before resolving identities. Catalog order must not
    // determine whether a resource supplied by another component has known scope.
    let mut discovery = ObjectScopes::default();
    for (_, _, drafts) in &drafted {
        for draft in drafts {
            discovery.declare_crds(&draft.objects)?;
        }
    }
    let mut catalog = Vec::new();
    for (spec, owner, drafts) in drafted {
        let mut units = Vec::new();
        let mut compiled_units = Vec::new();
        let mut permitted_objects = spec.cluster_objects.clone();
        let mut inputs = BTreeSet::new();
        for mut draft in drafts {
            if draft.helm.is_some() {
                preserve_crds(&mut draft.objects)?;
            }
            let objects = discovery.rendered(
                &draft.objects,
                match &draft.target {
                    AtomicTarget::HelmRelease { namespace, .. } => namespace,
                    AtomicTarget::ManifestSet { .. } => &profile.definition.namespace,
                },
                &spec.cluster_objects,
            )?;
            for object in &objects {
                if let Some(namespace) = &object.identity.namespace {
                    ensure!(
                        spec.namespaces.contains(namespace),
                        "component {} renders an undeclared namespace",
                        spec.id
                    );
                } else {
                    ensure!(
                        spec.cluster_objects.contains(&object.identity),
                        "component {} renders an undeclared cluster object {:?}",
                        spec.id,
                        object.identity
                    );
                }
                permitted_objects.insert(object.identity.clone());
            }
            inputs.extend(draft.inputs.iter().cloned());
            let prepared = PreparedAtomicUnit {
                component: spec.id.clone(),
                source: owner.clone(),
                configuration: resolved.configurations[&spec.id].clone(),
                target: draft.target,
                inputs: draft.inputs,
                objects,
                tool_scope: AtomicToolScope::Exact,
            };
            let mut manifests = draft.objects;
            for (manifest, object) in manifests.iter_mut().zip(&prepared.objects) {
                if let Some(namespace) = &object.identity.namespace {
                    manifest["metadata"]["namespace"] = Value::String(namespace.clone());
                }
            }
            units.push(prepared.clone());
            let helm = draft
                .helm
                .map(|(metadata, timeout)| {
                    CompiledHelmRelease::prepare(&metadata, &manifests, timeout)
                })
                .transpose()?;
            compiled_units.push(CompiledUnit {
                helm,
                installation_input: draft.installation_input,
                prepared,
                objects: manifests,
            });
        }
        let declaration = DeploymentComponent {
            id: spec.id.clone(),
            role: spec.role,
            source: owner,
            configuration: resolved.configurations[&spec.id].clone(),
            namespaces: spec.namespaces.clone(),
            targets: units.iter().map(|unit| unit.target.clone()).collect(),
            permitted_objects,
            inputs,
            dependencies: spec.dependencies.clone(),
            extension_release: spec.extension_release.clone(),
        };
        catalog.push(CompiledComponent {
            locked: lock_component(declaration, units)?,
            units: compiled_units,
        });
    }
    Ok(catalog)
}

fn component_source(source: &LockedSource) -> Result<ComponentSource> {
    Ok(ComponentSource {
        name: source.name.clone(),
        repository: source.repository.clone(),
        revision: SourceRevision::new(&source.revision)?,
    })
}

fn installation_source(profile: &LoadedProfile, revision: &str) -> Result<ComponentSource> {
    let origin = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(&profile.repository)
        .output()
        .context("reading installation origin")?;
    let repository = if origin.status.success() {
        normalize_origin(String::from_utf8(origin.stdout)?.trim())?
    } else {
        format!("file://{}", profile.repository.display())
    };
    Ok(ComponentSource {
        name: INSTALLATION_SOURCE_NAME.into(),
        repository,
        revision: SourceRevision::new(revision)?,
    })
}

fn file_input(source: &ComponentSource, repository: &Path, path: &Path) -> Result<ComponentInput> {
    SnapshotInputs::new(repository, source.revision.as_str())?.file(path)?;
    let root = fs::canonicalize(repository).context("resolving immutable source root")?;
    let path = fs::canonicalize(path).context("resolving component file input")?;
    let relative = path
        .strip_prefix(root)
        .context("component file escapes its immutable source")?;
    Ok(ComponentInput::File {
        source: source.clone(),
        path: relative
            .to_str()
            .context("component path is not UTF-8")?
            .to_owned(),
        digest: bytes_digest(&fs::read(&path)?)?,
    })
}

fn installation_unit(
    profile: &LoadedProfile,
    source: &ComponentSource,
    input: InstallationInput,
) -> Result<UnitDraft> {
    let mut inputs = BTreeSet::new();
    let mut objects = Vec::new();
    let mut append_file = |path: &Path, objects: &mut Vec<Value>| -> Result<()> {
        let path = profile.resolve(path);
        inputs.insert(file_input(source, &profile.repository, &path)?);
        append_yaml_objects(&path, objects)
    };
    match input {
        InstallationInput::Namespace => {
            objects.push(serde_json::json!({"apiVersion":"v1", "kind":"Namespace", "metadata":{"name":profile.definition.namespace}}));
            if let Some(scheduling) = profile.resolved_platform()?.gpu_scheduling {
                let namespace = scheduling.allocator.installation.namespace;
                if namespace != profile.definition.namespace {
                    objects.push(serde_json::json!({"apiVersion":"v1", "kind":"Namespace", "metadata":{"name":namespace}}));
                }
            }
        }
        InstallationInput::NodeBootstrap => {
            let cluster = profile
                .definition
                .kubernetes
                .local_cluster
                .as_ref()
                .context("node bootstrap has no local cluster")?;
            for path in &cluster.node_bootstrap_manifests {
                append_file(path, &mut objects)?;
            }
        }
        InstallationInput::PublicResources => {
            for path in &profile.definition.resources.manifests {
                append_file(path, &mut objects)?;
            }
            for config in &profile.definition.resources.config_maps {
                objects.push(config_map_manifest(profile, config)?);
                for path in config.files.values() {
                    inputs.insert(file_input(
                        source,
                        &profile.repository,
                        &profile.resolve(path),
                    )?);
                }
            }
        }
        InstallationInput::GatewayActivation => {
            let spec = profile
                .definition
                .gateway_activation
                .as_ref()
                .context("gateway activation is absent")?;
            let activation =
                prepare_gateway_activation(profile)?.context("gateway activation is absent")?;
            objects.push(gateway_activation_manifest(
                &profile.definition.namespace,
                &activation,
            ));
            for path in std::iter::once(&spec.control_plane).chain(spec.public_files.values()) {
                inputs.insert(file_input(
                    source,
                    &profile.repository,
                    &profile.resolve(path),
                )?);
            }
        }
        InstallationInput::GpuPlacement => {
            objects.push(
                prepare_gpu_placement(profile)?
                    .context("GPU placement is absent")?
                    .manifest,
            );
        }
        InstallationInput::GpuAllocator => {
            let scheduling = profile
                .resolved_platform()?
                .gpu_scheduling
                .context("GPU allocator is absent")?;
            let allocator = &scheduling.allocator.installation;
            ensure!(
                matches!(
                    allocator.conflicting_device_plugin_removal,
                    veoveo_deploy_contract::ConflictingGpuDevicePluginRemoval::RequireAbsent
                ),
                "component deployment requires a separately owned transition inventory for conflicting device-plugin removal"
            );
            objects.extend(prepare_gpu_allocator_objects(allocator)?);
            inputs.insert(ComponentInput::Chart {
                source: source.clone(),
                coordinate: format!(
                    "{}:{}@{}",
                    allocator.chart.coordinate, allocator.chart.version, allocator.chart.digest
                ),
                digest: ArtifactDigest::new(&allocator.chart.content_digest)?,
            });
            inputs.insert(ComponentInput::Image {
                source: source.clone(),
                target: "nvidia-dra-driver-gpu".into(),
                repository: allocator.image.repository.clone(),
                digest: ArtifactDigest::new(&allocator.image.digest)?,
            });
            return Ok(UnitDraft {
                helm: Some((
                    ChartMetadata {
                        api_version: "v2".into(),
                        name: "dra-driver-nvidia-gpu".into(),
                        version: allocator.chart.version.clone(),
                        app_version: Some(allocator.chart.version.clone()),
                    },
                    allocator.timeout_seconds,
                )),
                installation_input: Some(input),
                target: input.target(&profile.definition)?,
                inputs,
                objects,
            });
        }
    };
    Ok(UnitDraft {
        helm: None,
        installation_input: Some(input),
        target: input.target(&profile.definition)?,
        inputs,
        objects,
    })
}
