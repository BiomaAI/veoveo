use std::collections::BTreeMap;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use veoveo_deploy_contract::GpuWorkloadPlacement;

use crate::deployment::output_checked;

const DEPLOYMENT_REVISION_ANNOTATION: &str = "deployment.kubernetes.io/revision";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Deployment {
    metadata: ObjectMetadata,
    spec: DeploymentSpec,
    #[serde(default)]
    status: DeploymentStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentSpec {
    #[serde(default = "one_replica")]
    replicas: u16,
    selector: LabelSelector,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentStatus {
    #[serde(default)]
    ready_replicas: u16,
    #[serde(default)]
    available_replicas: u16,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelSelector {
    #[serde(default)]
    match_labels: BTreeMap<String, String>,
    #[serde(default)]
    match_expressions: Vec<LabelSelectorExpression>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LabelSelectorExpression {
    key: String,
    operator: LabelSelectorOperator,
    #[serde(default)]
    values: Vec<String>,
}

#[derive(Debug, Deserialize)]
enum LabelSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObjectMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    uid: String,
    #[serde(default)]
    annotations: BTreeMap<String, String>,
    deletion_timestamp: Option<String>,
    #[serde(default)]
    owner_references: Vec<OwnerReference>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OwnerReference {
    #[serde(default)]
    controller: bool,
    kind: String,
    uid: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReplicaSetList {
    items: Vec<ReplicaSet>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ReplicaSet {
    metadata: ObjectMetadata,
}

#[derive(Debug, Deserialize, Serialize)]
struct PodList {
    items: Vec<Pod>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Pod {
    metadata: ObjectMetadata,
    #[serde(default)]
    status: PodStatus,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PodStatus {
    #[serde(default)]
    conditions: Vec<PodCondition>,
    #[serde(default)]
    container_statuses: Vec<ContainerStatus>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PodCondition {
    #[serde(rename = "type")]
    kind: String,
    status: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ContainerStatus {
    name: String,
    ready: bool,
}

const fn one_replica() -> u16 {
    1
}

pub(super) fn ready_gpu_workload_pods(
    context: &str,
    namespace: &str,
    workload: &GpuWorkloadPlacement,
) -> Result<Vec<String>> {
    let deployment = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "deployment",
            workload.deployment.as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let deployment: Deployment =
        serde_json::from_slice(&deployment).context("decoding GPU workload Deployment")?;
    validate_replicas(&deployment, workload)?;
    let selector = deployment_pod_selector(&deployment.spec.selector)?;

    let replica_sets = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "replicasets",
            "-l",
            selector.as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let replica_sets: ReplicaSetList =
        serde_json::from_slice(&replica_sets).context("decoding GPU workload ReplicaSets")?;
    let pods = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "pods",
            "-l",
            selector.as_str(),
            "-o",
            "json",
        ],
        None,
    )?;
    let pods: PodList = serde_json::from_slice(&pods).context("decoding GPU workload pods")?;
    current_deployment_pods(&deployment, &replica_sets, &pods, workload)
}

fn validate_replicas(deployment: &Deployment, workload: &GpuWorkloadPlacement) -> Result<()> {
    let expected = workload.replicas;
    ensure!(
        deployment.spec.replicas == expected
            && deployment.status.ready_replicas == expected
            && deployment.status.available_replicas == expected,
        "GPU workload {} Deployment {} has desired={} ready={} available={}; expected exactly {expected}",
        workload.workload,
        workload.deployment,
        deployment.spec.replicas,
        deployment.status.ready_replicas,
        deployment.status.available_replicas
    );
    Ok(())
}

fn deployment_pod_selector(selector: &LabelSelector) -> Result<String> {
    let mut requirements = Vec::new();
    for (key, value) in &selector.match_labels {
        ensure_selector_atom(key, "label key")?;
        ensure_selector_atom(value, "label value")?;
        requirements.push(format!("{key}={value}"));
    }
    for expression in &selector.match_expressions {
        ensure_selector_atom(&expression.key, "expression key")?;
        for value in &expression.values {
            ensure_selector_atom(value, "expression value")?;
        }
        let requirement = match expression.operator {
            LabelSelectorOperator::In => {
                ensure!(
                    !expression.values.is_empty(),
                    "selector In expression has no values"
                );
                format!("{} in ({})", expression.key, expression.values.join(","))
            }
            LabelSelectorOperator::NotIn => {
                ensure!(
                    !expression.values.is_empty(),
                    "selector NotIn expression has no values"
                );
                format!("{} notin ({})", expression.key, expression.values.join(","))
            }
            LabelSelectorOperator::Exists => {
                ensure!(
                    expression.values.is_empty(),
                    "selector Exists expression has values"
                );
                expression.key.clone()
            }
            LabelSelectorOperator::DoesNotExist => {
                ensure!(
                    expression.values.is_empty(),
                    "selector DoesNotExist expression has values"
                );
                format!("!{}", expression.key)
            }
        };
        requirements.push(requirement);
    }
    ensure!(
        !requirements.is_empty(),
        "GPU workload Deployment selector is empty"
    );
    requirements.sort();
    Ok(requirements.join(","))
}

fn ensure_selector_atom(value: &str, label: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && !value
                .chars()
                .any(|character| matches!(character, ',' | '(' | ')' | '!' | '=')),
        "GPU workload Deployment selector {label} cannot be encoded safely: {value:?}"
    );
    Ok(())
}

fn current_deployment_pods(
    deployment: &Deployment,
    replica_sets: &ReplicaSetList,
    pods: &PodList,
    workload: &GpuWorkloadPlacement,
) -> Result<Vec<String>> {
    let deployment_uid = nonempty(&deployment.metadata.uid, "GPU workload Deployment UID")?;
    let deployment_revision = deployment
        .metadata
        .annotations
        .get(DEPLOYMENT_REVISION_ANNOTATION)
        .map(String::as_str)
        .context("GPU workload Deployment has no rollout revision")?;
    let current_replica_sets = replica_sets
        .items
        .iter()
        .filter(|replica_set| {
            replica_set
                .metadata
                .annotations
                .get(DEPLOYMENT_REVISION_ANNOTATION)
                .map(String::as_str)
                == Some(deployment_revision)
                && controlled_by(&replica_set.metadata, "Deployment", deployment_uid)
        })
        .collect::<Vec<_>>();
    ensure!(
        current_replica_sets.len() == 1,
        "GPU workload {} Deployment {} expected one current ReplicaSet at revision {} but found {}",
        workload.workload,
        workload.deployment,
        deployment_revision,
        current_replica_sets.len()
    );
    let replica_set_uid = nonempty(
        &current_replica_sets[0].metadata.uid,
        "current GPU workload ReplicaSet UID",
    )?;
    let mut names = Vec::new();
    for pod in &pods.items {
        let name = nonempty(&pod.metadata.name, "selected GPU workload pod name")?;
        ensure!(
            pod.metadata.deletion_timestamp.is_none(),
            "GPU workload {} selected terminating pod {name}",
            workload.workload
        );
        ensure!(
            controlled_by(&pod.metadata, "ReplicaSet", replica_set_uid),
            "GPU workload {} selected pod {name} is not owned by the current ReplicaSet",
            workload.workload
        );
        let ready = pod
            .status
            .conditions
            .iter()
            .any(|condition| condition.kind == "Ready" && condition.status == "True");
        ensure!(
            ready,
            "GPU workload {} selected pod {name} is not Ready",
            workload.workload
        );
        let container_ready = pod
            .status
            .container_statuses
            .iter()
            .any(|container| container.name == workload.container && container.ready);
        ensure!(
            container_ready,
            "GPU workload {} selected pod {name} container {} is not Ready",
            workload.workload,
            workload.container
        );
        names.push(name.to_owned());
    }
    names.sort();
    ensure!(
        names.len() == usize::from(workload.replicas),
        "GPU workload {} expected {} replicas but found {} current Ready pods",
        workload.workload,
        workload.replicas,
        names.len()
    );
    Ok(names)
}

fn controlled_by(metadata: &ObjectMetadata, kind: &str, uid: &str) -> bool {
    metadata
        .owner_references
        .iter()
        .any(|owner| owner.controller && owner.kind == kind && owner.uid == uid)
}

fn nonempty<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    if value.is_empty() {
        bail!("{label} is empty")
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn workload_inventory() -> (Deployment, ReplicaSetList, PodList, GpuWorkloadPlacement) {
        let deployment = serde_json::from_value(json!({
            "metadata": {
                "name": "example-runtime",
                "uid": "deployment-uid",
                "annotations": {"deployment.kubernetes.io/revision": "7"}
            },
            "spec": {
                "replicas": 1,
                "selector": {
                    "matchLabels": {
                        "app.kubernetes.io/instance": "example-installation",
                        "app.kubernetes.io/component": "runtime"
                    },
                    "matchExpressions": [{
                        "key": "environment",
                        "operator": "In",
                        "values": ["qualification"]
                    }]
                }
            },
            "status": {"readyReplicas": 1, "availableReplicas": 1}
        }))
        .unwrap();
        let replica_sets = serde_json::from_value(json!({"items": [{
            "metadata": {
                "name": "example-runtime-current",
                "uid": "replica-set-uid",
                "annotations": {"deployment.kubernetes.io/revision": "7"},
                "ownerReferences": [{
                    "controller": true,
                    "kind": "Deployment",
                    "uid": "deployment-uid"
                }]
            }
        }]}))
        .unwrap();
        let pods = serde_json::from_value(json!({"items": [{
            "metadata": {
                "name": "example-runtime-pod",
                "ownerReferences": [{
                    "controller": true,
                    "kind": "ReplicaSet",
                    "uid": "replica-set-uid"
                }]
            },
            "status": {
                "conditions": [{"type": "Ready", "status": "True"}],
                "containerStatuses": [{"name": "runtime", "ready": true}]
            }
        }]}))
        .unwrap();
        let workload = GpuWorkloadPlacement {
            workload: "example-runtime".to_owned(),
            deployment: "example-runtime".to_owned(),
            container: "runtime".to_owned(),
            replicas: 1,
        };
        (deployment, replica_sets, pods, workload)
    }

    #[test]
    fn selector_comes_from_the_deployment_contract() {
        let (deployment, replica_sets, pods, workload) = workload_inventory();

        assert_eq!(
            deployment_pod_selector(&deployment.spec.selector).unwrap(),
            "app.kubernetes.io/component=runtime,app.kubernetes.io/instance=example-installation,environment in (qualification)"
        );
        assert_eq!(
            current_deployment_pods(&deployment, &replica_sets, &pods, &workload).unwrap(),
            ["example-runtime-pod"]
        );
    }

    #[test]
    fn selector_supports_all_kubernetes_expression_operators() {
        let selector: LabelSelector = serde_json::from_value(json!({
            "matchExpressions": [
                {"key": "included", "operator": "In", "values": ["a", "b"]},
                {"key": "excluded", "operator": "NotIn", "values": ["c"]},
                {"key": "present", "operator": "Exists"},
                {"key": "absent", "operator": "DoesNotExist"}
            ]
        }))
        .unwrap();

        assert_eq!(
            deployment_pod_selector(&selector).unwrap(),
            "!absent,excluded notin (c),included in (a,b),present"
        );
    }

    #[test]
    fn verifier_rejects_stale_terminating_or_unready_pods() {
        let (deployment, replica_sets, pods, workload) = workload_inventory();

        let mut terminating = serde_json::to_value(&pods).unwrap();
        terminating["items"][0]["metadata"]["deletionTimestamp"] = json!("2026-01-01T00:00:00Z");
        let terminating: PodList = serde_json::from_value(terminating).unwrap();
        assert!(
            current_deployment_pods(&deployment, &replica_sets, &terminating, &workload)
                .unwrap_err()
                .to_string()
                .contains("terminating")
        );

        let mut unready = serde_json::to_value(&pods).unwrap();
        unready["items"][0]["status"]["conditions"][0]["status"] = json!("False");
        let unready: PodList = serde_json::from_value(unready).unwrap();
        assert!(
            current_deployment_pods(&deployment, &replica_sets, &unready, &workload)
                .unwrap_err()
                .to_string()
                .contains("not Ready")
        );

        let mut stale = serde_json::to_value(&pods).unwrap();
        stale["items"][0]["metadata"]["ownerReferences"][0]["uid"] = json!("previous-replica-set");
        let stale: PodList = serde_json::from_value(stale).unwrap();
        assert!(
            current_deployment_pods(&deployment, &replica_sets, &stale, &workload)
                .unwrap_err()
                .to_string()
                .contains("current ReplicaSet")
        );
    }

    #[test]
    fn verifier_rejects_ambiguous_rollouts_and_duplicate_pods() {
        let (deployment, replica_sets, pods, workload) = workload_inventory();
        let mut ambiguous = serde_json::to_value(&replica_sets).unwrap();
        let second = ambiguous["items"][0].clone();
        ambiguous["items"].as_array_mut().unwrap().push(second);
        let ambiguous: ReplicaSetList = serde_json::from_value(ambiguous).unwrap();
        assert!(
            current_deployment_pods(&deployment, &ambiguous, &pods, &workload)
                .unwrap_err()
                .to_string()
                .contains("found 2")
        );

        let mut duplicate = serde_json::to_value(&pods).unwrap();
        let mut second = duplicate["items"][0].clone();
        second["metadata"]["name"] = json!("example-runtime-pod-2");
        duplicate["items"].as_array_mut().unwrap().push(second);
        let duplicate: PodList = serde_json::from_value(duplicate).unwrap();
        assert!(
            current_deployment_pods(&deployment, &replica_sets, &duplicate, &workload)
                .unwrap_err()
                .to_string()
                .contains("found 2")
        );
    }
}
