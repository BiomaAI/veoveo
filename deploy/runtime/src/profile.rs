use crate::{
    charts::validate_helm_releases,
    cluster::{wait_for_cluster_gpu, wait_for_cluster_nodes},
    compile::compile_locked_components,
    configuration::{
        after_secret_closure, prepare_gateway_activation, prepare_secret_closure,
        validate_node_bootstrap_secret_boundary,
    },
    gpu::{apply_gpu_placement, ensure_gpu_allocator, prepare_gpu_placement, verify_gpu_placement},
    images::{validate_bake_selections, validate_locked_images},
    installed::InstalledState,
    process::status_checked,
    sources::{
        load_deployment_lock, load_profile, resolve_locked_sources, resolve_sources,
        validate_locked_profile,
    },
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
    time::Duration,
};
use veoveo_deploy_contract::{DeploymentSourceRole, components::InstallationInput};

pub fn profile_validate(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    validate_node_bootstrap_secret_boundary(&profile)?;
    let _gateway_activation = prepare_gateway_activation(&profile)?;
    let _gpu_placement = prepare_gpu_placement(&profile)?;
    let sources = resolve_sources(&profile)?;
    let selected_images = validate_bake_selections(&profile, &sources)?;
    profile.validate_image_plan(&selected_images)?;
    validate_helm_releases(&profile, &sources)?;
    let platform = profile.resolved_platform()?;
    println!(
        "Deployment profile {} is valid: {} sources, {} image publication phases, {} Helm releases, {} platform components, and {} MCP servers",
        profile.definition.name,
        sources.len(),
        sources
            .iter()
            .map(|source| match source.definition.role {
                DeploymentSourceRole::Platform => 1,
                DeploymentSourceRole::Extension | DeploymentSourceRole::Workload => {
                    source.definition.image_groups.len()
                }
            })
            .sum::<usize>(),
        sources
            .iter()
            .map(|source| source.definition.releases.len())
            .sum::<usize>(),
        platform.components.len(),
        platform.mcp_servers.len(),
    );
    Ok(())
}

pub fn profile_up(path: &Path, lock_path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let gateway_activation = prepare_gateway_activation(&profile)?;
    let lock = load_deployment_lock(lock_path)?;
    validate_locked_profile(&profile, &lock)?;
    validate_locked_images(&profile, &lock)?;
    let selected = lock
        .components
        .iter()
        .map(|component| component.declaration.id.clone())
        .collect::<BTreeSet<_>>();
    let sources = resolve_locked_sources(&profile, &lock, &selected)?;
    let source_roots = sources
        .iter()
        .map(|(identity, source)| (identity.clone(), source.repository.clone()))
        .collect::<BTreeMap<_, _>>();
    let compiled = compile_locked_components(&profile, &lock, &source_roots, &selected)?;
    ensure!(
        compiled.len() == lock.components.len(),
        "prepared component catalog differs from the deployment lock"
    );
    for component in &compiled {
        let locked = lock
            .components
            .iter()
            .find(|locked| locked.declaration.id == component.locked.declaration.id)
            .context("prepared component has no locked owner")?;
        ensure!(
            &component.locked == locked,
            "prepared component {} differs from its locked ownership, inputs, or objects",
            locked.declaration.id
        );
        ensure!(
            component
                .units
                .iter()
                .map(|unit| &unit.prepared.target)
                .collect::<BTreeSet<_>>()
                == locked.declaration.targets.iter().collect::<BTreeSet<_>>(),
            "prepared executable targets differ from their locked inventory"
        );
    }
    let objects = compiled
        .iter()
        .flat_map(|component| {
            component
                .units
                .iter()
                .flat_map(|unit| unit.objects.iter().cloned())
        })
        .collect::<Vec<_>>();
    let platform = profile.resolved_platform()?;
    let context = profile.definition.kubernetes.context.as_str();
    let installed = InstalledState::open(&profile.repository, context, &compiled)?;
    let gpu_migration = platform
        .gpu_scheduling
        .as_ref()
        .map(|scheduling| {
            crate::gpu::migration::prepare(
                context,
                &profile.definition.namespace,
                scheduling,
                &lock.components,
                &selected,
            )
        })
        .transpose()?;
    let invalidated = gpu_migration
        .as_ref()
        .map(|migration| migration.quiesced_workloads())
        .unwrap_or_default();
    let mutation_plan = installed.plan(&lock.components, &selected, &compiled, &invalidated)?;
    let gpu_placement = prepare_gpu_placement(&profile)?;
    if let Some(placement) = &gpu_placement {
        let (_, claim) = installation_units(&compiled, InstallationInput::GpuPlacement)
            .next()
            .context("GPU placement has no prepared claim operation")?;
        ensure!(
            claim.objects.as_slice() == std::slice::from_ref(&placement.manifest),
            "GPU placement configuration differs from the compiled claim"
        );
    }
    let secret_closure = prepare_secret_closure(
        path,
        lock_path,
        &profile,
        &objects,
        gateway_activation.as_ref(),
    )?;

    after_secret_closure(secret_closure, |_closure| {
        for (component, unit) in installation_units(&compiled, InstallationInput::NodeBootstrap) {
            installed.apply_planned(component, unit, &mutation_plan)?;
        }
        if platform.gpu_scheduling.is_some() {
            wait_for_cluster_nodes(context, Duration::from_secs(120))?;
        } else {
            wait_for_cluster_gpu(context, Duration::from_secs(120))?;
        }

        for (component, unit) in installation_units(&compiled, InstallationInput::Namespace) {
            installed.apply_planned(component, unit, &mutation_plan)?;
        }

        if let Some(placement) = &gpu_placement {
            let scheduling = platform
                .gpu_scheduling
                .as_ref()
                .context("prepared GPU placement has no resolved scheduling profile")?;
            let (component, allocator) =
                installation_units(&compiled, InstallationInput::GpuAllocator)
                    .next()
                    .context("GPU allocator has no prepared Helm operation")?;
            ensure_gpu_allocator(
                context,
                scheduling,
                gpu_migration
                    .as_ref()
                    .context("GPU migration has no checked inventory")?,
                || {
                    installed
                        .apply_planned(component, allocator, &mutation_plan)
                        .map(|_| ())
                },
            )?;
            apply_gpu_placement(
                context,
                &profile.definition.namespace,
                scheduling,
                placement,
                || {
                    let (component, claim) =
                        installation_units(&compiled, InstallationInput::GpuPlacement)
                            .next()
                            .context("GPU placement has no prepared claim operation")?;
                    installed
                        .apply_planned(component, claim, &mutation_plan)
                        .map(|_| ())
                },
            )?;
        }

        for input in [
            InstallationInput::PublicResources,
            InstallationInput::GatewayActivation,
        ] {
            for (component, unit) in installation_units(&compiled, input) {
                installed.apply_planned(component, unit, &mutation_plan)?;
            }
        }

        for source in &profile.definition.sources {
            for release in &source.releases {
                let target = veoveo_deploy_contract::components::AtomicTarget::HelmRelease {
                    namespace: profile.definition.namespace.clone(),
                    name: release.name.clone(),
                };
                let (component, prepared) = compiled
                    .iter()
                    .flat_map(|component| component.units.iter().map(move |unit| (component, unit)))
                    .find(|(_, unit)| unit.prepared.target == target)
                    .context("source release has no prepared Helm operation")?;
                installed.apply_planned(component, prepared, &mutation_plan)?;
            }
        }
        for deployment in &profile.definition.wait_for_deployments {
            let target = format!("deployment/{deployment}");
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    profile.definition.namespace.as_str(),
                    "rollout",
                    "status",
                    target.as_str(),
                    "--timeout=10m",
                ],
                &[],
                None,
            )?;
        }
        if let Some(scheduling) = &platform.gpu_scheduling {
            verify_gpu_placement(context, &profile.definition.namespace, scheduling)?;
        }
        println!(
            "Deployment profile {} now runs {} digest-locked sources",
            profile.definition.name,
            sources.len()
        );
        Ok(())
    })
}

fn installation_units(
    components: &[crate::compile::CompiledComponent],
    input: InstallationInput,
) -> impl Iterator<
    Item = (
        &crate::compile::CompiledComponent,
        &crate::compile::CompiledUnit,
    ),
> {
    components.iter().flat_map(move |component| {
        component
            .units
            .iter()
            .filter(move |unit| unit.installation_input == Some(input))
            .map(move |unit| (component, unit))
    })
}

pub fn profile_gpu_verify(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let platform = profile.resolved_platform()?;
    let scheduling = platform
        .gpu_scheduling
        .as_ref()
        .context("deployment profile does not declare managed GPU scheduling")?;
    verify_gpu_placement(
        profile.definition.kubernetes.context.as_str(),
        profile.definition.namespace.as_str(),
        scheduling,
    )?;
    println!(
        "Deployment profile {} GPU placement is healthy",
        profile.definition.name
    );
    Ok(())
}

pub fn profile_down(path: &Path) -> Result<()> {
    let profile = load_profile(path)?;
    let context = profile.definition.kubernetes.context.as_str();
    let releases = profile
        .definition
        .sources
        .iter()
        .flat_map(|source| source.releases.iter())
        .collect::<Vec<_>>();
    for release in releases.into_iter().rev() {
        let output = Command::new("helm")
            .args([
                "--kube-context",
                context,
                "status",
                release.name.as_str(),
                "--namespace",
                profile.definition.namespace.as_str(),
            ])
            .output()
            .context("checking Helm release state")?;
        if output.status.success() {
            status_checked(
                "helm",
                [
                    "--kube-context",
                    context,
                    "uninstall",
                    release.name.as_str(),
                    "--namespace",
                    profile.definition.namespace.as_str(),
                ],
                &[],
                None,
            )?;
        }
    }
    Ok(())
}
