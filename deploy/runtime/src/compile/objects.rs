use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::components::{ObjectIdentity, RenderedObject};
use veoveo_extension_contract::ArtifactDigest;

/// Offline scope declaration. Built-in API kinds have fixed scope; custom kinds
/// require a rendered CRD, an explicit namespace, or an exact cluster permission.
/// The installer must check these declarations against destination API discovery
/// before its first write. Publication never contacts the destination cluster.
#[derive(Default)]
pub(crate) struct ObjectScopes {
    declared: BTreeMap<(String, String), bool>,
}

impl ObjectScopes {
    pub(crate) fn declare_crds(&mut self, objects: &[Value]) -> Result<()> {
        for object in objects {
            if object.get("apiVersion").and_then(Value::as_str) != Some("apiextensions.k8s.io/v1")
                || object.get("kind").and_then(Value::as_str) != Some("CustomResourceDefinition")
            {
                continue;
            }
            let group = field(object, "/spec/group")?;
            ensure!(!group.is_empty(), "CRD must declare a non-core API group");
            let kind = field(object, "/spec/names/kind")?;
            let namespaced = match field(object, "/spec/scope")? {
                "Namespaced" => true,
                "Cluster" => false,
                _ => anyhow::bail!("CRD declares an invalid scope"),
            };
            ensure!(
                builtin_scope(group, kind).is_none(),
                "CRD cannot redeclare a built-in API kind"
            );
            let versions = object
                .pointer("/spec/versions")
                .and_then(Value::as_array)
                .context("CRD omits versions")?;
            ensure!(
                versions
                    .iter()
                    .any(|v| v.get("served").and_then(Value::as_bool) == Some(true)),
                "CRD has no served version"
            );
            let key = (group.to_owned(), kind.to_owned());
            if let Some(previous) = self.declared.insert(key, namespaced) {
                ensure!(
                    previous == namespaced,
                    "CRD declarations disagree about resource scope"
                );
            }
        }
        Ok(())
    }

    pub(crate) fn rendered(
        &self,
        objects: &[Value],
        namespace: &str,
        cluster_objects: &BTreeSet<ObjectIdentity>,
    ) -> Result<Vec<RenderedObject>> {
        objects.iter().map(|object| {
            let version = field(object, "/apiVersion")?;
            let group = api_group(version)?;
            let kind = field(object, "/kind")?;
            ensure!(kind != "List", "Kubernetes lists must be expanded before compilation");
            ensure!(kind != "Secret" || !group.is_empty(), "deployment components cannot own Secrets");
            let mut identity = ObjectIdentity {
                group: group.to_owned(),
                kind: kind.to_owned(),
                name: field(object, "/metadata/name")?.to_owned(),
                namespace: None,
            };
            let explicit_namespace = match object.pointer("/metadata/namespace") {
                None => None,
                Some(Value::String(value)) if !value.is_empty() => Some(value.as_str()),
                _ => anyhow::bail!("object namespace must be a nonempty string when present"),
            };
            let declared = builtin_scope(group, kind)
                .or_else(|| self.declared.get(&(group.to_owned(), kind.to_owned())).copied());
            let namespaced = match declared {
                Some(scope) => scope,
                None if cluster_objects.contains(&identity) => false,
                None if explicit_namespace.is_some() => true,
                None => anyhow::bail!("custom resource {group}/{kind} requires a CRD, explicit namespace, or exact cluster permission"),
            };
            ensure!(namespaced || explicit_namespace.is_none(), "cluster-scoped object declares a namespace");
            identity.namespace = namespaced.then(|| explicit_namespace.unwrap_or(namespace).to_owned());
            // Make the namespace used for identity also part of the exact object
            // submitted by the executor. Implicit and explicit defaults hash alike.
            let mut normalized = object.clone();
            if let Some(namespace) = &identity.namespace {
                normalized["metadata"]["namespace"] = Value::String(namespace.clone());
            }
            Ok(RenderedObject { identity, digest: object_digest(&normalized)? })
        }).collect()
    }
}

fn api_group(version: &str) -> Result<&str> {
    let parts = version.split('/').collect::<Vec<_>>();
    ensure!(
        (1..=2).contains(&parts.len())
            && parts.iter().all(|part| !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))),
        "invalid Kubernetes API version"
    );
    Ok(if parts.len() == 1 { "" } else { parts[0] })
}

/// Scope is fixed by group/kind, independently of served API versions. Unknown
/// kinds are deliberately not inferred from name suffixes or capitalization.
fn builtin_scope(group: &str, kind: &str) -> Option<bool> {
    match (group, kind) {
        ("", "Namespace" | "Node" | "PersistentVolume")
        | ("rbac.authorization.k8s.io", "ClusterRole" | "ClusterRoleBinding")
        | ("apiextensions.k8s.io", "CustomResourceDefinition")
        | ("apiregistration.k8s.io", "APIService")
        | ("node.k8s.io", "RuntimeClass")
        | ("scheduling.k8s.io", "PriorityClass")
        | (
            "storage.k8s.io",
            "StorageClass" | "CSIDriver" | "CSINode" | "VolumeAttachment" | "VolumeAttributesClass",
        )
        | ("resource.k8s.io", "DeviceClass" | "ResourceSlice" | "DeviceTaintRule")
        | (
            "admissionregistration.k8s.io",
            "MutatingWebhookConfiguration"
            | "ValidatingWebhookConfiguration"
            | "ValidatingAdmissionPolicy"
            | "ValidatingAdmissionPolicyBinding"
            | "MutatingAdmissionPolicy"
            | "MutatingAdmissionPolicyBinding",
        )
        | ("flowcontrol.apiserver.k8s.io", "FlowSchema" | "PriorityLevelConfiguration")
        | ("certificates.k8s.io", "CertificateSigningRequest" | "ClusterTrustBundle") => {
            Some(false)
        }
        (
            "",
            "ConfigMap"
            | "Secret"
            | "Service"
            | "ServiceAccount"
            | "Pod"
            | "PodTemplate"
            | "ReplicationController"
            | "PersistentVolumeClaim"
            | "Endpoints"
            | "Event"
            | "LimitRange"
            | "ResourceQuota",
        )
        | (
            "apps",
            "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "ControllerRevision",
        )
        | ("batch", "Job" | "CronJob")
        | ("rbac.authorization.k8s.io", "Role" | "RoleBinding")
        | ("networking.k8s.io", "Ingress" | "NetworkPolicy")
        | ("discovery.k8s.io", "EndpointSlice")
        | ("policy", "PodDisruptionBudget")
        | ("autoscaling", "HorizontalPodAutoscaler")
        | ("coordination.k8s.io", "Lease" | "LeaseCandidate")
        | ("events.k8s.io", "Event")
        | ("storage.k8s.io", "CSIStorageCapacity")
        | ("resource.k8s.io", "ResourceClaim" | "ResourceClaimTemplate") => Some(true),
        ("networking.k8s.io", "IngressClass" | "IPAddress" | "ServiceCIDR") => Some(false),
        _ => None,
    }
}

pub(super) fn bytes_digest(bytes: &[u8]) -> Result<ArtifactDigest> {
    let hex = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(ArtifactDigest::new(format!("sha256:{hex}"))?)
}

fn object_digest(object: &Value) -> Result<ArtifactDigest> {
    fn sort(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.sort_keys();
                for child in map.values_mut() {
                    sort(child);
                }
            }
            Value::Array(array) => {
                for child in array {
                    sort(child);
                }
            }
            _ => {}
        }
    }
    let mut object = object.clone();
    sort(&mut object);
    bytes_digest(&serde_json::to_vec(&object)?)
}

pub(super) fn container_images(objects: &[Value]) -> Result<BTreeSet<String>> {
    let mut images = BTreeSet::new();
    for object in objects {
        let pointer = match field(object, "/kind")? {
            "Pod" => "/spec",
            "Deployment"
            | "DaemonSet"
            | "StatefulSet"
            | "ReplicaSet"
            | "Job"
            | "ReplicationController" => "/spec/template/spec",
            "CronJob" => "/spec/jobTemplate/spec/template/spec",
            "PodTemplate" => "/template/spec",
            _ => continue,
        };
        let pod = object
            .pointer(pointer)
            .context("rendered workload omits its Pod specification")?;
        for field in ["containers", "initContainers", "ephemeralContainers"] {
            if let Some(containers) = pod.get(field) {
                for container in containers
                    .as_array()
                    .context("Pod containers must be an array")?
                {
                    let image = container
                        .get("image")
                        .and_then(Value::as_str)
                        .context("container omits its image")?;
                    ensure!(
                        !image.is_empty() && !image.chars().any(char::is_whitespace),
                        "invalid container image reference"
                    );
                    images.insert(image.to_owned());
                }
            }
        }
    }
    Ok(images)
}

fn field<'a>(object: &'a Value, pointer: &str) -> Result<&'a str> {
    object
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("rendered object omits {pointer}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config() -> Value {
        json!({"apiVersion":"v1", "kind":"ConfigMap", "metadata":{"name":"public-config"}, "data":{"setting":"one"}})
    }

    fn crd(scope: &str) -> Value {
        json!({"apiVersion":"apiextensions.k8s.io/v1", "kind":"CustomResourceDefinition",
            "metadata":{"name":"scenes.example.invalid"}, "spec":{"group":"example.invalid",
                "names":{"kind":"Scene", "plural":"scenes"}, "scope":scope,
                "versions":[{"name":"v1", "served":true, "storage":true}]}})
    }

    #[test]
    fn namespace_normalization_binds_the_actual_target_without_a_cluster() {
        let scopes = ObjectScopes::default();
        let mut implicit = config();
        let first = scopes
            .rendered(&[implicit.clone()], "veoveo", &BTreeSet::new())
            .unwrap();
        implicit["metadata"]["namespace"] = json!("veoveo");
        let explicit = scopes
            .rendered(&[implicit.clone()], "other-default", &BTreeSet::new())
            .unwrap();
        assert_eq!(first, explicit);
        implicit["metadata"]["namespace"] = json!("other");
        let other = scopes
            .rendered(&[implicit], "veoveo", &BTreeSet::new())
            .unwrap();
        assert_ne!(first[0].identity, other[0].identity);
        assert_ne!(first[0].digest, other[0].digest);
    }

    #[test]
    fn custom_scope_requires_explicit_evidence_and_cannot_change_with_catalog_order() {
        let mut scopes = ObjectScopes::default();
        let scene = json!({"apiVersion":"example.invalid/v1", "kind":"Scene", "metadata":{"name":"sample"}});
        assert!(
            scopes
                .rendered(std::slice::from_ref(&scene), "veoveo", &BTreeSet::new())
                .is_err()
        );
        scopes.declare_crds(&[crd("Namespaced")]).unwrap();
        let objects = scopes
            .rendered(std::slice::from_ref(&scene), "veoveo", &BTreeSet::new())
            .unwrap();
        assert_eq!(objects[0].identity.namespace.as_deref(), Some("veoveo"));
        assert!(scopes.declare_crds(&[crd("Cluster")]).is_err());
        let cluster = ObjectIdentity {
            group: "example.invalid".into(),
            kind: "Scene".into(),
            namespace: None,
            name: "sample".into(),
        };
        let permissions = BTreeSet::from([cluster.clone()]);
        let objects = ObjectScopes::default()
            .rendered(&[scene], "veoveo", &permissions)
            .unwrap();
        assert_eq!(objects[0].identity, cluster);
    }

    #[test]
    fn custom_names_cannot_override_builtin_scope() {
        let mut declaration = crd("Cluster");
        declaration["spec"]["group"] = json!("apps");
        declaration["spec"]["names"]["kind"] = json!("Deployment");
        assert!(
            ObjectScopes::default()
                .declare_crds(&[declaration])
                .unwrap_err()
                .to_string()
                .contains("built-in")
        );
        let namespaced_node = json!({"apiVersion":"v1", "kind":"Node", "metadata":{"name":"worker", "namespace":"veoveo"}});
        assert!(
            ObjectScopes::default()
                .rendered(&[namespaced_node], "veoveo", &BTreeSet::new())
                .unwrap_err()
                .to_string()
                .contains("cluster-scoped")
        );
    }

    #[test]
    fn secret_lists_and_malformed_identity_are_rejected_before_hashing() {
        let scopes = ObjectScopes::default();
        for object in [
            json!({"apiVersion":"v1", "kind":"Secret", "metadata":{"name":"credentials"}}),
            json!({"apiVersion":"v1", "kind":"List", "items":[]}),
            json!({"apiVersion":"v1", "kind":"ConfigMap", "metadata":{"name":"config", "namespace":42}}),
            json!({"apiVersion":"v1/../../other", "kind":"ConfigMap", "metadata":{"name":"config"}}),
        ] {
            assert!(
                scopes
                    .rendered(&[object], "veoveo", &BTreeSet::new())
                    .is_err()
            );
        }
    }

    #[test]
    fn init_and_cronjob_images_participate_in_the_rendered_input_closure() {
        let object = json!({"apiVersion":"batch/v1", "kind":"CronJob", "metadata":{"name":"capture"},
        "spec":{"jobTemplate":{"spec":{"template":{"spec":{
            "initContainers":[{"name":"init", "image":"registry/init@sha256:aaaa"}],
            "containers":[{"name":"capture", "image":"registry/capture@sha256:bbbb"}]
        }}}}}});
        assert_eq!(
            container_images(&[object]).unwrap(),
            BTreeSet::from([
                "registry/init@sha256:aaaa".into(),
                "registry/capture@sha256:bbbb".into()
            ])
        );
        let mut before = config();
        let first = object_digest(&before).unwrap();
        before["data"]["setting"] = json!("two");
        assert_ne!(first, object_digest(&before).unwrap());
    }
}
