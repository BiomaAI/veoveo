//! Operational inputs retained from the same snapshot that rendered each owner.
use std::collections::BTreeSet;

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{
    GpuSchedulingProfile, LoadedProfile,
    components::{InstallationInput, ObjectIdentity},
};

use super::CompiledUnit;
use crate::{
    configuration::{
        PreparedGatewayActivation, gateway_activation_manifest, prepare_gateway_activation,
    },
    gpu::{PreparedGpuPlacement, prepare_gpu_placement},
};

#[derive(Debug, Default)]
pub(crate) struct ExecutionInputs {
    pub(crate) gateway_activation: Option<PreparedGatewayActivation>,
    pub(crate) gpu_scheduling: Option<GpuSchedulingProfile>,
    pub(crate) gpu_placement: Option<PreparedGpuPlacement>,
    pub(crate) gpu_workloads: BTreeSet<ObjectIdentity>,
    pub(crate) declared_deployments: BTreeSet<ObjectIdentity>,
    pub(crate) deployments: BTreeSet<ObjectIdentity>,
}

impl ExecutionInputs {
    pub(crate) fn prepare(profile: &LoadedProfile, units: &[CompiledUnit]) -> Result<Self> {
        let identities = units
            .iter()
            .flat_map(|unit| &unit.prepared.objects)
            .map(|object| &object.identity)
            .collect::<BTreeSet<_>>();
        let deployment = |name: &str| ObjectIdentity {
            group: "apps".into(),
            kind: "Deployment".into(),
            namespace: Some(profile.definition.namespace.clone()),
            name: name.into(),
        };
        let declared_deployments = profile
            .definition
            .wait_for_deployments
            .iter()
            .map(|name| deployment(name))
            .collect::<BTreeSet<_>>();
        let deployments = declared_deployments
            .iter()
            .filter(|identity| identities.contains(*identity))
            .cloned()
            .collect();
        let owns = |input| {
            units
                .iter()
                .find(|unit| unit.installation_input == Some(input))
        };
        let gateway_activation = if let Some(unit) = owns(InstallationInput::GatewayActivation) {
            let activation = prepare_gateway_activation(profile)?
                .context("compiled gateway activation has no configuration")?;
            ensure!(
                unit.objects.as_slice()
                    == std::slice::from_ref(&gateway_activation_manifest(
                        &profile.definition.namespace,
                        &activation
                    )),
                "gateway execution inputs differ from the compiled bundle"
            );
            Some(activation)
        } else {
            None
        };
        let gpu_placement = if let Some(unit) = owns(InstallationInput::GpuPlacement) {
            let placement = prepare_gpu_placement(profile)?
                .context("compiled GPU claim has no configuration")?;
            ensure!(
                unit.objects.as_slice() == std::slice::from_ref(&placement.manifest),
                "GPU execution inputs differ from the compiled claim"
            );
            Some(placement)
        } else {
            None
        };
        let gpu_scheduling = profile
            .resolved_platform()?
            .gpu_scheduling
            .filter(|scheduling| {
                owns(InstallationInput::GpuAllocator).is_some()
                    || gpu_placement.is_some()
                    || scheduling
                        .same_physical_device_groups
                        .iter()
                        .flat_map(|group| &group.workloads)
                        .any(|workload| identities.contains(&deployment(&workload.deployment)))
            });
        let gpu_workloads = gpu_scheduling
            .iter()
            .flat_map(|scheduling| &scheduling.same_physical_device_groups)
            .flat_map(|group| &group.workloads)
            .map(|workload| deployment(&workload.deployment))
            .filter(|identity| identities.contains(identity))
            .collect();
        Ok(Self {
            gateway_activation,
            gpu_scheduling,
            gpu_placement,
            gpu_workloads,
            declared_deployments,
            deployments,
        })
    }
}
