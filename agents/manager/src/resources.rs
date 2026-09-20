//! Fixed workload composition from an admitted template and immutable revision.
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use veoveo_mcp_contract::agent_management as wire;
use veoveo_platform_store::agent_management::{
    AgentExecution, AgentTemplateParameter,
    instances::{ManagedAgentInstance, ManagedAgentReconciliation},
};

use crate::{
    config::Config,
    kubernetes::{GENERATION, OWNER_LABEL},
    kubernetes_types::*,
};

pub fn metadata(instance: &ManagedAgentInstance, name: &str) -> Metadata {
    Metadata {
        name: name.into(),
        namespace: Some(instance.resources.namespace.clone()),
        labels: BTreeMap::from([(OWNER_LABEL.into(), instance.resources.workload.clone())]),
        annotations: BTreeMap::from([(GENERATION.into(), instance.generation.to_string())]),
        ..Default::default()
    }
}

pub fn volume_claim(instance: &ManagedAgentInstance, template: &wire::RuntimeTemplate) -> Pvc {
    Pvc {
        api_version: "v1".into(),
        kind: "PersistentVolumeClaim".into(),
        metadata: metadata(instance, &instance.resources.volume_claim),
        spec: PvcSpec {
            access_modes: vec!["ReadWriteOnce".into()],
            storage_class_name: template.workload.storage_class.clone(),
            resources: StorageResources {
                requests: StorageQuantity {
                    storage: format!("{}Gi", instance.resources.storage_gib),
                },
            },
            volume_name: None,
        },
        status: Default::default(),
    }
}

pub fn configuration_items(
    config: &ConfigMap,
    template: &wire::RuntimeTemplate,
) -> Result<Vec<ConfigItem>> {
    ensure!(
        config.immutable && config.metadata.name == template.workload.config_map,
        "template ConfigMap must be immutable"
    );
    ensure!(
        wire::runtime_config_revision(&config.data) == template.workload.config_digest,
        "template ConfigMap digest changed"
    );
    ensure!(
        config.data.contains_key("manifest.json") && config.data.len() <= 65,
        "template requires a manifest and at most 64 migrations"
    );
    config
        .data
        .keys()
        .map(|name| {
            if name == "manifest.json" {
                return Ok(ConfigItem {
                    key: name.clone(),
                    path: name.clone(),
                });
            }
            let valid = name.len() > 9
                && name.len() <= 200
                && name.as_bytes()[..4].iter().all(u8::is_ascii_digit)
                && name.as_bytes()[4] == b'_'
                && name.ends_with(".sql")
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'));
            ensure!(valid, "template contains an unsupported migration filename");
            Ok(ConfigItem {
                key: name.clone(),
                path: format!("migrations/{name}"),
            })
        })
        .collect()
}

fn literal(name: &str, value: impl Into<String>) -> Env {
    Env {
        name: name.into(),
        source: EnvValue::Literal {
            value: value.into(),
        },
    }
}
fn secret(name: &str, secret: &str, key: &str) -> Env {
    Env {
        name: name.into(),
        source: EnvValue::Reference {
            value_from: EnvSource::SecretKeyRef(SecretKey {
                name: secret.into(),
                key: key.into(),
            }),
        },
    }
}

pub fn deployment(
    config: &Config,
    snapshot: &ManagedAgentReconciliation,
    template: &wire::RuntimeTemplate,
    model: &wire::ModelConnection,
    items: Vec<ConfigItem>,
) -> Result<Deployment> {
    let instance = &snapshot.instance;
    let public_key = instance
        .public_key
        .as_ref()
        .context("managed public key is missing")?;
    let model_secret = template
        .workload
        .model_secrets
        .iter()
        .find(|s| s.reference == model.api_key)
        .context("model credential binding is missing")?;
    let mut env = vec![
        literal("VEOVEO_AGENT_TENANT", &snapshot.tenant_key),
        literal("VEOVEO_AGENT_ID", &instance.key),
        literal("VEOVEO_AGENT_NAME", &instance.name),
        literal("VEOVEO_AGENT_PROFILE", &instance.identity.profile),
        literal("VEOVEO_AGENT_WORK_CONTEXT", &snapshot.context_key),
        literal("VEOVEO_AGENT_CLIENT_ID", &instance.identity.client_id),
        literal("VEOVEO_AGENT_KEY_ID", &public_key.kid),
        literal("VEOVEO_GATEWAY_URL", &config.gateway_url),
        literal(
            "VEOVEO_GATEWAY_TRANSPORT_URL",
            &config.gateway_transport_url,
        ),
        literal(
            "VEOVEO_GATEWAY_AUDIENCE",
            format!("{}/oauth/token", config.gateway_url.trim_end_matches('/')),
        ),
        literal("VEOVEO_GATEWAY_RESOURCE", &instance.identity.resource),
        literal("VEOVEO_AGENT_MODEL_URL", &model.base_url),
        literal("VEOVEO_AGENT_MODEL_ID", &model.model),
        literal("VEOVEO_MANAGED_GENERATION", instance.generation.to_string()),
        literal("VEOVEO_MANAGED_MODEL", serde_json::to_string(model)?),
        literal("VEOVEO_SURREAL_ENDPOINT", &config.store_endpoint),
        literal("VEOVEO_SURREAL_NAMESPACE", &config.store_namespace),
        literal("VEOVEO_SURREAL_DATABASE", &config.store_database),
        literal("VEOVEO_SURREAL_AUTH_LEVEL", "database"),
        literal("RUST_LOG", "info"),
        secret(
            "VEOVEO_SURREAL_USERNAME",
            &template.workload.database_secret,
            "username",
        ),
        secret(
            "VEOVEO_SURREAL_PASSWORD",
            &template.workload.database_secret,
            "password",
        ),
        secret(
            "VEOVEO_MANAGED_PRIVATE_KEY",
            &instance.resources.credential_secret,
            "private-key-der-b64",
        ),
        secret(
            "VEOVEO_MANAGED_MODEL_KEY",
            &model_secret.secret,
            &model_secret.key,
        ),
        Env {
            name: "VEOVEO_AGENT_POD_UID".into(),
            source: EnvValue::Reference {
                value_from: EnvSource::FieldRef(FieldRef {
                    field_path: "metadata.uid".into(),
                }),
            },
        },
    ];
    let AgentExecution::Managed { parameters, .. } = &snapshot.revision.content.execution else {
        anyhow::bail!("managed revision is required");
    };
    for (name, value) in parameters {
        let binding = template
            .parameters
            .get(name)
            .context("parameter is not admitted")?;
        let value = match value {
            AgentTemplateParameter::Text(v) => v.clone(),
            AgentTemplateParameter::Integer(v) => v.to_string(),
            AgentTemplateParameter::Boolean(v) => v.to_string(),
        };
        env.push(literal(&binding.environment_variable, value));
    }
    let mut pod_metadata = metadata(instance, &instance.resources.workload);
    pod_metadata.name.clear();
    pod_metadata.namespace = None;
    let quantity = ComputeQuantity {
        cpu: format!("{}m", template.workload.cpu_millis),
        memory: format!("{}Mi", template.workload.memory_mib),
    };
    Ok(Deployment {
        api_version: "apps/v1".into(),
        kind: "Deployment".into(),
        metadata: metadata(instance, &instance.resources.workload),
        spec: DeploymentSpec {
            replicas: 1,
            selector: LabelSelector {
                match_labels: pod_metadata.labels.clone(),
            },
            strategy: Strategy {
                kind: "Recreate".into(),
            },
            revision_history_limit: 1,
            template: PodTemplate {
                metadata: pod_metadata,
                spec: PodSpec {
                    service_account_name: "veoveo-agent-kernel".into(),
                    automount_service_account_token: false,
                    security_context: PodSecurity {
                        run_as_non_root: true,
                        run_as_user: 10001,
                        run_as_group: 10001,
                        fs_group: 10001,
                        seccomp_profile: Seccomp {
                            kind: "RuntimeDefault".into(),
                        },
                    },
                    termination_grace_period_seconds: snapshot
                        .revision
                        .content
                        .budgets
                        .deadline_seconds
                        + 15,
                    containers: vec![Container {
                        name: "agent".into(),
                        image: template.workload.image.clone(),
                        image_pull_policy: "IfNotPresent".into(),
                        args: vec![
                            "run".into(),
                            "--manifest".into(),
                            "/etc/veoveo/agent/manifest.json".into(),
                            "--data-dir".into(),
                            "/var/lib/veoveo/agent".into(),
                        ],
                        env,
                        resources: ComputeResources {
                            requests: quantity.clone(),
                            limits: quantity,
                        },
                        security_context: ContainerSecurity {
                            allow_privilege_escalation: false,
                            read_only_root_filesystem: true,
                            capabilities: Capabilities {
                                drop: vec!["ALL".into()],
                            },
                        },
                        readiness_probe: Probe {
                            exec: Exec {
                                command: vec![
                                    "/usr/bin/test".into(),
                                    "-f".into(),
                                    "/tmp/veoveo-managed-ready".into(),
                                ],
                            },
                            period_seconds: 2,
                            timeout_seconds: 1,
                            failure_threshold: 1,
                        },
                        volume_mounts: vec![
                            VolumeMount {
                                name: "config".into(),
                                mount_path: "/etc/veoveo/agent".into(),
                                read_only: true,
                            },
                            VolumeMount {
                                name: "memory".into(),
                                mount_path: "/var/lib/veoveo/agent".into(),
                                read_only: false,
                            },
                            VolumeMount {
                                name: "tmp".into(),
                                mount_path: "/tmp".into(),
                                read_only: false,
                            },
                        ],
                    }],
                    volumes: vec![
                        Volume {
                            name: "config".into(),
                            source: VolumeSource::ConfigMap(ConfigVolume {
                                name: template.workload.config_map.clone(),
                                items,
                            }),
                        },
                        Volume {
                            name: "memory".into(),
                            source: VolumeSource::PersistentVolumeClaim(ClaimVolume {
                                claim_name: instance.resources.volume_claim.clone(),
                            }),
                        },
                        Volume {
                            name: "tmp".into(),
                            source: VolumeSource::EmptyDir(EmptyDir {
                                size_limit: "128Mi".into(),
                            }),
                        },
                    ],
                },
            },
        },
    })
}
