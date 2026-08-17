//! Pure extraction and evaluation of rendered Kubernetes Secret references.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KubernetesObjectKey {
    pub group: String,
    pub version: String,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretObjectKey {
    pub namespace: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretReferenceKind {
    EnvironmentKey,
    EnvironmentFrom,
    SecretVolume,
    ProjectedVolume,
    ImagePull,
    IngressTls,
    GatewayTls,
    GatewayActivation,
    CustomResource,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretReferenceRequirement {
    pub secret: SecretObjectKey,
    pub key: Option<String>,
    pub optional: bool,
    pub kind: SecretReferenceKind,
    pub referring_object: KubernetesObjectKey,
    pub field_path: String,
}

/// One contract-owned custom-resource Secret reference shape.
///
/// `reference_path` is a JSON Pointer-like path whose `*` segments traverse
/// every array entry. The terminal value is an object containing the declared
/// relative name, key, namespace, and optionality fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomSecretReferenceSpec {
    pub group: String,
    pub version: String,
    pub kind: String,
    pub reference_path: String,
    pub name_field: String,
    pub key_field: Option<String>,
    pub namespace_field: Option<String>,
    pub optional_field: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CustomSecretReferenceRegistry {
    specs: BTreeMap<(String, String, String), Vec<CustomSecretReferenceSpec>>,
}

impl CustomSecretReferenceRegistry {
    pub fn register(&mut self, spec: CustomSecretReferenceSpec) {
        self.specs
            .entry((spec.group.clone(), spec.version.clone(), spec.kind.clone()))
            .or_default()
            .push(spec);
    }

    fn specs_for(&self, object: &KubernetesObjectKey) -> Option<&[CustomSecretReferenceSpec]> {
        self.specs
            .get(&(
                object.group.clone(),
                object.version.clone(),
                object.kind.clone(),
            ))
            .map(Vec::as_slice)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SecretClosureError {
    InvalidObject {
        reason: &'static str,
    },
    InvalidReference {
        object: Box<KubernetesObjectKey>,
        field_path: String,
    },
    UnverifiableObject {
        object: Box<KubernetesObjectKey>,
    },
    RenderedSecretForbidden {
        object: Box<KubernetesObjectKey>,
    },
    InvalidDigest {
        field: &'static str,
    },
    DuplicateObservation {
        secret: SecretObjectKey,
    },
    MissingObservation {
        secret: SecretObjectKey,
    },
    UnexpectedObservation {
        secret: SecretObjectKey,
    },
}

impl std::fmt::Display for SecretClosureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidObject { reason } => {
                write!(formatter, "invalid Kubernetes object: {reason}")
            }
            Self::InvalidReference { object, field_path } => write!(
                formatter,
                "invalid Secret reference at {} {}/{} field {field_path}",
                object.kind,
                object.namespace.as_deref().unwrap_or("<cluster>"),
                object.name,
            ),
            Self::UnverifiableObject { object } => write!(
                formatter,
                "Secret closure is unverifiable for {} {}/{}",
                object.kind,
                object.namespace.as_deref().unwrap_or("<cluster>"),
                object.name,
            ),
            Self::RenderedSecretForbidden { object } => write!(
                formatter,
                "rendered Secret {}/{} must remain installation-owned",
                object.namespace.as_deref().unwrap_or("<cluster>"),
                object.name,
            ),
            Self::InvalidDigest { field } => write!(formatter, "{field} digest is empty"),
            Self::DuplicateObservation { secret } => write!(
                formatter,
                "duplicate Secret observation for {}/{}",
                secret.namespace, secret.name,
            ),
            Self::MissingObservation { secret } => write!(
                formatter,
                "missing Secret observation for {}/{}",
                secret.namespace, secret.name,
            ),
            Self::UnexpectedObservation { secret } => write!(
                formatter,
                "unexpected Secret observation for {}/{}",
                secret.namespace, secret.name,
            ),
        }
    }
}

impl std::error::Error for SecretClosureError {}

pub fn collect_secret_requirements(
    objects: &[Value],
    default_namespace: &str,
    custom: &CustomSecretReferenceRegistry,
) -> Result<Vec<SecretReferenceRequirement>, SecretClosureError> {
    if default_namespace.trim().is_empty() {
        return Err(SecretClosureError::InvalidObject {
            reason: "default namespace is empty",
        });
    }
    let mut requirements = Vec::new();
    for value in objects {
        let object = object_key(value, default_namespace)?;
        if object.group.is_empty() && object.version == "v1" && object.kind == "Secret" {
            return Err(SecretClosureError::RenderedSecretForbidden {
                object: Box::new(object),
            });
        }
        if let Some(pod_spec_path) = pod_spec_path(&object) {
            if let Some(pod_spec) = value.pointer(pod_spec_path) {
                collect_pod_spec(pod_spec, pod_spec_path, &object, &mut requirements)?;
            }
        } else if object.group == "networking.k8s.io" && object.kind == "Ingress" {
            collect_ingress(value, &object, &mut requirements)?;
        } else if object.group == "gateway.networking.k8s.io" && object.kind == "Gateway" {
            collect_gateway(value, &object, &mut requirements)?;
        } else if let Some(specs) = custom.specs_for(&object) {
            collect_custom(value, &object, specs, &mut requirements)?;
        } else if is_custom_resource(&object) && contains_secret_bearing_key(value) {
            return Err(SecretClosureError::UnverifiableObject {
                object: Box::new(object),
            });
        }
    }
    requirements.sort();
    requirements.dedup();
    Ok(requirements)
}

fn object_key(
    value: &Value,
    default_namespace: &str,
) -> Result<KubernetesObjectKey, SecretClosureError> {
    let api_version = value
        .get("apiVersion")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(SecretClosureError::InvalidObject {
            reason: "apiVersion is missing",
        })?;
    let (group, version) = api_version
        .split_once('/')
        .map_or(("", api_version), |(group, version)| (group, version));
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(SecretClosureError::InvalidObject {
            reason: "kind is missing",
        })?;
    let metadata = value.get("metadata").and_then(Value::as_object).ok_or(
        SecretClosureError::InvalidObject {
            reason: "metadata is missing",
        },
    )?;
    let name = metadata
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(SecretClosureError::InvalidObject {
            reason: "metadata.name is missing",
        })?;
    let namespace = (!is_cluster_scoped(group, kind)).then(|| {
        metadata
            .get("namespace")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(default_namespace)
            .to_owned()
    });
    Ok(KubernetesObjectKey {
        group: group.to_owned(),
        version: version.to_owned(),
        kind: kind.to_owned(),
        namespace,
        name: name.to_owned(),
    })
}

fn is_cluster_scoped(group: &str, kind: &str) -> bool {
    (group.is_empty() && matches!(kind, "Namespace" | "Node" | "PersistentVolume"))
        || (group == "rbac.authorization.k8s.io"
            && matches!(kind, "ClusterRole" | "ClusterRoleBinding"))
        || (group == "apiextensions.k8s.io" && kind == "CustomResourceDefinition")
        || (group == "storage.k8s.io" && matches!(kind, "StorageClass" | "CSIDriver"))
        || (group == "resource.k8s.io" && kind == "DeviceClass")
}

fn pod_spec_path(object: &KubernetesObjectKey) -> Option<&'static str> {
    match (object.group.as_str(), object.kind.as_str()) {
        ("", "Pod") => Some("/spec"),
        ("apps", "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet") => {
            Some("/spec/template/spec")
        }
        ("batch", "Job") => Some("/spec/template/spec"),
        ("batch", "CronJob") => Some("/spec/jobTemplate/spec/template/spec"),
        _ => None,
    }
}

fn collect_pod_spec(
    spec: &Value,
    path: &str,
    object: &KubernetesObjectKey,
    output: &mut Vec<SecretReferenceRequirement>,
) -> Result<(), SecretClosureError> {
    for (container_field, containers) in [
        ("containers", spec.get("containers")),
        ("initContainers", spec.get("initContainers")),
    ] {
        for (container_index, container) in array_items(containers).enumerate() {
            for (env_index, env) in array_items(container.get("env")).enumerate() {
                if let Some(reference) = env.pointer("/valueFrom/secretKeyRef") {
                    push_reference(
                        object,
                        reference,
                        format!(
                            "{path}/{container_field}/{container_index}/env/{env_index}/valueFrom/secretKeyRef"
                        ),
                        "name",
                        Some("key"),
                        None,
                        Some("optional"),
                        SecretReferenceKind::EnvironmentKey,
                        output,
                    )?;
                }
            }
            for (env_index, env_from) in array_items(container.get("envFrom")).enumerate() {
                if let Some(reference) = env_from.get("secretRef") {
                    push_reference(
                        object,
                        reference,
                        format!(
                            "{path}/{container_field}/{container_index}/envFrom/{env_index}/secretRef"
                        ),
                        "name",
                        None,
                        None,
                        Some("optional"),
                        SecretReferenceKind::EnvironmentFrom,
                        output,
                    )?;
                }
            }
        }
    }
    for (index, reference) in array_items(spec.get("imagePullSecrets")).enumerate() {
        push_reference(
            object,
            reference,
            format!("{path}/imagePullSecrets/{index}"),
            "name",
            None,
            None,
            None,
            SecretReferenceKind::ImagePull,
            output,
        )?;
    }
    for (volume_index, volume) in array_items(spec.get("volumes")).enumerate() {
        if let Some(reference) = volume.get("secret") {
            let base = format!("{path}/volumes/{volume_index}/secret");
            if let Some(items) = reference.get("items").and_then(Value::as_array) {
                for (item_index, item) in items.iter().enumerate() {
                    let key = item.get("key").and_then(Value::as_str).ok_or_else(|| {
                        SecretClosureError::InvalidReference {
                            object: Box::new(object.clone()),
                            field_path: format!("{base}/items/{item_index}/key"),
                        }
                    })?;
                    push_named_reference(
                        object,
                        reference,
                        format!("{base}/items/{item_index}"),
                        "secretName",
                        Some(key.to_owned()),
                        None,
                        Some("optional"),
                        SecretReferenceKind::SecretVolume,
                        output,
                    )?;
                }
            } else {
                push_reference(
                    object,
                    reference,
                    base,
                    "secretName",
                    None,
                    None,
                    Some("optional"),
                    SecretReferenceKind::SecretVolume,
                    output,
                )?;
            }
        }
        for (source_index, source) in array_items(volume.pointer("/projected/sources")).enumerate()
        {
            let Some(reference) = source.get("secret") else {
                continue;
            };
            let base =
                format!("{path}/volumes/{volume_index}/projected/sources/{source_index}/secret");
            let items = reference.get("items").and_then(Value::as_array);
            if let Some(items) = items {
                for (item_index, item) in items.iter().enumerate() {
                    let key = item.get("key").and_then(Value::as_str).ok_or_else(|| {
                        SecretClosureError::InvalidReference {
                            object: Box::new(object.clone()),
                            field_path: format!("{base}/items/{item_index}/key"),
                        }
                    })?;
                    push_named_reference(
                        object,
                        reference,
                        format!("{base}/items/{item_index}"),
                        "name",
                        Some(key.to_owned()),
                        None,
                        Some("optional"),
                        SecretReferenceKind::ProjectedVolume,
                        output,
                    )?;
                }
            } else {
                push_reference(
                    object,
                    reference,
                    base,
                    "name",
                    None,
                    None,
                    Some("optional"),
                    SecretReferenceKind::ProjectedVolume,
                    output,
                )?;
            }
        }
    }
    Ok(())
}

fn collect_ingress(
    value: &Value,
    object: &KubernetesObjectKey,
    output: &mut Vec<SecretReferenceRequirement>,
) -> Result<(), SecretClosureError> {
    for (index, tls) in array_items(value.pointer("/spec/tls")).enumerate() {
        if tls.get("secretName").is_some() {
            push_reference(
                object,
                tls,
                format!("/spec/tls/{index}/secretName"),
                "secretName",
                None,
                None,
                None,
                SecretReferenceKind::IngressTls,
                output,
            )?;
        }
    }
    Ok(())
}

fn collect_gateway(
    value: &Value,
    object: &KubernetesObjectKey,
    output: &mut Vec<SecretReferenceRequirement>,
) -> Result<(), SecretClosureError> {
    for (listener_index, listener) in array_items(value.pointer("/spec/listeners")).enumerate() {
        for (reference_index, reference) in
            array_items(listener.pointer("/tls/certificateRefs")).enumerate()
        {
            let group = reference.get("group").and_then(Value::as_str).unwrap_or("");
            let kind = reference
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("Secret");
            if !group.is_empty() || kind != "Secret" {
                continue;
            }
            push_reference(
                object,
                reference,
                format!("/spec/listeners/{listener_index}/tls/certificateRefs/{reference_index}"),
                "name",
                None,
                Some("namespace"),
                None,
                SecretReferenceKind::GatewayTls,
                output,
            )?;
        }
    }
    Ok(())
}

fn collect_custom(
    value: &Value,
    object: &KubernetesObjectKey,
    specs: &[CustomSecretReferenceSpec],
    output: &mut Vec<SecretReferenceRequirement>,
) -> Result<(), SecretClosureError> {
    for spec in specs {
        let mut matches = Vec::new();
        collect_path_matches(value, &spec.reference_path, &mut matches);
        for (path, reference) in matches {
            push_reference(
                object,
                reference,
                path,
                &spec.name_field,
                spec.key_field.as_deref(),
                spec.namespace_field.as_deref(),
                spec.optional_field.as_deref(),
                SecretReferenceKind::CustomResource,
                output,
            )?;
        }
    }
    Ok(())
}

fn collect_path_matches<'a>(value: &'a Value, path: &str, output: &mut Vec<(String, &'a Value)>) {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    collect_path_segments(value, &segments, String::new(), output);
}

fn collect_path_segments<'a>(
    value: &'a Value,
    segments: &[&str],
    path: String,
    output: &mut Vec<(String, &'a Value)>,
) {
    let Some((segment, tail)) = segments.split_first() else {
        output.push((path, value));
        return;
    };
    if *segment == "*" {
        for (index, child) in array_items(Some(value)).enumerate() {
            collect_path_segments(child, tail, format!("{path}/{index}"), output);
        }
    } else if let Some(child) = value.get(*segment) {
        collect_path_segments(child, tail, format!("{path}/{segment}"), output);
    }
}

#[allow(clippy::too_many_arguments)]
fn push_reference(
    object: &KubernetesObjectKey,
    reference: &Value,
    field_path: String,
    name_field: &str,
    key_field: Option<&str>,
    namespace_field: Option<&str>,
    optional_field: Option<&str>,
    kind: SecretReferenceKind,
    output: &mut Vec<SecretReferenceRequirement>,
) -> Result<(), SecretClosureError> {
    let key = key_field
        .and_then(|field| reference.get(field))
        .and_then(Value::as_str)
        .map(str::to_owned);
    push_named_reference(
        object,
        reference,
        field_path,
        name_field,
        key,
        namespace_field,
        optional_field,
        kind,
        output,
    )
}

#[allow(clippy::too_many_arguments)]
fn push_named_reference(
    object: &KubernetesObjectKey,
    reference: &Value,
    field_path: String,
    name_field: &str,
    key: Option<String>,
    namespace_field: Option<&str>,
    optional_field: Option<&str>,
    kind: SecretReferenceKind,
    output: &mut Vec<SecretReferenceRequirement>,
) -> Result<(), SecretClosureError> {
    let name = reference
        .get(name_field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SecretClosureError::InvalidReference {
            object: Box::new(object.clone()),
            field_path: field_path.clone(),
        })?;
    if key.as_ref().is_some_and(String::is_empty) {
        return Err(SecretClosureError::InvalidReference {
            object: Box::new(object.clone()),
            field_path,
        });
    }
    let namespace = namespace_field
        .and_then(|field| reference.get(field))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| object.namespace.clone())
        .ok_or_else(|| SecretClosureError::InvalidReference {
            object: Box::new(object.clone()),
            field_path: field_path.clone(),
        })?;
    let optional = optional_field
        .and_then(|field| reference.get(field))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    output.push(SecretReferenceRequirement {
        secret: SecretObjectKey {
            namespace,
            name: name.to_owned(),
        },
        key,
        optional,
        kind,
        referring_object: object.clone(),
        field_path,
    });
    Ok(())
}

fn array_items(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_array).into_iter().flatten()
}

fn is_custom_resource(object: &KubernetesObjectKey) -> bool {
    !matches!(
        object.group.as_str(),
        "" | "apps"
            | "batch"
            | "networking.k8s.io"
            | "gateway.networking.k8s.io"
            | "policy"
            | "rbac.authorization.k8s.io"
            | "apiextensions.k8s.io"
            | "storage.k8s.io"
            | "resource.k8s.io"
    )
}

fn contains_secret_bearing_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            let normalized = key.to_ascii_lowercase();
            normalized.contains("secret") || contains_secret_bearing_key(value)
        }),
        Value::Array(values) => values.iter().any(contains_secret_bearing_key),
        _ => false,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum SecretObservationStatus {
    Present { keys: BTreeSet<String> },
    Missing,
    Forbidden,
    Timeout,
    Malformed,
    Transport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretObservation {
    pub secret: SecretObjectKey,
    pub status: SecretObservationStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretClosureStatus {
    Satisfied,
    MissingSecret,
    MissingKey,
    Forbidden,
    Timeout,
    Malformed,
    Transport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretPresenceResult {
    pub secret: SecretObjectKey,
    pub status: SecretObservationStatus,
    pub required_keys: BTreeSet<String>,
    pub missing_keys: BTreeSet<String>,
    pub optional_only: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretClosure {
    pub profile_digest: String,
    pub lock_digest: String,
    pub requirements: Vec<SecretReferenceRequirement>,
    pub presence: Vec<SecretPresenceResult>,
    pub status: SecretClosureStatus,
}

impl SecretClosure {
    pub fn evaluate(
        profile_digest: impl Into<String>,
        lock_digest: impl Into<String>,
        mut requirements: Vec<SecretReferenceRequirement>,
        observations: Vec<SecretObservation>,
    ) -> Result<Self, SecretClosureError> {
        let profile_digest = profile_digest.into();
        let lock_digest = lock_digest.into();
        if profile_digest.trim().is_empty() {
            return Err(SecretClosureError::InvalidDigest { field: "profile" });
        }
        if lock_digest.trim().is_empty() {
            return Err(SecretClosureError::InvalidDigest { field: "lock" });
        }
        requirements.sort();
        requirements.dedup();
        let expected = requirements
            .iter()
            .map(|requirement| requirement.secret.clone())
            .collect::<BTreeSet<_>>();
        let mut observed = BTreeMap::new();
        for observation in observations {
            if !expected.contains(&observation.secret) {
                return Err(SecretClosureError::UnexpectedObservation {
                    secret: observation.secret,
                });
            }
            let secret = observation.secret.clone();
            if observed
                .insert(secret.clone(), observation.status)
                .is_some()
            {
                return Err(SecretClosureError::DuplicateObservation { secret });
            }
        }
        let mut presence = Vec::with_capacity(expected.len());
        let mut statuses = BTreeSet::new();
        for secret in expected {
            let status =
                observed
                    .remove(&secret)
                    .ok_or_else(|| SecretClosureError::MissingObservation {
                        secret: secret.clone(),
                    })?;
            let matching = requirements
                .iter()
                .filter(|requirement| requirement.secret == secret)
                .collect::<Vec<_>>();
            let required_keys = matching
                .iter()
                .filter(|requirement| !requirement.optional)
                .filter_map(|requirement| requirement.key.clone())
                .collect::<BTreeSet<_>>();
            let optional_only = matching.iter().all(|requirement| requirement.optional);
            let missing_keys = match &status {
                SecretObservationStatus::Present { keys } => {
                    required_keys.difference(keys).cloned().collect()
                }
                _ => BTreeSet::new(),
            };
            let terminal = match &status {
                SecretObservationStatus::Present { .. } if !missing_keys.is_empty() => {
                    Some(SecretClosureStatus::MissingKey)
                }
                SecretObservationStatus::Present { .. } => None,
                SecretObservationStatus::Missing if optional_only => None,
                SecretObservationStatus::Missing => Some(SecretClosureStatus::MissingSecret),
                SecretObservationStatus::Forbidden => Some(SecretClosureStatus::Forbidden),
                SecretObservationStatus::Timeout => Some(SecretClosureStatus::Timeout),
                SecretObservationStatus::Malformed => Some(SecretClosureStatus::Malformed),
                SecretObservationStatus::Transport => Some(SecretClosureStatus::Transport),
            };
            if let Some(terminal) = terminal {
                statuses.insert(status_rank(terminal));
            }
            presence.push(SecretPresenceResult {
                secret,
                status,
                required_keys,
                missing_keys,
                optional_only,
            });
        }
        let status = statuses
            .iter()
            .next_back()
            .copied()
            .map(status_from_rank)
            .unwrap_or(SecretClosureStatus::Satisfied);
        Ok(Self {
            profile_digest,
            lock_digest,
            requirements,
            presence,
            status,
        })
    }
}

const fn status_rank(status: SecretClosureStatus) -> u8 {
    match status {
        SecretClosureStatus::Satisfied => 0,
        SecretClosureStatus::MissingKey => 1,
        SecretClosureStatus::MissingSecret => 2,
        SecretClosureStatus::Malformed => 3,
        SecretClosureStatus::Forbidden => 4,
        SecretClosureStatus::Timeout => 5,
        SecretClosureStatus::Transport => 6,
    }
}

const fn status_from_rank(rank: u8) -> SecretClosureStatus {
    match rank {
        1 => SecretClosureStatus::MissingKey,
        2 => SecretClosureStatus::MissingSecret,
        3 => SecretClosureStatus::Malformed,
        4 => SecretClosureStatus::Forbidden,
        5 => SecretClosureStatus::Timeout,
        6 => SecretClosureStatus::Transport,
        _ => SecretClosureStatus::Satisfied,
    }
}
