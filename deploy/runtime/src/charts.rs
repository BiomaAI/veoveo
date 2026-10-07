use crate::{
    configuration::prepare_gateway_activation,
    gpu::prepare_gpu_placement,
    process::{output_checked, path_str},
    snapshot::SnapshotInputs,
    sources::{ResolvedSource, resolve_revision},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
};
use veoveo_deploy_contract::{
    DeploymentSource, FirstPartyMcpServer, LoadedProfile, LockedChart, LockedSource,
    PlatformComponent, ReleaseSpec, ReleaseValuesContract, source_chart_content_digest,
};
const VALIDATION_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

pub(crate) fn validate_locked_charts(
    source: &DeploymentSource,
    locked: &LockedSource,
    repository: &Path,
) -> Result<()> {
    // The complete catalog validates ownership separately. Only the selected
    // releases supplied here may inspect this checkout's chart and values files.
    for expected in lock_source_charts(source, repository)? {
        let chart = locked
            .charts
            .iter()
            .find(|candidate| candidate.release == expected.release)
            .with_context(|| {
                format!(
                    "deployment lock source {} omits Helm release {}",
                    source.name, expected.release
                )
            })?;
        ensure!(
            chart.coordinate == expected.coordinate,
            "deployment lock chart coordinate for release {} is {}, expected {}",
            expected.release,
            chart.coordinate,
            expected.coordinate
        );
        ensure!(
            chart.digest == expected.digest,
            "deployment lock chart digest for release {} is {}, source produced {}",
            expected.release,
            chart.digest,
            expected.digest
        );
    }
    Ok(())
}

pub(crate) fn validate_helm_releases(
    profile: &LoadedProfile,
    sources: &[ResolvedSource],
) -> Result<()> {
    let platform = profile.resolved_platform()?;
    for source in sources {
        for release in &source.definition.releases {
            let rendered = helm_render(
                profile,
                source,
                release,
                VALIDATION_REVISION,
                &platform.components,
                &platform.mcp_servers,
            )?;
            let images = rendered_container_images(&rendered)?;
            ensure!(
                !images.is_empty(),
                "Helm release {} rendered no container images",
                release.name
            );
            let registry_prefix = format!("{}/", profile.definition.registry.pull_address);
            let owned = images
                .iter()
                .filter(|image| image.starts_with(&registry_prefix))
                .collect::<Vec<_>>();
            ensure!(
                !owned.is_empty(),
                "Helm release {} rendered no images from selected registry {}",
                release.name,
                profile.definition.registry.pull_address
            );
            for image in owned {
                ensure!(
                    image.contains("@sha256:") || image.ends_with(VALIDATION_REVISION),
                    "Helm release {} rendered mutable container image {image}",
                    release.name
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn helm_render_locked(
    profile: &LoadedProfile,
    source: &ResolvedSource,
    release: &ReleaseSpec,
    image_digests: &BTreeMap<String, String>,
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
    module_plan: Option<&crate::compile::module_plan::GeneratedModulePlan>,
) -> Result<String> {
    let chart = source.repository.join(&release.chart);
    let mut args = vec![
        "template".to_owned(),
        release.name.clone(),
        path_str(&chart)?.to_owned(),
        "--namespace".to_owned(),
        profile.definition.namespace.clone(),
        "--include-crds".to_owned(),
    ];
    for values in ordered_release_values(&source.repository, &profile.directory, release) {
        args.push("--values".to_owned());
        args.push(path_str(&values)?.to_owned());
    }
    append_release_values(
        &mut args,
        profile,
        release,
        &source.revision,
        Some(image_digests),
        components,
        mcp_servers,
    )?;
    let mut module_plan_file = None;
    if let Some(module_plan) = module_plan {
        let mut file = tempfile::NamedTempFile::new()?;
        file.write_all(&serde_json::to_vec(&module_plan.plan)?)?;
        args.extend([
            "--set-file".to_owned(),
            format!("moduleInstallation.planJson={}", path_str(file.path())?),
        ]);
        module_plan_file = Some(file);
    }
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let rendered = output_checked("helm", refs, None)
        .with_context(|| format!("rendering locked Helm release {}", release.name))?;
    drop(module_plan_file);
    String::from_utf8(rendered).context("Helm output is not UTF-8")
}

pub(crate) fn helm_render(
    profile: &LoadedProfile,
    source: &ResolvedSource,
    release: &ReleaseSpec,
    revision: &str,
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
) -> Result<String> {
    let chart = source.repository.join(&release.chart);
    let mut args = vec![
        "template".to_owned(),
        release.name.clone(),
        path_str(&chart)?.to_owned(),
    ];
    for values in ordered_release_values(&source.repository, &profile.directory, release) {
        args.push("--values".to_owned());
        args.push(path_str(&values)?.to_owned());
    }
    append_release_values(
        &mut args,
        profile,
        release,
        revision,
        None,
        components,
        mcp_servers,
    )?;
    if release.values_contract == ReleaseValuesContract::Platform
        && profile
            .resolved_platform()?
            .components
            .contains(&PlatformComponent::Gateway)
    {
        let input = profile
            .definition
            .module_installation
            .as_ref()
            .context("source validation requires explicit moduleInstallation inputs")?;
        let fixture = input.development_plan.as_ref()
            .context("source validation requires an actual generated developmentPlan; it does not qualify an installed composition image")?;
        let path = profile.resolve(fixture);
        let plan: veoveo_modules::ModulePlanDocument =
            serde_json::from_slice(&std::fs::read(&path)?)?;
        let selection: veoveo_modules::ModuleSelectionDocument =
            serde_json::from_slice(&std::fs::read(profile.resolve(&input.selection))?)?;
        veoveo_deploy_contract::validate_module_plan(
            &plan,
            &selection,
            plan.composition().as_str(),
            &profile.resolved_platform()?,
        )?;
        args.extend([
            "--set-file".into(),
            format!("moduleInstallation.planJson={}", path_str(&path)?),
        ]);
    }
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let rendered = output_checked("helm", refs, None)
        .with_context(|| format!("rendering Helm release {}", release.name))?;
    String::from_utf8(rendered).context("Helm output is not UTF-8")
}

pub(crate) fn ordered_release_values(
    source_repository: &Path,
    installation_directory: &Path,
    release: &ReleaseSpec,
) -> Vec<PathBuf> {
    release
        .source_values
        .iter()
        .map(|path| source_repository.join(path))
        .chain(
            release
                .installation_values
                .iter()
                .map(|path| installation_directory.join(path)),
        )
        .collect()
}

pub(crate) fn append_release_values(
    args: &mut Vec<String>,
    profile: &LoadedProfile,
    release: &ReleaseSpec,
    revision: &str,
    image_digests: Option<&BTreeMap<String, String>>,
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
) -> Result<()> {
    match release.values_contract {
        ReleaseValuesContract::Platform | ReleaseValuesContract::VeoveoSource => {
            args.extend([
                "--set-string".to_owned(),
                format!(
                    "global.veoveoRegistry={}",
                    profile.definition.registry.pull_address
                ),
                "--set-string".to_owned(),
                format!("global.veoveoTag={revision}"),
            ]);
            if let Some(image_digests) = image_digests {
                ensure!(
                    !image_digests.is_empty(),
                    "locked release {} has no image digests",
                    release.name
                );
                args.extend([
                    "--set".to_owned(),
                    "global.production=true".to_owned(),
                    "--set-json".to_owned(),
                    format!(
                        "global.imageDigests={}",
                        serde_json::to_string(image_digests)?
                    ),
                ]);
            }
            if release.values_contract == ReleaseValuesContract::Platform {
                let platform = profile.resolved_platform()?;
                args.extend([
                    "--set-string".to_owned(),
                    format!("global.installationId={}", profile.definition.name),
                    "--set-string".to_owned(),
                    "installationPreset=custom".to_owned(),
                    "--set-string".to_owned(),
                    format!(
                        "computerCapacity={}",
                        platform.computer_capacity.helm_value()
                    ),
                    "--set-json".to_owned(),
                    format!(
                        "components={}",
                        serde_json::to_string(
                            &components
                                .iter()
                                .map(|component| component.helm_value())
                                .collect::<Vec<_>>()
                        )?
                    ),
                    "--set-json".to_owned(),
                    format!("mcpServers={}", serde_json::to_string(mcp_servers)?),
                ]);
                if !platform.artifact_audiences.is_empty() {
                    args.extend([
                        "--set-json".to_owned(),
                        format!(
                            "artifactService.allowedAudiences={}",
                            serde_json::to_string(&platform.artifact_audiences)?
                        ),
                    ]);
                }
                if let Some(placement) = prepare_gpu_placement(profile)? {
                    args.extend([
                        "--set-json".to_owned(),
                        format!(
                            "global.gpuPlacement={}",
                            serde_json::to_string(&serde_json::json!({
                                "enabled": true,
                                "claimName": placement.claim_name,
                                "runtimeClassName": placement.runtime_class_name,
                                "evidenceDigest": placement.evidence_digest,
                                "workloadRequests": placement.workload_requests,
                                "workloadReplicas": placement.workload_replicas
                            }))?
                        ),
                    ]);
                }
                if let Some(activation) = prepare_gateway_activation(profile)? {
                    args.extend([
                        "--set-string".to_owned(),
                        format!(
                            "gateway.existingControlPlaneConfigMap={}",
                            activation.config_map_name
                        ),
                        "--set-string".to_owned(),
                        format!("gateway.controlPlaneRevision={}", activation.revision),
                        "--set-string".to_owned(),
                        format!("global.existingSecret={}", activation.confidential_secret),
                    ]);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn rendered_container_images(rendered: &str) -> Result<Vec<String>> {
    let mut images = Vec::new();
    for line in rendered.lines() {
        let trimmed = line.trim_start();
        let value = trimmed
            .strip_prefix("image:")
            .or_else(|| trimmed.strip_prefix("- image:"));
        let Some(value) = value else {
            continue;
        };
        let value = value.trim().trim_matches(['\'', '"']);
        ensure!(
            !value.is_empty(),
            "rendered Kubernetes image field is empty"
        );
        ensure!(
            !value.chars().any(char::is_whitespace),
            "rendered Kubernetes image field contains whitespace: {value}"
        );
        images.push(value.to_owned());
    }
    Ok(images)
}

/// Locks source charts after checking their actual input bytes against Git.
/// The caller retains the immutable checkout through subsequent rendering.
pub fn lock_source_charts(
    source: &DeploymentSource,
    repository: &Path,
) -> Result<Vec<LockedChart>> {
    let revision = resolve_revision(repository, "HEAD")?;
    let snapshot = SnapshotInputs::new(repository, &revision)?;
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
            snapshot.tree(&release.chart)?;
            for values in &release.source_values {
                snapshot.file(values)?;
            }
            Ok(LockedChart {
                release: release.name.clone(),
                coordinate: format!(
                    "source://{}/{}",
                    source.name,
                    release.chart.to_string_lossy()
                ),
                digest: source_chart_content_digest(repository, &release.chart)?.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_deploy_contract::InstallationPreset;
    mod gpu_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testing/fixtures/platform-selection/gpu_scheduling.rs"
        ));
    }

    #[test]
    fn release_values_use_chart_identities_for_every_typed_component() {
        let repository = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let mut profile = LoadedProfile::load(
            &repository.join("testing/fixtures/platform-selection/deployment.json"),
            repository,
        )
        .unwrap();
        profile.definition.gateway_activation = None;
        profile.definition.gateway_requirements.clear();
        let release = profile.definition.sources[0].releases[0].clone();
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../helm/veoveo/values.schema.json")).unwrap();
        let admitted_components = schema["properties"]["components"]["items"]["enum"]
            .as_array()
            .unwrap();
        let admitted_servers = schema["properties"]["mcpServers"]["items"]["enum"]
            .as_array()
            .unwrap();
        for preset in [InstallationPreset::Full, InstallationPreset::Foundation] {
            profile.definition.platform = gpu_fixture::selection(preset);
            let selected = profile.definition.platform.resolve().unwrap();
            let mut args = Vec::new();
            append_release_values(
                &mut args,
                &profile,
                &release,
                VALIDATION_REVISION,
                None,
                &selected.components,
                &selected.mcp_servers,
            )
            .unwrap();
            let components: serde_json::Value = serde_json::from_str(
                args.iter()
                    .find_map(|arg| arg.strip_prefix("components="))
                    .unwrap(),
            )
            .unwrap();
            assert!(
                components
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|value| admitted_components.contains(value))
            );
            let expected: BTreeSet<_> = selected
                .components
                .iter()
                .map(|component| component.helm_value())
                .collect();
            let actual: BTreeSet<_> = components
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect();
            assert_eq!(actual, expected);
            if preset == InstallationPreset::Full {
                let chart: BTreeSet<_> = admitted_components
                    .iter()
                    .map(|value| value.as_str().unwrap())
                    .collect();
                assert_eq!(
                    actual, chart,
                    "all component variants must agree with chart identities"
                );
            }
            let servers: serde_json::Value = serde_json::from_str(
                args.iter()
                    .find_map(|arg| arg.strip_prefix("mcpServers="))
                    .unwrap(),
            )
            .unwrap();
            assert!(
                servers
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|value| admitted_servers.contains(value))
            );
            assert!(args.contains(&"computerCapacity=unconfigured".into()));
        }
    }
}
