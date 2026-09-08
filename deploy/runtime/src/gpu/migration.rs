//! Resolve every direct GPU migration effect before profile installation writes.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use veoveo_deploy_contract::{
    ConflictingGpuDevicePluginRemoval, GpuSchedulingProfile,
    components::{
        AtomicTarget, ComponentId, LockedComponent, ObjectIdentity, validate_component_catalog,
    },
};

use crate::{
    compile::objects::ObjectScopes,
    configuration::append_yaml_bytes,
    helm_state::{HelmReleaseMetadata, release_metadata},
    ownership::{read_objects, validate_manager_value},
    process::{output_checked, status_checked},
};

#[derive(Debug, PartialEq, Eq)]
struct ObjectVersion {
    identity: ObjectIdentity,
    uid: String,
    resource_version: String,
}

impl ObjectVersion {
    fn observe(identity: ObjectIdentity, value: &Value) -> Result<Self> {
        let text = |pointer| {
            value
                .pointer(pointer)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .context("GPU migration object has no immutable observation")
        };
        ensure!(
            value["metadata"]["deletionTimestamp"].is_null(),
            "GPU migration object is already deleting"
        );
        Ok(Self {
            identity,
            uid: text("/metadata/uid")?,
            resource_version: text("/metadata/resourceVersion")?,
        })
    }
}

#[derive(Debug)]
enum Removal {
    None,
    DaemonSet(ObjectVersion),
    Helm {
        release: HelmReleaseMetadata,
        objects: Vec<ObjectVersion>,
        absent: BTreeSet<ObjectIdentity>,
    },
}

#[derive(Debug)]
pub(crate) struct PreparedGpuMigration {
    deployments: Vec<ObjectVersion>,
    absent_deployments: BTreeSet<ObjectIdentity>,
    removal: Removal,
}

pub(crate) fn prepare(
    context: &str,
    namespace: &str,
    scheduling: &GpuSchedulingProfile,
    catalog: &[LockedComponent],
    selected: &BTreeSet<ComponentId>,
) -> Result<PreparedGpuMigration> {
    let allocator = AtomicTarget::HelmRelease {
        namespace: scheduling.allocator.installation.namespace.clone(),
        name: scheduling.allocator.installation.release_name.clone(),
    };
    let targets = scheduling
        .same_physical_device_groups
        .iter()
        .flat_map(|group| &group.workloads)
        .map(|workload| ObjectIdentity {
            group: "apps".into(),
            kind: "Deployment".into(),
            namespace: Some(namespace.into()),
            name: workload.deployment.clone(),
        })
        .collect::<BTreeSet<_>>();
    prepare_targets(
        context,
        &scheduling
            .allocator
            .installation
            .conflicting_device_plugin_removal,
        &allocator,
        &targets,
        catalog,
        selected,
    )
}

fn prepare_targets(
    context: &str,
    policy: &ConflictingGpuDevicePluginRemoval,
    allocator: &AtomicTarget,
    targets: &BTreeSet<ObjectIdentity>,
    catalog: &[LockedComponent],
    selected: &BTreeSet<ComponentId>,
) -> Result<PreparedGpuMigration> {
    validate_component_catalog(catalog)?;
    let removal = prepare_removal(context, policy, catalog)?;
    if matches!(removal, Removal::None) {
        return Ok(PreparedGpuMigration {
            deployments: Vec::new(),
            absent_deployments: BTreeSet::new(),
            removal,
        });
    }
    ensure!(
        catalog
            .iter()
            .any(|component| selected.contains(&component.declaration.id)
                && component.declaration.targets.contains(allocator)),
        "GPU migration requires its allocator component in the selection"
    );
    let owners = selected_workload_owners(catalog, selected, targets)?;
    let live = read_objects(context, targets, None)?;
    let absent_deployments = targets
        .iter()
        .filter(|identity| !live.contains_key(*identity))
        .cloned()
        .collect();
    let mut deployments = Vec::new();
    for (identity, value) in live {
        validate_manager_value(&owners[&identity], &value)?;
        deployments.push(ObjectVersion::observe(identity, &value)?);
    }
    Ok(PreparedGpuMigration {
        deployments,
        absent_deployments,
        removal,
    })
}

#[cfg(test)]
mod tests;

fn selected_workload_owners(
    catalog: &[LockedComponent],
    selected: &BTreeSet<ComponentId>,
    targets: &BTreeSet<ObjectIdentity>,
) -> Result<BTreeMap<ObjectIdentity, AtomicTarget>> {
    let mut owners = BTreeMap::new();
    for target in targets {
        let (component, unit) = catalog
            .iter()
            .flat_map(|component| component.units.iter().map(move |unit| (component, unit)))
            .find(|(_, unit)| unit.objects.iter().any(|object| &object.identity == target))
            .with_context(|| {
                format!("GPU quiesce target {target:?} has no locked component owner")
            })?;
        ensure!(
            selected.contains(&component.declaration.id),
            "GPU migration would quiesce unselected component {}",
            component.declaration.id
        );
        owners.insert(target.clone(), unit.target.clone());
    }
    Ok(owners)
}

fn prepare_removal(
    context: &str,
    policy: &ConflictingGpuDevicePluginRemoval,
    catalog: &[LockedComponent],
) -> Result<Removal> {
    match policy {
        ConflictingGpuDevicePluginRemoval::RequireAbsent => Ok(Removal::None),
        ConflictingGpuDevicePluginRemoval::DeleteDaemonSet { namespace, name } => {
            let identity = ObjectIdentity {
                group: "apps".into(),
                kind: "DaemonSet".into(),
                namespace: Some(namespace.clone()),
                name: name.clone(),
            };
            validate_retirement(catalog, std::iter::once(&identity))?;
            let live = read_objects(context, &BTreeSet::from([identity.clone()]), None)?;
            let Some(value) = live.get(&identity) else {
                return Ok(Removal::None);
            };
            // The exact removal declaration authorizes this retired raw resource;
            // it cannot bypass another Helm or GitOps owner.
            validate_manager_value(
                &AtomicTarget::ManifestSet {
                    name: "gpu-device-plugin-retirement".into(),
                },
                value,
            )?;
            Ok(Removal::DaemonSet(ObjectVersion::observe(identity, value)?))
        }
        ConflictingGpuDevicePluginRemoval::UninstallHelmRelease {
            namespace,
            release_name,
            expected_chart_version,
        } => {
            let Some(release) = release_metadata(context, namespace, release_name)? else {
                return Ok(Removal::None);
            };
            if release.status == "uninstalled" {
                return Ok(Removal::None);
            }
            ensure!(
                matches!(release.status.as_str(), "deployed" | "failed"),
                "GPU device-plugin release has an active transition"
            );
            ensure!(
                release
                    .chart
                    .ends_with(&format!("-{expected_chart_version}")),
                "GPU device-plugin release chart differs from the declared removal version"
            );
            let target = AtomicTarget::HelmRelease {
                namespace: namespace.clone(),
                name: release_name.clone(),
            };
            ensure!(
                !catalog
                    .iter()
                    .any(|component| component.declaration.targets.contains(&target)),
                "GPU migration cannot uninstall a current component release"
            );
            let mut objects = Vec::new();
            for section in ["manifest", "hooks"] {
                let bytes = output_checked(
                    "helm",
                    [
                        "--kube-context",
                        context,
                        "get",
                        section,
                        release_name,
                        "--namespace",
                        namespace,
                        "--revision",
                        &release.revision.value()?.to_string(),
                    ],
                    None,
                )?;
                append_yaml_bytes(&bytes, "GPU device-plugin removal inventory", &mut objects)?;
            }
            for object in &objects {
                ensure!(
                    !object
                        .pointer("/metadata/annotations/helm.sh~1hook")
                        .and_then(Value::as_str)
                        .is_some_and(|hooks| hooks
                            .split(',')
                            .any(|hook| matches!(hook.trim(), "pre-delete" | "post-delete"))),
                    "GPU device-plugin retirement cannot execute deletion hooks"
                );
                ensure!(
                    !matches!(
                        object["kind"].as_str(),
                        Some("Namespace" | "Node" | "CustomResourceDefinition")
                    ),
                    "GPU device-plugin retirement contains a resource with effects outside its object inventory"
                );
            }
            let mut scopes = ObjectScopes::default();
            scopes.declare_crds(&objects)?;
            let inventory = scopes.rendered(&objects, namespace, &BTreeSet::new())?;
            let identities = inventory
                .into_iter()
                .map(|object| object.identity)
                .collect::<BTreeSet<_>>();
            validate_retirement(catalog, identities.iter())?;
            let live = read_objects(context, &identities, None)?;
            let absent = identities
                .iter()
                .filter(|identity| !live.contains_key(*identity))
                .cloned()
                .collect();
            let mut observed = Vec::new();
            for (identity, value) in live {
                validate_manager_value(&target, &value)?;
                observed.push(ObjectVersion::observe(identity, &value)?);
            }
            ensure!(
                release_metadata(context, namespace, release_name)?.as_ref() == Some(&release),
                "GPU device-plugin release changed during preflight"
            );
            Ok(Removal::Helm {
                release,
                objects: observed,
                absent,
            })
        }
    }
}

fn validate_retirement<'a>(
    catalog: &[LockedComponent],
    objects: impl Iterator<Item = &'a ObjectIdentity>,
) -> Result<()> {
    for identity in objects {
        ensure!(
            !catalog
                .iter()
                .any(|component| component.declaration.permitted_objects.contains(identity)),
            "GPU retirement overlaps a current component object {identity:?}"
        );
    }
    Ok(())
}

impl PreparedGpuMigration {
    pub(crate) fn apply(&self, context: &str) -> Result<()> {
        let removal_objects = match &self.removal {
            Removal::None => return Ok(()),
            Removal::DaemonSet(object) => std::slice::from_ref(object),
            Removal::Helm {
                release, objects, ..
            } => {
                ensure!(
                    release_metadata(context, &release.namespace, &release.name)?.as_ref()
                        == Some(release),
                    "GPU device-plugin release changed after preflight"
                );
                objects.as_slice()
            }
        };
        let mut absent = self.absent_deployments.clone();
        if let Removal::Helm {
            absent: retired, ..
        } = &self.removal
        {
            absent.extend(retired.iter().cloned());
        }
        ensure!(
            read_objects(context, &absent, None)?.is_empty(),
            "GPU migration object appeared after preflight"
        );
        verify_objects(context, self.deployments.iter().chain(removal_objects))?;
        for deployment in &self.deployments {
            let identity = &deployment.identity;
            let namespace = identity
                .namespace
                .as_deref()
                .context("GPU Deployment has no namespace")?;
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    namespace,
                    "scale",
                    "deployment",
                    &identity.name,
                    "--replicas=0",
                    "--resource-version",
                    &deployment.resource_version,
                ],
                &[],
                None,
            )?;
            status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    namespace,
                    "rollout",
                    "status",
                    &format!("deployment/{}", identity.name),
                    "--timeout=5m",
                ],
                &[],
                None,
            )?;
        }
        verify_objects(context, removal_objects.iter())?;
        ensure!(
            read_objects(context, &absent, None)?.is_empty(),
            "GPU migration object appeared while quiescing workloads"
        );
        if let Removal::Helm { release, .. } = &self.removal {
            ensure!(
                release_metadata(context, &release.namespace, &release.name)?.as_ref()
                    == Some(release),
                "GPU device-plugin release changed while quiescing workloads"
            );
        }
        match &self.removal {
            Removal::None => unreachable!(),
            Removal::DaemonSet(object) => status_checked(
                "kubectl",
                [
                    "--context",
                    context,
                    "--namespace",
                    object
                        .identity
                        .namespace
                        .as_deref()
                        .context("GPU DaemonSet has no namespace")?,
                    "delete",
                    "daemonset",
                    &object.identity.name,
                    "--wait=true",
                    "--timeout=5m",
                ],
                &[],
                None,
            ),
            Removal::Helm { release, .. } => status_checked(
                "helm",
                [
                    "--kube-context",
                    context,
                    "uninstall",
                    &release.name,
                    "--namespace",
                    &release.namespace,
                    "--wait",
                ],
                &[],
                None,
            ),
        }
    }
}

fn verify_objects<'a>(
    context: &str,
    expected: impl Iterator<Item = &'a ObjectVersion>,
) -> Result<()> {
    let expected = expected.collect::<Vec<_>>();
    let identities = expected
        .iter()
        .map(|object| object.identity.clone())
        .collect();
    let actual = read_objects(context, &identities, None)?;
    for expected in expected {
        let value = actual
            .get(&expected.identity)
            .context("GPU migration object disappeared after preflight")?;
        ensure!(
            ObjectVersion::observe(expected.identity.clone(), value)? == *expected,
            "GPU migration object changed after preflight"
        );
    }
    Ok(())
}
