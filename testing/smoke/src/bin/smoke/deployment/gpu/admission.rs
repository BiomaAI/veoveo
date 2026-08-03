use std::collections::BTreeSet;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

use super::{MANAGED_NODE_LABEL, MANAGED_NODE_LABEL_POINTER, MANAGED_NODE_LABEL_VALUE};
use crate::deployment::output_checked;

pub(super) fn validate_kubelet_daemon_set_contract(kubelet: &Value) -> Result<()> {
    let expected_selector = serde_json::json!({
        MANAGED_NODE_LABEL: MANAGED_NODE_LABEL_VALUE
    });
    let selector = kubelet
        .pointer("/spec/template/spec/nodeSelector")
        .context("NVIDIA DRA kubelet-plugin has no nodeSelector")?;
    ensure!(
        selector == &expected_selector,
        "NVIDIA DRA kubelet-plugin nodeSelector is {selector}, expected the sole platform-managed selector {expected_selector}"
    );
    let required_affinity = kubelet.pointer(
        "/spec/template/spec/affinity/nodeAffinity/requiredDuringSchedulingIgnoredDuringExecution",
    );
    ensure!(
        required_affinity.is_none_or(Value::is_null),
        "NVIDIA DRA kubelet-plugin retains an unmanaged required node-admission predicate at spec.template.spec.affinity.nodeAffinity.requiredDuringSchedulingIgnoredDuringExecution: {}",
        required_affinity.expect("nonempty required affinity was checked")
    );
    Ok(())
}

pub(super) fn validate_kubelet_daemon_set_readiness(
    context: &str,
    kubelet: &Value,
    pods: &Value,
    nodes: &[String],
) -> Result<()> {
    let expected = u64::try_from(nodes.len()).expect("node count fits u64");
    let desired = daemon_set_count(kubelet, "desiredNumberScheduled");
    let current = daemon_set_count(kubelet, "currentNumberScheduled");
    let ready = daemon_set_count(kubelet, "numberReady");
    let available = daemon_set_count(kubelet, "numberAvailable");
    if desired == expected && current == expected && ready == expected && available == expected {
        return Ok(());
    }

    let admission = selected_node_admission_diagnostics(context, kubelet, nodes)?;
    let pods = kubelet_pod_diagnostics(pods)?;
    bail!(
        "NVIDIA DRA kubelet-plugin admission is incomplete: desired={desired} current={current} ready={ready} available={available} expected={expected}; node admission: {admission}; pod state: {pods}"
    )
}

fn daemon_set_count(kubelet: &Value, field: &str) -> u64 {
    kubelet
        .pointer(&format!("/status/{field}"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn selected_node_admission_diagnostics(
    context: &str,
    kubelet: &Value,
    selected_nodes: &[String],
) -> Result<String> {
    let output = output_checked(
        "kubectl",
        ["--context", context, "get", "nodes", "-o", "json"],
        None,
    )?;
    let inventory: Value =
        serde_json::from_slice(&output).context("decoding node admission inventory")?;
    let nodes = inventory["items"]
        .as_array()
        .context("node admission inventory has no items")?;
    let selected = selected_nodes
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let tolerations = kubelet
        .pointer("/spec/template/spec/tolerations")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut reports = Vec::new();
    let mut reported_nodes = BTreeSet::new();
    for node in nodes {
        let Some(name) = node.pointer("/metadata/name").and_then(Value::as_str) else {
            continue;
        };
        if !selected.contains(name) {
            continue;
        }
        reported_nodes.insert(name);
        let managed_label = node
            .pointer(MANAGED_NODE_LABEL_POINTER)
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        let ready = node
            .pointer("/status/conditions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|condition| {
                condition.get("type").and_then(Value::as_str) == Some("Ready")
                    && condition.get("status").and_then(Value::as_str) == Some("True")
            });
        let unschedulable = node
            .pointer("/spec/unschedulable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let untolerated = node
            .pointer("/spec/taints")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|taint| {
                matches!(
                    taint.get("effect").and_then(Value::as_str),
                    Some("NoSchedule" | "NoExecute")
                ) && !tolerations
                    .iter()
                    .any(|toleration| toleration_matches(taint, toleration))
            })
            .map(format_taint)
            .collect::<Vec<_>>();
        reports.push(format!(
            "node={name} {MANAGED_NODE_LABEL}={managed_label} ready={ready} unschedulable={unschedulable} untoleratedTaints={untolerated:?}"
        ));
    }
    for missing in selected_nodes {
        if !reported_nodes.contains(missing.as_str()) {
            reports.push(format!("node={missing} missingFromInventory=true"));
        }
    }
    Ok(reports.join("; "))
}

fn toleration_matches(taint: &Value, toleration: &Value) -> bool {
    let taint_key = taint.get("key").and_then(Value::as_str).unwrap_or("");
    let taint_value = taint.get("value").and_then(Value::as_str).unwrap_or("");
    let taint_effect = taint.get("effect").and_then(Value::as_str).unwrap_or("");
    let key = toleration.get("key").and_then(Value::as_str).unwrap_or("");
    let effect = toleration
        .get("effect")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !effect.is_empty() && effect != taint_effect {
        return false;
    }
    match toleration
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or("Equal")
    {
        "Exists" => key.is_empty() || key == taint_key,
        "Equal" => {
            key == taint_key
                && toleration
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    == taint_value
        }
        _ => false,
    }
}

fn format_taint(taint: &Value) -> String {
    let key = taint
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    let value = taint.get("value").and_then(Value::as_str).unwrap_or("");
    let effect = taint
        .get("effect")
        .and_then(Value::as_str)
        .unwrap_or("<missing>");
    format!("{key}={value}:{effect}")
}

fn kubelet_pod_diagnostics(pods: &Value) -> Result<String> {
    let pods = pods["items"]
        .as_array()
        .context("NVIDIA DRA pod inventory has no items")?;
    if pods.is_empty() {
        return Ok("no kubelet-plugin pod was admitted".to_owned());
    }
    let mut reports = Vec::new();
    for pod in pods {
        let name = pod
            .pointer("/metadata/name")
            .and_then(Value::as_str)
            .unwrap_or("<unnamed>");
        let node = pod
            .pointer("/spec/nodeName")
            .and_then(Value::as_str)
            .unwrap_or("<unassigned>");
        let phase = pod
            .pointer("/status/phase")
            .and_then(Value::as_str)
            .unwrap_or("<unknown>");
        let waiting = ["/status/initContainerStatuses", "/status/containerStatuses"]
            .into_iter()
            .flat_map(|pointer| {
                pod.pointer(pointer)
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .filter_map(|status| {
                let reason = status.pointer("/state/waiting/reason")?.as_str()?;
                let container = status.get("name")?.as_str()?;
                Some(format!("{container}:{reason}"))
            })
            .collect::<Vec<_>>();
        reports.push(format!(
            "pod={name} node={node} phase={phase} waiting={waiting:?}"
        ));
    }
    Ok(reports.join("; "))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        kubelet_pod_diagnostics, toleration_matches, validate_kubelet_daemon_set_contract,
    };

    #[test]
    fn kubelet_daemon_set_rejects_discovery_affinity() {
        let managed = json!({
            "spec": {"template": {"spec": {
                "nodeSelector": {"nvidia.com/dra-kubelet-plugin": "true"}
            }}}
        });
        validate_kubelet_daemon_set_contract(&managed).unwrap();

        let mut unmanaged = managed;
        unmanaged["spec"]["template"]["spec"]["affinity"] = json!({
            "nodeAffinity": {"requiredDuringSchedulingIgnoredDuringExecution": {
                "nodeSelectorTerms": [{"matchExpressions": [{
                    "key": "feature.node.kubernetes.io/pci-10de.present",
                    "operator": "In",
                    "values": ["true"]
                }]}]
            }}
        });
        let error = validate_kubelet_daemon_set_contract(&unmanaged)
            .unwrap_err()
            .to_string();
        assert!(error.contains("required node-admission predicate"));
        assert!(error.contains("pci-10de"));
    }

    #[test]
    fn node_admission_diagnostics_identify_untolerated_taints() {
        let taint = json!({"key": "dedicated", "value": "gpu", "effect": "NoSchedule"});
        assert!(!toleration_matches(
            &taint,
            &json!({"key": "nvidia.com/gpu", "operator": "Exists", "effect": "NoSchedule"})
        ));
        assert!(toleration_matches(
            &taint,
            &json!({"key": "dedicated", "operator": "Equal", "value": "gpu", "effect": "NoSchedule"})
        ));
    }

    #[test]
    fn kubelet_pod_diagnostics_preserve_waiting_reason() {
        let pods = json!({"items": [{
            "metadata": {"name": "gpu-driver"},
            "spec": {"nodeName": "gpu-node"},
            "status": {
                "phase": "Pending",
                "containerStatuses": [{
                    "name": "gpus",
                    "state": {"waiting": {"reason": "ImagePullBackOff"}}
                }]
            }
        }]});
        let diagnostic = kubelet_pod_diagnostics(&pods).unwrap();
        assert!(diagnostic.contains("gpu-driver"));
        assert!(diagnostic.contains("ImagePullBackOff"));
    }
}
