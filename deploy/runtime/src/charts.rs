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
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
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
    let image_digests = release_image_digests(
        release.values_contract,
        &source.image_digests,
        &source.deployment_image_digests,
    );
    append_release_values(
        &mut args,
        profile,
        release,
        &source.revision,
        Some(image_digests),
        components,
        mcp_servers,
    )?;
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let rendered = output_checked("helm", refs, None)
        .with_context(|| format!("rendering locked Helm release {}", release.name))?;
    String::from_utf8(rendered).context("Helm output is not UTF-8")
}

pub(crate) fn release_image_digests<'a>(
    values_contract: ReleaseValuesContract,
    source: &'a BTreeMap<String, String>,
    deployment: &'a BTreeMap<String, String>,
) -> &'a BTreeMap<String, String> {
    match values_contract {
        ReleaseValuesContract::Extension => deployment,
        ReleaseValuesContract::Platform | ReleaseValuesContract::VeoveoSource => source,
    }
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
                    "--set-json".to_owned(),
                    format!("components={}", serde_json::to_string(components)?),
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
        ReleaseValuesContract::Extension => {
            args.extend([
                "--set-string".to_owned(),
                format!(
                    "veoveo.registry={}",
                    profile.definition.registry.pull_address
                ),
                "--set-string".to_owned(),
                format!("veoveo.sourceTag={revision}"),
                "--set-string".to_owned(),
                format!("veoveo.installationId={}", profile.definition.name),
            ]);
            if let Some(image_digests) = image_digests {
                ensure!(
                    !image_digests.is_empty(),
                    "locked release {} has no image digests",
                    release.name
                );
                args.extend([
                    "--set".to_owned(),
                    "veoveo.production=true".to_owned(),
                    "--set-json".to_owned(),
                    format!(
                        "veoveo.imageDigests={}",
                        serde_json::to_string(image_digests)?
                    ),
                ]);
            }
            if let Some(placement) = prepare_gpu_placement(profile)? {
                args.extend([
                    "--set-json".to_owned(),
                    format!(
                        "veoveo.gpuPlacement={}",
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
