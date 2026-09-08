use crate::{
    charts::validate_helm_releases,
    cluster::wait_for_cluster_nodes,
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
use veoveo_deploy_contract::{
    DeploymentSourceRole,
    components::{InstallationInput, InstallationReceipt, select_components},
};

mod coordination;
mod execution;
mod operations;
use execution::ExecutionScope;

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

pub fn profile_up(
    path: &Path,
    lock_path: &Path,
    selection: &veoveo_deploy_contract::components::ComponentSelection,
) -> Result<InstallationReceipt> {
    let profile = load_profile(path)?;
    let lock = load_deployment_lock(lock_path)?;
    validate_locked_profile(&profile, &lock)?;
    coordination::validate_reserved_identity(&lock.components)?;
    validate_locked_images(&profile, &lock)?;
    let requested = match selection {
        veoveo_deploy_contract::components::ComponentSelection::All => lock
            .components
            .iter()
            .map(|component| component.declaration.id.clone())
            .collect(),
        veoveo_deploy_contract::components::ComponentSelection::Exact(ids) => ids.clone(),
    };
    let selected = select_components(&lock.components, &requested)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let sources = resolve_locked_sources(&profile, &lock, &selected)?;
    let source_roots = sources
        .iter()
        .map(|(identity, source)| (identity.clone(), source.repository.clone()))
        .collect::<BTreeMap<_, _>>();
    let compiled = compile_locked_components(&profile, &lock, &source_roots, &selected)?;
    ensure!(
        compiled.len() == selected.len(),
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
    let execution = ExecutionScope::prepare(&compiled, &lock.components)?;
    let context = profile.definition.kubernetes.context.as_str();
    let installed = InstalledState::open(&profile.repository, context, &compiled)?;
    let gpu_migration = installation_units(&compiled, InstallationInput::GpuAllocator)
        .next()
        .map(|_| {
            let scheduling = execution
                .gpu_scheduling
                .context("allocator has no compiled GPU settings")?;
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
    // Complete deterministic preflight before the coordination API write.
    installed.plan(&lock.components, &requested, &compiled, &invalidated)?;
    let secret_closure = prepare_secret_closure(
        path,
        lock_path,
        &profile,
        &objects,
        execution.gateway_activation,
    )?;

    after_secret_closure(secret_closure, |_closure| {
        let mut coordination = coordination::ExecutionLock::acquire(context)?;
        installed.validate_destination()?;
        // Another installer may have completed during source preparation. Repeat
        // ownership and installed-state observations under the lock.
        let mutation_plan =
            installed.plan(&lock.components, &requested, &compiled, &invalidated)?;
        let unselected_before = installed.unselected(&lock.components, &selected)?;
        coordination.begin_execution()?;
        let mut operations = operations::Operations::new(&installed, &mutation_plan, &coordination);
        for (component, unit) in installation_units(&compiled, InstallationInput::NodeBootstrap) {
            operations.apply(component, unit)?;
        }
        // GPU admission belongs to the selected allocator, claim, and consumers.
        // A CPU component must also work on DRA nodes without extended resources.
        wait_for_cluster_nodes(context, Duration::from_secs(120))?;

        for (component, unit) in installation_units(&compiled, InstallationInput::Namespace) {
            operations.apply(component, unit)?;
        }

        if let Some((component, allocator)) =
            installation_units(&compiled, InstallationInput::GpuAllocator).next()
        {
            let scheduling = execution
                .gpu_scheduling
                .as_ref()
                .context("prepared GPU placement has no resolved scheduling profile")?;
            coordination.check()?;
            ensure_gpu_allocator(
                context,
                scheduling,
                gpu_migration
                    .as_ref()
                    .context("GPU migration has no checked inventory")?,
                || operations.apply(component, allocator),
            )?;
        }
        if let Some(placement) = execution.gpu_placement {
            let scheduling = execution
                .gpu_scheduling
                .context("GPU claim has no compiled settings")?;
            let (component, claim) = installation_units(&compiled, InstallationInput::GpuPlacement)
                .next()
                .context("GPU placement has no prepared claim operation")?;
            let mut created = false;
            coordination.check()?;
            apply_gpu_placement(
                context,
                &profile.definition.namespace,
                scheduling,
                placement,
                || {
                    created = true;
                    operations.apply(component, claim)
                },
            )?;
            if !created {
                operations.reuse_claim(component, claim)?;
            }
        }

        for input in [
            InstallationInput::PublicResources,
            InstallationInput::GatewayActivation,
        ] {
            for (component, unit) in installation_units(&compiled, input) {
                operations.apply(component, unit)?;
            }
        }

        for id in &mutation_plan.expanded {
            let component = compiled
                .iter()
                .find(|component| &component.locked.declaration.id == id)
                .context("planned owner has no compiled operations")?;
            for source in &profile.definition.sources {
                for release in &source.releases {
                    let target = veoveo_deploy_contract::components::AtomicTarget::HelmRelease {
                        namespace: profile.definition.namespace.clone(),
                        name: release.name.clone(),
                    };
                    if let Some(prepared) = component.units.iter().find(|unit| {
                        unit.prepared.target == target && unit.installation_input.is_none()
                    }) {
                        operations.apply(component, prepared)?;
                    }
                }
            }
        }
        for deployment in &execution.deployments {
            let target = format!("deployment/{}", deployment.name);
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    deployment
                        .namespace
                        .as_deref()
                        .context("wait Deployment has no namespace")?,
                    "rollout",
                    "status",
                    target.as_str(),
                    "--timeout=10m",
                ],
                &[],
                None,
            )?;
        }
        if let Some(scheduling) = execution.gpu_scheduling {
            crate::gpu::verify_gpu_workloads(
                context,
                &profile.definition.namespace,
                scheduling,
                &execution.gpu_workloads,
            )?;
        }
        println!(
            "Deployment profile {} now runs {} digest-locked sources",
            profile.definition.name,
            sources.len()
        );
        let unselected_after = installed.unselected(&lock.components, &selected)?;
        let operations = operations.finish()?;
        let coordination = coordination.release()?;
        Ok(InstallationReceipt {
            schema_version: "veoveo.io/component-installation/v2".into(),
            coordination,
            unselected_before,
            unselected_after,
            operations,
            plan: mutation_plan,
        })
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
    let mut coordination = coordination::ExecutionLock::acquire(context)?;
    let releases = profile
        .definition
        .sources
        .iter()
        .flat_map(|source| source.releases.iter())
        .collect::<Vec<_>>();
    coordination.begin_execution()?;
    for release in releases.into_iter().rev() {
        coordination.check()?;
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
    coordination.release()?;
    Ok(())
}
