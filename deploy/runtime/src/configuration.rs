use crate::{charts::helm_render_locked, process::kubectl_apply_value, sources::ResolvedSource};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
};
use veoveo_deploy_contract::{
    ConfigMapSpec, CustomSecretReferenceRegistry, FirstPartyMcpServer, KubernetesObjectKey,
    LoadedProfile, PlatformComponent, SecretClosure, SecretClosureStatus, SecretObjectKey,
    SecretObservation, SecretObservationStatus, SecretReferenceKind, SecretReferenceRequirement,
    collect_secret_requirements, gateway_bundle_digest,
};
use veoveo_mcp_contract::GatewayControlPlane;
const GATEWAY_MOUNT_ROOT: &str = "/etc/veoveo/gateway/";

#[derive(Debug)]
pub(crate) struct PreparedGatewayActivation {
    pub(crate) config_map_name: String,
    pub(crate) revision: String,
    pub(crate) confidential_secret: String,
    pub(crate) required_secret_keys: BTreeSet<String>,
    pub(crate) data: BTreeMap<String, String>,
}

pub(crate) fn apply_config_map(
    profile: &LoadedProfile,
    context: &str,
    config_map: &ConfigMapSpec,
) -> Result<()> {
    let mut data = BTreeMap::new();
    for (key, path) in &config_map.files {
        data.insert(
            key.clone(),
            fs::read_to_string(profile.resolve(path))
                .with_context(|| format!("reading ConfigMap source {}", path.display()))?,
        );
    }
    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {
                "name": config_map.name,
                "namespace": profile.definition.namespace
            },
            "data": data
        }),
    )
}

pub(crate) fn after_secret_closure<T>(
    closure: SecretClosure,
    action: impl FnOnce(&SecretClosure) -> Result<T>,
) -> Result<T> {
    ensure!(
        closure.status == SecretClosureStatus::Satisfied,
        "rendered Secret-reference closure failed: {}",
        serde_json::to_string(&closure)?
    );
    action(&closure)
}

pub(crate) fn prepare_secret_closure(
    profile_path: &Path,
    lock_path: &Path,
    profile: &LoadedProfile,
    sources: &[ResolvedSource],
    components: &BTreeSet<PlatformComponent>,
    mcp_servers: &BTreeSet<FirstPartyMcpServer>,
    gateway_activation: Option<&PreparedGatewayActivation>,
) -> Result<SecretClosure> {
    let mut objects = Vec::new();
    if let Some(cluster) = &profile.definition.kubernetes.local_cluster {
        for manifest in &cluster.node_bootstrap_manifests {
            append_yaml_objects(&profile.resolve(manifest), &mut objects)?;
        }
    }
    for manifest in &profile.definition.resources.manifests {
        append_yaml_objects(&profile.resolve(manifest), &mut objects)?;
    }
    for source in sources {
        for release in &source.definition.releases {
            let rendered = helm_render_locked(profile, source, release, components, mcp_servers)?;
            append_yaml_bytes(
                rendered.as_bytes(),
                &format!("Helm release {}", release.name),
                &mut objects,
            )?;
        }
    }

    let mut requirements = collect_secret_requirements(
        &objects,
        &profile.definition.namespace,
        &CustomSecretReferenceRegistry::default(),
    )?;
    if let Some(activation) = gateway_activation {
        for key in &activation.required_secret_keys {
            requirements.push(SecretReferenceRequirement {
                secret: SecretObjectKey {
                    namespace: profile.definition.namespace.clone(),
                    name: activation.confidential_secret.clone(),
                },
                key: Some(key.clone()),
                optional: false,
                kind: SecretReferenceKind::GatewayActivation,
                referring_object: KubernetesObjectKey {
                    group: String::new(),
                    version: "v1".to_owned(),
                    kind: "ConfigMap".to_owned(),
                    namespace: Some(profile.definition.namespace.clone()),
                    name: activation.config_map_name.clone(),
                },
                field_path: "/gatewayActivation/requiredSecretKeys".to_owned(),
            });
        }
    }
    requirements.sort();
    requirements.dedup();
    let secret_keys = requirements
        .iter()
        .map(|requirement| requirement.secret.clone())
        .collect::<BTreeSet<_>>();
    let observations = secret_keys
        .into_iter()
        .map(|secret| observe_secret(&profile.definition.kubernetes.context, secret))
        .collect::<Vec<_>>();
    let profile_digest = file_digest(profile_path, "deployment profile")?;
    let lock_digest = file_digest(lock_path, "deployment lock")?;
    SecretClosure::evaluate(profile_digest, lock_digest, requirements, observations)
        .map_err(Into::into)
}

pub(crate) fn validate_node_bootstrap_secret_boundary(profile: &LoadedProfile) -> Result<()> {
    let mut objects = Vec::new();
    if let Some(cluster) = &profile.definition.kubernetes.local_cluster {
        for manifest in &cluster.node_bootstrap_manifests {
            append_yaml_objects(&profile.resolve(manifest), &mut objects)?;
        }
    }
    let requirements = collect_secret_requirements(
        &objects,
        &profile.definition.namespace,
        &CustomSecretReferenceRegistry::default(),
    )?;
    ensure!(
        requirements.is_empty(),
        "node bootstrap manifests cannot depend on Secrets before the installation owner can supply them"
    );
    Ok(())
}

pub(crate) fn file_digest(path: &Path, label: &str) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {label} {}", path.display()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

pub(crate) fn append_yaml_objects(path: &Path, output: &mut Vec<Value>) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("reading manifest {}", path.display()))?;
    append_yaml_bytes(&bytes, &format!("manifest {}", path.display()), output)
}

pub(crate) fn append_yaml_bytes(bytes: &[u8], source: &str, output: &mut Vec<Value>) -> Result<()> {
    for document in serde_yaml_ng::Deserializer::from_slice(bytes) {
        let yaml = serde_yaml_ng::Value::deserialize(document)
            .with_context(|| format!("decoding {source}"))?;
        if yaml.is_null() {
            continue;
        }
        let value = serde_json::to_value(yaml).with_context(|| format!("normalizing {source}"))?;
        if value.get("kind").and_then(Value::as_str) == Some("List") {
            let items = value
                .get("items")
                .and_then(Value::as_array)
                .with_context(|| format!("Kubernetes List in {source} has no items"))?;
            output.extend(items.iter().cloned());
        } else {
            output.push(value);
        }
    }
    Ok(())
}

pub(crate) fn observe_secret(context: &str, secret: SecretObjectKey) -> SecretObservation {
    let output = Command::new("kubectl")
        .args([
            "--context",
            context,
            "--namespace",
            secret.namespace.as_str(),
            "get",
            "secret",
            secret.name.as_str(),
            "--ignore-not-found=true",
            "--request-timeout=10s",
            "--output=json",
        ])
        .output();
    match output {
        Ok(output) => decode_secret_observation(
            secret,
            output.status.success(),
            &output.stdout,
            &output.stderr,
        ),
        Err(_) => SecretObservation {
            secret,
            status: SecretObservationStatus::Transport,
        },
    }
}

pub(crate) fn decode_secret_observation(
    secret: SecretObjectKey,
    success: bool,
    stdout: &[u8],
    stderr: &[u8],
) -> SecretObservation {
    let status = if success && stdout.iter().all(u8::is_ascii_whitespace) {
        SecretObservationStatus::Missing
    } else if success {
        decode_present_secret(&secret, stdout).unwrap_or(SecretObservationStatus::Malformed)
    } else {
        let diagnostic = String::from_utf8_lossy(stderr).to_ascii_lowercase();
        if diagnostic.contains("forbidden") {
            SecretObservationStatus::Forbidden
        } else if diagnostic.contains("timed out")
            || diagnostic.contains("timeout")
            || diagnostic.contains("deadline exceeded")
        {
            SecretObservationStatus::Timeout
        } else {
            SecretObservationStatus::Transport
        }
    };
    SecretObservation { secret, status }
}

pub(crate) fn decode_present_secret(
    expected: &SecretObjectKey,
    bytes: &[u8],
) -> Option<SecretObservationStatus> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    if value.get("apiVersion").and_then(Value::as_str) != Some("v1")
        || value.get("kind").and_then(Value::as_str) != Some("Secret")
        || value.pointer("/metadata/name").and_then(Value::as_str) != Some(&expected.name)
        || value.pointer("/metadata/namespace").and_then(Value::as_str) != Some(&expected.namespace)
    {
        return None;
    }
    let keys = value
        .get("data")
        .and_then(Value::as_object)
        .map(|data| data.keys().cloned().collect())
        .unwrap_or_default();
    Some(SecretObservationStatus::Present { keys })
}

pub(crate) fn prepare_gateway_activation(
    profile: &LoadedProfile,
) -> Result<Option<PreparedGatewayActivation>> {
    let Some(activation) = &profile.definition.gateway_activation else {
        return Ok(None);
    };
    let control_plane_path = profile.resolve(&activation.control_plane);
    let control_plane_text = fs::read_to_string(&control_plane_path).with_context(|| {
        format!(
            "reading gateway activation control plane {}",
            control_plane_path.display()
        )
    })?;
    let control_plane: GatewayControlPlane = serde_json::from_str(&control_plane_text)
        .with_context(|| {
            format!(
                "decoding gateway activation control plane {}",
                control_plane_path.display()
            )
        })?;
    control_plane
        .validate()
        .context("validating gateway activation control plane")?;

    let jwks_keys = control_plane
        .jwks_file_paths()
        .into_iter()
        .map(gateway_mount_key)
        .collect::<Result<BTreeSet<_>>>()?;
    let ca_keys = control_plane
        .certificate_authority_file_paths()
        .into_iter()
        .map(gateway_mount_key)
        .collect::<Result<BTreeSet<_>>>()?;
    let referenced = jwks_keys.union(&ca_keys).cloned().collect::<BTreeSet<_>>();
    let configured = activation
        .public_files
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        referenced == configured,
        "gateway activation publicFiles differ from control-plane file references; referenced={referenced:?}, configured={configured:?}"
    );

    let mut data = BTreeMap::from([(activation.control_plane_key.clone(), control_plane_text)]);
    for (key, path) in &activation.public_files {
        let resolved = profile.resolve(path);
        let text = fs::read_to_string(&resolved).with_context(|| {
            format!(
                "reading gateway activation public file {}",
                resolved.display()
            )
        })?;
        validate_gateway_public_file(key, &text, jwks_keys.contains(key), ca_keys.contains(key))?;
        data.insert(key.clone(), text);
    }

    let bundle_digest = gateway_bundle_digest(&data)?;
    let digest = bundle_digest
        .as_str()
        .strip_prefix("sha256:")
        .expect("validated SHA-256 digest")
        .to_owned();
    Ok(Some(PreparedGatewayActivation {
        config_map_name: format!("{}-{}", activation.config_map_name_prefix, &digest[..12]),
        revision: digest,
        confidential_secret: activation.confidential_secret.clone(),
        required_secret_keys: activation.required_secret_keys.clone(),
        data,
    }))
}

pub(crate) fn gateway_mount_key(path: &str) -> Result<String> {
    let key = path.strip_prefix(GATEWAY_MOUNT_ROOT).with_context(|| {
        format!("gateway public file path {path:?} must be beneath {GATEWAY_MOUNT_ROOT}")
    })?;
    ensure!(
        !key.is_empty() && !key.contains('/'),
        "gateway public file path {path:?} must resolve to one ConfigMap data key"
    );
    Ok(key.to_owned())
}

pub(crate) fn validate_gateway_public_file(
    key: &str,
    text: &str,
    jwks: bool,
    ca: bool,
) -> Result<()> {
    if jwks {
        let jwks: jsonwebtoken::jwk::JwkSet = serde_json::from_str(text)
            .with_context(|| format!("decoding gateway JWKS file {key}"))?;
        ensure!(!jwks.keys.is_empty(), "gateway JWKS file {key} has no keys");
    }
    if ca {
        let certificates = reqwest::Certificate::from_pem_bundle(text.as_bytes())
            .with_context(|| format!("parsing gateway CA bundle {key}"))?;
        ensure!(
            !certificates.is_empty(),
            "gateway CA bundle {key} has no certificates"
        );
    }
    Ok(())
}

pub(crate) fn apply_gateway_activation(
    context: &str,
    namespace: &str,
    activation: &PreparedGatewayActivation,
) -> Result<()> {
    kubectl_apply_value(
        context,
        &serde_json::json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "metadata": {
                "name": activation.config_map_name,
                "namespace": namespace,
                "labels": {
                    "app.kubernetes.io/managed-by": "veoveo-profile",
                    "veoveo.ai/gateway-activation": "true"
                },
                "annotations": {
                    "veoveo.ai/gateway-activation-revision": activation.revision
                }
            },
            "immutable": true,
            "data": activation.data
        }),
    )
}
