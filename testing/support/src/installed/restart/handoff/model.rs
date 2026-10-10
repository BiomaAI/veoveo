use super::*;
use anyhow::Context;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use veoveo_types::Sha256Digest;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HandoffFixture {
    pub namespace_uid: Uuid,
    pub service: String,
    pub service_uid: Uuid,
    pub service_resource_version: String,
    pub service_spec_sha256: Sha256Digest,
    pub service_selector: BTreeMap<String, String>,
    pub service_port: u16,
    pub deployment_a: String,
    pub deployment_a_uid: Uuid,
    pub deployment_b: String,
    pub component_a: String,
    pub component_b: String,
    pub container: String,
    pub pod_a: String,
    pub pod_a_uid: Uuid,
    pub container_a_id: String,
    pub image: String,
    pub image_id: String,
    pub node: String,
    pub workload_spec_sha256: Sha256Digest,
    pub pvc: Option<Pvc>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pvc {
    pub name: String,
    pub uid: Uuid,
}
impl HandoffFixture {
    pub(super) fn validate(&self, target: &InstallationTarget) -> Result<()> {
        for name in [
            &self.service,
            &self.deployment_a,
            &self.deployment_b,
            &self.component_a,
            &self.component_b,
            &self.container,
            &self.pod_a,
            &self.node,
        ] {
            super::super::drain::name(name)?;
        }
        ensure!(
            target.expected_deployments.contains(&self.deployment_a),
            "original handoff Deployment is not declared"
        );
        ensure!(
            self.deployment_a != self.deployment_b && self.component_a != self.component_b,
            "handoff requires distinct workload selectors"
        );
        ensure!(
            self.service_port > 0
                && self.service_selector.get("app.kubernetes.io/component")
                    == Some(&self.component_a),
            "handoff original Service selector differs"
        );
        ensure!(
            [
                self.namespace_uid,
                self.service_uid,
                self.deployment_a_uid,
                self.pod_a_uid
            ]
            .iter()
            .all(|id| !id.is_nil()),
            "handoff identities must be nonzero"
        );
        ensure!(
            !self.container_a_id.is_empty()
                && self.image.contains("@sha256:")
                && !self.image_id.is_empty(),
            "handoff requires immutable process image"
        );
        let (_, digest) = self
            .image
            .rsplit_once('@')
            .context("immutable handoff image digest absent")?;
        Sha256Digest::parse(digest)?;
        ensure!(
            !self.service_resource_version.is_empty(),
            "original Service version absent"
        );
        if let Some(pvc) = &self.pvc {
            super::super::drain::name(&pvc.name)?;
            ensure!(!pvc.uid.is_nil(), "authority PVC identity is zero");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum HandoffPhase {
    OriginalOnly,
    OriginalRouting,
    BothReady,
    ReplacementOnly,
    OriginalRestoredWithReplacementRouting,
    Restored,
}
#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffReceipt {
    pub original_admitted: bool,
    pub original_exit_observed: bool,
    pub original_exit_code: Option<i32>,
    pub original_exit_finished_at: Option<DateTime<Utc>>,
    pub deployment_b_uid: Option<Uuid>,
    pub pod_b_uid: Option<Uuid>,
    pub container_b_id: Option<String>,
    pub container_b_restart_count: Option<u32>,
    pub pod_b_created_at: Option<DateTime<Utc>>,
    pub service_resource_version: Option<String>,
    pub watch_events: u64,
    pub restored: bool,
    pub watches_closed: bool,
    pub barrier_nonce: Option<Uuid>,
    pub barrier_observed: bool,
    pub assertion_versions: Vec<HandoffResourceVersion>,
    pub final_versions: Vec<HandoffResourceVersion>,
    pub observations: Vec<HandoffObservation>,
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct Metadata {
    pub name: String,
    pub uid: Uuid,
    pub resource_version: String,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub annotations: BTreeMap<String, String>,
    #[serde(default)]
    pub owner_references: Vec<Owner>,
    pub creation_timestamp: Option<DateTime<Utc>>,
    pub deletion_timestamp: Option<DateTime<Utc>>,
}
#[derive(Deserialize, Clone)]
pub(super) struct Owner {
    pub kind: String,
    pub uid: Uuid,
    pub controller: Option<bool>,
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct Object {
    pub metadata: Metadata,
    #[serde(default)]
    pub spec: serde_json::Value,
    #[serde(default)]
    pub status: serde_json::Value,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    #[serde(default)]
    pub ports: Vec<Port>,
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct Endpoint {
    pub addresses: Vec<String>,
    pub target_ref: Option<Reference>,
    pub conditions: Conditions,
}
#[derive(Deserialize, Clone)]
pub(super) struct Reference {
    pub kind: String,
    pub uid: Uuid,
}
#[derive(Deserialize, Clone)]
pub(super) struct Conditions {
    pub ready: Option<bool>,
    pub terminating: Option<bool>,
}
#[derive(Deserialize, Clone)]
pub(super) struct Port {
    pub port: Option<u16>,
    pub name: Option<String>,
    pub protocol: Option<String>,
}
pub(super) struct Inventory {
    pub resource_version: String,
    pub objects: BTreeMap<Uuid, Object>,
}
fn named<'a>(objects: &'a BTreeMap<Uuid, Object>, name: &str) -> Result<&'a Object> {
    let mut selected = objects.values().filter(|o| o.metadata.name == name);
    let result = selected
        .next()
        .ok_or_else(|| anyhow::anyhow!("handoff object absent"))?;
    ensure!(selected.next().is_none(), "handoff object duplicated");
    Ok(result)
}
fn deployment_spec(object: &Object) -> Result<serde_json::Value> {
    let spec = object
        .spec
        .pointer("/template/spec")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("handoff pod template absent"))?;
    normalize_workload(spec)
}
fn normalize_workload(mut spec: serde_json::Value) -> Result<serde_json::Value> {
    if let Some(map) = spec.as_object_mut() {
        map.remove("nodeName");
        if let Some(alias) = map.remove("serviceAccount") {
            ensure!(
                map.get("serviceAccountName") == Some(&alias),
                "workload service-account alias disagrees"
            );
        }
    }
    // Node pinning and mount access are the two admitted orchestration deltas.
    // Normalize both templates; live admission separately requires B read-only.
    for field in ["containers", "initContainers"] {
        for container in spec
            .get_mut(field)
            .and_then(|v| v.as_array_mut())
            .into_iter()
            .flatten()
        {
            for mount in container
                .get_mut("volumeMounts")
                .and_then(|v| v.as_array_mut())
                .into_iter()
                .flatten()
            {
                if mount.get("name").and_then(|v| v.as_str()) == Some("workspace") {
                    mount
                        .as_object_mut()
                        .context("workspace mount must be an object")?
                        .remove("readOnly");
                }
            }
        }
    }
    Ok(spec)
}
pub(super) fn digest(value: &serde_json::Value) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_hex(hex::encode(Sha256::digest(
        serde_json::to_vec(value)?,
    )))?)
}
fn admit_pod_template(pod: &Object, template: &Object) -> Result<()> {
    let expected = deployment_spec(template)?;
    let mut actual = normalize_workload(pod.spec.clone())?;
    let map = actual
        .as_object_mut()
        .context("handoff Pod specification invalid")?;
    // API-server/scheduler fields absent from the admitted template have only
    // these supported defaults. All containers, env, mounts, resources, volumes,
    // security and configured scheduling fields otherwise compare in full.
    for (field, default) in [
        ("priority", serde_json::json!(0)),
        (
            "preemptionPolicy",
            serde_json::json!("PreemptLowerPriority"),
        ),
        ("enableServiceLinks", serde_json::json!(true)),
        ("dnsPolicy", serde_json::json!("ClusterFirst")),
        ("schedulerName", serde_json::json!("default-scheduler")),
        ("restartPolicy", serde_json::json!("Always")),
        ("terminationGracePeriodSeconds", serde_json::json!(30)),
        ("securityContext", serde_json::json!({})),
        ("imagePullSecrets", serde_json::json!([])),
    ] {
        if expected.get(field).is_none() && map.get(field) == Some(&default) {
            map.remove(field);
        }
    }
    if let Some(alias) = map.remove("serviceAccount") {
        ensure!(
            map.get("serviceAccountName") == Some(&alias),
            "Pod service-account alias disagrees"
        );
    }
    if let Some(tolerations) = map.get_mut("tolerations").and_then(|v| v.as_array_mut()) {
        let configured = expected.get("tolerations").and_then(|v| v.as_array());
        tolerations.retain(|value| configured.is_some_and(|values| values.contains(value)) || ![
            serde_json::json!({"key":"node.kubernetes.io/not-ready","operator":"Exists","effect":"NoExecute","tolerationSeconds":300}),
            serde_json::json!({"key":"node.kubernetes.io/unreachable","operator":"Exists","effect":"NoExecute","tolerationSeconds":300}),
        ].contains(value));
        if tolerations.is_empty() && configured.is_none() {
            map.remove("tolerations");
        }
    }
    ensure!(
        actual == expected,
        "running Pod configuration differs from admitted template"
    );
    Ok(())
}
fn ready(pod: &Object, fixture: &HandoffFixture, replacement: bool) -> Result<bool> {
    let Some(node) = pod
        .spec
        .get("nodeName")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    else {
        return Ok(false);
    };
    ensure!(node == fixture.node, "handoff node changed");
    let containers = pod
        .spec
        .get("containers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("handoff containers absent"))?;
    let container = containers
        .iter()
        .find(|v| v.get("name").and_then(|v| v.as_str()) == Some(&fixture.container))
        .ok_or_else(|| anyhow::anyhow!("handoff server container absent"))?;
    ensure!(
        container.get("image").and_then(|v| v.as_str()) == Some(&fixture.image),
        "handoff image changed"
    );
    if replacement && fixture.pvc.is_some() {
        ensure!(
            container
                .get("volumeMounts")
                .and_then(|v| v.as_array())
                .is_some_and(|v| v.iter().any(|m| m.get("name").and_then(|v| v.as_str())
                    == Some("workspace")
                    && m.get("readOnly").and_then(|v| v.as_bool()) == Some(true))),
            "replacement authority volume must be read-only"
        );
    }
    if replacement && fixture.pvc.is_some() {
        for field in ["containers", "initContainers"] {
            for container in pod
                .spec
                .get(field)
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                for mount in container
                    .get("volumeMounts")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    ensure!(
                        mount.get("name").and_then(|v| v.as_str()) != Some("workspace")
                            || mount.get("readOnly").and_then(|v| v.as_bool()) == Some(true),
                        "replacement authority mount permits a writer"
                    );
                }
            }
        }
    }
    if let Some(pvc) = &fixture.pvc {
        ensure!(
            pod.spec
                .get("volumes")
                .and_then(|v| v.as_array())
                .is_some_and(|v| v.iter().any(|v| v.get("name").and_then(|v| v.as_str())
                    == Some("workspace")
                    && v.pointer("/persistentVolumeClaim/claimName")
                        .and_then(|v| v.as_str())
                        == Some(&pvc.name))),
            "handoff authority PVC binding changed"
        );
    }
    let Some(status) = pod
        .status
        .get("containerStatuses")
        .and_then(|v| v.as_array())
        .and_then(|values| {
            values.iter().find(|value| {
                value.get("name").and_then(|value| value.as_str()) == Some(&fixture.container)
            })
        })
    else {
        return Ok(false);
    };
    let Some(image_id) = status
        .get("imageID")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
    else {
        return Ok(false);
    };
    ensure!(
        image_id == fixture.image_id,
        "handoff runnable image changed"
    );
    Ok(pod.metadata.deletion_timestamp.is_none()
        && status.get("ready").and_then(|v| v.as_bool()) == Some(true)
        && status.pointer("/state/running").is_some())
}
pub(super) fn check(
    f: &HandoffFixture,
    all: &BTreeMap<watch::Kind, Inventory>,
    r: &mut HandoffReceipt,
    phase: HandoffPhase,
) -> Result<bool> {
    use watch::Kind::*;
    let objects = |kind| {
        all.get(&kind)
            .map(|v| &v.objects)
            .ok_or_else(|| anyhow::anyhow!("handoff inventory incomplete"))
    };
    let deployments = objects(Deployments)?;
    let pods = objects(Pods)?;
    let replica_sets = objects(ReplicaSets)?;
    let services = objects(Services)?;
    let slices = objects(Slices)?;
    let a = named(deployments, &f.deployment_a)?;
    ensure!(
        a.metadata.uid == f.deployment_a_uid,
        "original Deployment replaced"
    );
    ensure!(
        digest(&deployment_spec(a)?)? == f.workload_spec_sha256,
        "original configuration changed"
    );
    let service = named(services, &f.service)?;
    ensure!(
        service.metadata.uid == f.service_uid,
        "handoff Service replaced"
    );
    let mut service_spec = service.spec.clone();
    service_spec
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("handoff Service specification absent"))?
        .remove("selector");
    ensure!(
        digest(&service_spec)? == f.service_spec_sha256,
        "handoff Service configuration changed"
    );
    if phase == HandoffPhase::OriginalOnly {
        ensure!(
            service.metadata.resource_version == f.service_resource_version,
            "original Service version changed before baseline"
        );
    }
    let selector: BTreeMap<String, String> =
        serde_json::from_value(service.spec.get("selector").cloned().unwrap_or_default())?;
    let mut b_selector = f.service_selector.clone();
    b_selector.insert("app.kubernetes.io/component".into(), f.component_b.clone());
    let replacement_routing = matches!(
        phase,
        HandoffPhase::ReplacementOnly | HandoffPhase::OriginalRestoredWithReplacementRouting
    );
    if selector
        != if replacement_routing {
            b_selector
        } else {
            f.service_selector.clone()
        }
    {
        return Ok(false);
    }
    ensure!(
        service
            .spec
            .get("ports")
            .and_then(|v| v.as_array())
            .is_some_and(|ports| ports.len() == 1
                && ports[0].get("port").and_then(|v| v.as_u64())
                    == Some(u64::from(f.service_port))),
        "handoff Service port changed"
    );
    r.service_resource_version = Some(service.metadata.resource_version.clone());
    let a_pods: Vec<_> = pods
        .values()
        .filter(|p| p.metadata.labels.get("app.kubernetes.io/component") == Some(&f.component_a))
        .collect();
    let b_pods: Vec<_> = pods
        .values()
        .filter(|p| p.metadata.labels.get("app.kubernetes.io/component") == Some(&f.component_b))
        .collect();
    let b = deployments
        .values()
        .find(|d| d.metadata.name == f.deployment_b);
    for (selected, deployment) in [(&a_pods, Some(a)), (&b_pods, b)] {
        for pod in selected {
            let owner = pod
                .metadata
                .owner_references
                .iter()
                .filter(|o| o.kind == "ReplicaSet" && o.controller == Some(true))
                .collect::<Vec<_>>();
            ensure!(
                owner.len() == 1,
                "handoff Pod requires one controlling ReplicaSet"
            );
            let Some(rs) = replica_sets.get(&owner[0].uid) else {
                return Ok(false);
            };
            if deployment.is_none() {
                return Ok(false);
            }
            ensure!(
                digest(&deployment_spec(rs)?)? == f.workload_spec_sha256,
                "ReplicaSet template differs from admitted workload"
            );
            admit_pod_template(pod, rs)?;
            ensure!(
                deployment.is_some_and(|d| rs
                    .metadata
                    .owner_references
                    .iter()
                    .any(|o| o.kind == "Deployment"
                        && o.controller == Some(true)
                        && o.uid == d.metadata.uid)),
                "handoff Pod belongs to another Deployment"
            );
        }
    }
    let original_only = phase == HandoffPhase::OriginalOnly;
    if original_only {
        ensure!(
            b.is_none() && b_pods.is_empty(),
            "replacement existed before original baseline"
        );
    }
    let a_absent = phase == HandoffPhase::ReplacementOnly;
    if a_absent {
        if !r.original_exit_observed
            || a.spec.get("replicas").and_then(|v| v.as_u64()) != Some(0)
            || !a_pods.is_empty()
        {
            return Ok(false);
        }
    } else if a.spec.get("replicas").and_then(|v| v.as_u64()) != Some(1)
        || a_pods.len() != 1
        || !ready(a_pods[0], f, false)?
    {
        return Ok(false);
    }
    if original_only {
        ensure!(
            a_pods[0]
                .status
                .get("containerStatuses")
                .and_then(|v| v.as_array())
                .is_some_and(|v| v.iter().any(|v| v.get("name").and_then(|v| v.as_str())
                    == Some(&f.container)
                    && v.get("containerID").and_then(|v| v.as_str()) == Some(&f.container_a_id))),
            "original process changed before baseline"
        );
        ensure!(
            a_pods[0].metadata.uid == f.pod_a_uid && a_pods[0].metadata.name == f.pod_a,
            "original Pod changed before baseline"
        );
    }
    let b_uid = if matches!(
        phase,
        HandoffPhase::BothReady
            | HandoffPhase::ReplacementOnly
            | HandoffPhase::OriginalRestoredWithReplacementRouting
            | HandoffPhase::OriginalRouting
    ) && (phase != HandoffPhase::OriginalRouting || b.is_some())
    {
        let Some(b) = b else { return Ok(false) };
        if b.spec.get("replicas").and_then(|v| v.as_u64()) != Some(1) {
            return Ok(false);
        }
        let labels: BTreeMap<String, String> = serde_json::from_value(
            b.spec
                .pointer("/template/metadata/labels")
                .cloned()
                .unwrap_or_default(),
        )?;
        ensure!(
            f.service_selector.iter().all(|(key, value)| labels.get(key)
                == Some(if key == "app.kubernetes.io/component" {
                    &f.component_b
                } else {
                    value
                })),
            "replacement does not preserve installation labels with distinct orchestration selector"
        );
        ensure!(
            digest(&deployment_spec(b)?)? == f.workload_spec_sha256,
            "replacement configuration differs"
        );
        if let Some(uid) = r.deployment_b_uid {
            ensure!(uid == b.metadata.uid, "replacement Deployment changed");
        } else {
            r.deployment_b_uid = Some(b.metadata.uid);
        }
        if phase == HandoffPhase::OriginalRouting {
            ensure!(
                b_pods.len() <= 1,
                "unexpected replacement scale during original-route cleanup"
            );
            if let Some(pod) = b_pods.first() {
                ensure!(
                    pod.metadata.uid != f.pod_a_uid
                        && r.pod_b_uid.is_none_or(|uid| uid == pod.metadata.uid),
                    "replacement Pod identity changed during cleanup"
                );
                r.pod_b_uid = Some(pod.metadata.uid);
                r.pod_b_created_at = pod.metadata.creation_timestamp;
            }
            b_pods.first().map(|pod| pod.metadata.uid)
        } else {
            if b_pods.len() != 1 || !ready(b_pods[0], f, true)? {
                return Ok(false);
            }
            let pod = b_pods[0];
            ensure!(
                f.service_selector
                    .iter()
                    .all(|(key, value)| pod.metadata.labels.get(key)
                        == Some(if key == "app.kubernetes.io/component" {
                            &f.component_b
                        } else {
                            value
                        })),
                "replacement Pod differs from full Service selector"
            );
            let statuses = pod
                .status
                .get("containerStatuses")
                .and_then(|v| v.as_array())
                .unwrap();
            let status = statuses
                .iter()
                .find(|v| v.get("name").and_then(|v| v.as_str()) == Some(&f.container))
                .unwrap();
            let container_id = status
                .get("containerID")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| anyhow::anyhow!("replacement process identity absent"))?;
            let restart_count = status
                .get("restartCount")
                .and_then(|v| v.as_u64())
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| anyhow::anyhow!("replacement restart count absent"))?;
            if let Some(previous) = &r.container_b_id {
                ensure!(
                    previous == container_id && r.container_b_restart_count == Some(restart_count),
                    "replacement process restarted"
                );
            } else {
                r.container_b_id = Some(container_id.to_owned());
                r.container_b_restart_count = Some(restart_count);
            }

            ensure!(
                pod.metadata.uid != f.pod_a_uid,
                "handoff reused original Pod"
            );
            if let Some(uid) = r.pod_b_uid {
                ensure!(uid == pod.metadata.uid, "replacement Pod changed");
            } else {
                r.pod_b_uid = Some(pod.metadata.uid);
                r.pod_b_created_at = pod.metadata.creation_timestamp;
            }
            Some(pod.metadata.uid)
        }
    } else {
        if phase == HandoffPhase::Restored && (b.is_some() || !b_pods.is_empty()) {
            return Ok(false);
        }
        None
    };
    let expected = if replacement_routing {
        b_uid.unwrap()
    } else {
        a_pods[0].metadata.uid
    };
    let mut endpoint_count = 0;
    for slice in slices.values() {
        ensure!(
            slice.metadata.labels.get("kubernetes.io/service-name") == Some(&f.service)
                && slice
                    .metadata
                    .owner_references
                    .iter()
                    .any(|o| o.kind == "Service"
                        && o.uid == f.service_uid
                        && o.controller == Some(true)),
            "foreign EndpointSlice"
        );
        ensure!(
            slice.ports.len() == 1
                && slice.ports[0].port == Some(f.service_port)
                && slice.ports[0].name.as_deref() == Some("http")
                && slice.ports[0].protocol.as_deref() == Some("TCP"),
            "handoff endpoint port differs"
        );
        for endpoint in &slice.endpoints {
            endpoint_count += 1;
            if endpoint.conditions.ready != Some(true)
                || endpoint.conditions.terminating == Some(true)
                || endpoint.addresses.len() != 1
                || pods
                    .get(&expected)
                    .and_then(|p| p.status.get("podIP"))
                    .and_then(|v| v.as_str())
                    != endpoint.addresses.first().map(String::as_str)
                || !endpoint
                    .target_ref
                    .as_ref()
                    .is_some_and(|v| v.kind == "Pod" && v.uid == expected)
            {
                return Ok(false);
            }
        }
    }
    if endpoint_count != 1 {
        return Ok(false);
    }
    if phase == HandoffPhase::Restored {
        r.restored = true;
    }
    let observation = HandoffObservation {
        phase,
        service_resource_version: service.metadata.resource_version.clone(),
        selector,
        endpoint_pod_uid: expected,
        pod_a_uid: a_pods.first().map(|pod| pod.metadata.uid),
        pod_b_uid: b_pods.first().map(|pod| pod.metadata.uid),
        deployment_a_resource_version: a.metadata.resource_version.clone(),
        deployment_b_resource_version: b
            .map(|deployment| deployment.metadata.resource_version.clone()),
    };
    if r.observations.last() != Some(&observation) {
        ensure!(
            r.observations.len() < 128,
            "handoff observation budget exceeded"
        );
        r.observations.push(observation);
    }
    Ok(true)
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffObservation {
    pub phase: HandoffPhase,
    pub service_resource_version: String,
    pub selector: BTreeMap<String, String>,
    pub endpoint_pod_uid: Uuid,
    pub pod_a_uid: Option<Uuid>,
    pub pod_b_uid: Option<Uuid>,
    pub deployment_a_resource_version: String,
    pub deployment_b_resource_version: Option<String>,
}

pub(super) fn observe_exit(
    f: &HandoffFixture,
    pod: &Object,
    receipt: &mut HandoffReceipt,
) -> Result<()> {
    if pod.metadata.uid != f.pod_a_uid {
        return Ok(());
    }
    if let Some(status) = pod
        .status
        .get("containerStatuses")
        .and_then(|v| v.as_array())
        .and_then(|values| {
            values
                .iter()
                .find(|v| v.get("name").and_then(|v| v.as_str()) == Some(&f.container))
        })
    {
        ensure!(
            status.get("containerID").and_then(|v| v.as_str()) == Some(&f.container_a_id),
            "original process identity changed before exit"
        );
        if let Some(terminated) = status.pointer("/state/terminated") {
            let code = terminated
                .get("exitCode")
                .and_then(|v| v.as_i64())
                .and_then(|value| i32::try_from(value).ok())
                .context("original process exit code absent")?;
            let finished_at = terminated
                .get("finishedAt")
                .and_then(|v| v.as_str())
                .context("original process exit time absent")?
                .parse::<DateTime<Utc>>()?;
            if let Some(previous) = receipt.original_exit_code {
                ensure!(
                    previous == code && receipt.original_exit_finished_at == Some(finished_at),
                    "original process exit facts changed"
                );
            }
            receipt.original_exit_code = Some(code);
            receipt.original_exit_finished_at = Some(finished_at);
            ensure!(code == 0, "original process did not exit successfully");
            receipt.original_exit_observed = true;
        }
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandoffResourceVersion {
    pub kind: watch::Kind,
    pub uid: Uuid,
    pub resource_version: String,
}
pub(super) fn versions(
    f: &HandoffFixture,
    all: &BTreeMap<watch::Kind, Inventory>,
    r: &HandoffReceipt,
) -> Result<Vec<HandoffResourceVersion>> {
    use watch::Kind::*;
    let mut versions = Vec::new();
    for kind in [Deployments, ReplicaSets, Services, Slices] {
        let inventory = all.get(&kind).context("handoff version inventory absent")?;
        for object in inventory.objects.values() {
            let selected = match kind {
                Deployments => {
                    object.metadata.uid == f.deployment_a_uid
                        || Some(object.metadata.uid) == r.deployment_b_uid
                }
                ReplicaSets => object.metadata.owner_references.iter().any(|owner| {
                    owner.kind == "Deployment"
                        && owner.controller == Some(true)
                        && (owner.uid == f.deployment_a_uid
                            || Some(owner.uid) == r.deployment_b_uid)
                }),
                Services => object.metadata.uid == f.service_uid,
                Slices => true,
                Pods => false,
            };
            if selected {
                versions.push(HandoffResourceVersion {
                    kind,
                    uid: object.metadata.uid,
                    resource_version: object.metadata.resource_version.clone(),
                });
            }
        }
    }
    Ok(versions)
}
pub(super) fn observe_marker(r: &mut HandoffReceipt, pod: &Object) -> Result<()> {
    let Some(nonce) = r.barrier_nonce else {
        return Ok(());
    };
    if let Some(marker) = pod.metadata.annotations.get("veoveo.ai/acceptance-fence") {
        ensure!(
            Some(pod.metadata.uid) == r.pod_b_uid && marker == &nonce.to_string(),
            "foreign or replayed handoff barrier marker"
        );
        r.barrier_observed = true;
    }
    Ok(())
}
