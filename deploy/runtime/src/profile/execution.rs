//! Resolve installation helpers exclusively from the compiled component inputs.
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use veoveo_deploy_contract::{
    GpuSchedulingProfile,
    components::{LockedComponent, ObjectIdentity},
};

use crate::{
    compile::CompiledComponent, configuration::PreparedGatewayActivation, gpu::PreparedGpuPlacement,
};

#[derive(Debug)]
pub(super) struct ExecutionScope<'a> {
    pub gateway_activation: Option<&'a PreparedGatewayActivation>,
    pub gpu_scheduling: Option<&'a GpuSchedulingProfile>,
    pub gpu_placement: Option<&'a PreparedGpuPlacement>,
    pub deployments: BTreeSet<&'a ObjectIdentity>,
}

impl<'a> ExecutionScope<'a> {
    pub fn prepare(
        components: &'a [CompiledComponent],
        catalog: &[LockedComponent],
    ) -> Result<Self> {
        let owned = catalog
            .iter()
            .flat_map(|component| &component.units)
            .flat_map(|unit| &unit.objects)
            .map(|object| &object.identity)
            .collect::<BTreeSet<_>>();
        let mut scope = Self {
            gateway_activation: None,
            gpu_scheduling: None,
            gpu_placement: None,
            deployments: BTreeSet::new(),
        };
        for component in components {
            let inputs = &component.execution;
            for deployment in &inputs.declared_deployments {
                ensure!(
                    owned.contains(deployment),
                    "configured rollout wait has no locked Deployment owner: {deployment:?}"
                );
            }
            if let Some(activation) = &inputs.gateway_activation {
                ensure!(
                    scope.gateway_activation.replace(activation).is_none(),
                    "multiple compiled gateway activation owners"
                );
            }
            if let Some(placement) = &inputs.gpu_placement {
                ensure!(
                    scope.gpu_placement.replace(placement).is_none(),
                    "multiple compiled GPU claim owners"
                );
            }
            if let Some(scheduling) = &inputs.gpu_scheduling {
                if let Some(previous) = scope.gpu_scheduling {
                    ensure!(
                        previous == scheduling,
                        "selected components have incompatible retained GPU configuration; update their configuration together"
                    );
                } else {
                    scope.gpu_scheduling = Some(scheduling);
                }
            }
            scope.deployments.extend(&inputs.deployments);
        }
        Ok(scope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::live_tests::component;
    use veoveo_deploy_contract::*;

    #[test]
    fn unknown_rollout_waits_fail_before_execution() {
        let mut component = component("test", "platform", &["platform"]);
        component
            .execution
            .declared_deployments
            .insert(ObjectIdentity {
                group: "apps".into(),
                kind: "Deployment".into(),
                namespace: Some("test".into()),
                name: "misspelled-workload".into(),
            });
        assert!(
            ExecutionScope::prepare(
                std::slice::from_ref(&component),
                std::slice::from_ref(&component.locked)
            )
            .unwrap_err()
            .to_string()
            .contains("no locked Deployment owner")
        );
    }

    #[test]
    fn retained_gpu_consumers_cannot_silently_change_allocator_policy() {
        let scheduling: GpuSchedulingProfile = serde_json::from_value(serde_json::json!({
            "runtimeClassName":"nvidia", "allocatableDevices":1,
            "evidenceDigest":format!("sha256:{}", "a".repeat(64)),
            "allocator":{
                "claimName":"fixture", "fullDeviceClassName":"gpu.nvidia.com", "migDeviceClassName":"mig.nvidia.com",
                "driverName":"gpu.nvidia.com", "configurationApiVersion":"resource.nvidia.com/v1beta1",
                "installation":{
                    "releaseName":"allocator", "namespace":"test",
                    "chart":{"coordinate":NVIDIA_DRA_CHART_COORDINATE, "version":NVIDIA_DRA_VERSION,
                        "digest":NVIDIA_DRA_CHART_DIGEST,"contentDigest":NVIDIA_DRA_CHART_CONTENT_DIGEST},
                    "image":{"repository":NVIDIA_DRA_IMAGE_REPOSITORY,"tag":format!("v{NVIDIA_DRA_VERSION}"),
                        "digest":NVIDIA_DRA_IMAGE_DIGEST,"platformDigests":{"linux/amd64":NVIDIA_DRA_IMAGE_AMD64_DIGEST,"linux/arm64":NVIDIA_DRA_IMAGE_ARM64_DIGEST}},
                    "nvidiaDriverRoot":"/", "eligibleNodeSelector":{"kubernetes.io/hostname":"test-gpu"},
                    "conflictingDevicePluginRemoval":{"mode":"require-absent"},
                    "maturityAcceptance":"technology-preview", "timeoutSeconds":300
                }
            }, "samePhysicalDeviceGroups":[], "differentPhysicalDeviceGroups":[]
        })).unwrap();
        let mut allocator = component("test", "allocator", &["allocator"]);
        allocator.execution.gpu_scheduling = Some(scheduling.clone());
        let mut consumer = component("test", "consumer", &["consumer"]);
        consumer.execution.gpu_scheduling = Some(scheduling);
        let mut components = vec![allocator, consumer];
        let catalog = components
            .iter()
            .map(|component| component.locked.clone())
            .collect::<Vec<_>>();
        assert!(ExecutionScope::prepare(&components, &catalog).is_ok());
        components[1]
            .execution
            .gpu_scheduling
            .as_mut()
            .unwrap()
            .allocator
            .installation
            .conflicting_device_plugin_removal =
            ConflictingGpuDevicePluginRemoval::DeleteDaemonSet {
                namespace: "other".into(),
                name: "other-driver".into(),
            };
        assert!(
            ExecutionScope::prepare(&components, &catalog)
                .unwrap_err()
                .to_string()
                .contains("incompatible retained GPU configuration")
        );
        // The unselected consumer contributes no policy to an allocator-only scope.
        assert_eq!(
            ExecutionScope::prepare(&components[..1], &catalog)
                .unwrap()
                .gpu_scheduling
                .unwrap()
                .allocator
                .installation
                .conflicting_device_plugin_removal,
            ConflictingGpuDevicePluginRemoval::RequireAbsent
        );
    }
}
