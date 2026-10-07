mod admission;
mod installation_capture;

use super::*;
use kubernetes_types::*;
use serde_json::json;
use veoveo_agent_runtime::contract::authoring as wire;
use veoveo_agent_runtime::persistence::{AgentRevision, instances::*};

fn fixture() -> (Config, ManagedAgentReconciliation, ConfigMap) {
    let data = std::collections::BTreeMap::from([
        ("manifest.json".into(), "{}".into()),
        (
            "0001_memory.sql".into(),
            "CREATE TABLE notes (text VARCHAR);".into(),
        ),
    ]);
    let model: wire::ModelConnection = serde_json::from_value(json!({"id":"approved","name":"Approved","provider":"fixture","tenant":"test","workContexts":["operations"],"baseUrl":"https://model.test/v1","model":"approved","apiKey":"model-key","limits":{"maxOutputTokens":128,"maxCompletionCalls":2,"maxToolCalls":3,"deadlineSeconds":60}})).unwrap();
    let template: wire::RuntimeTemplate = serde_json::from_value(json!({
        "id":"pilot","name":"Pilot","tenant":"test","workContexts":["operations"],"requiredDeployerScopes":["operator:use"],"profile":"operator","scopes":["operator:use"],"roles":[],"membership":"contributor",
        "models":["approved"],"tools":["time__resolve_time"],"resourceSubscriptions":[],"parameters":{"vehicle":{"label":"Vehicle","shape":{"kind":"identifier","maxLength":40},"environmentVariable":"VEOVEO_PARAM_VEHICLE"}},
        "workload":{"namespace":"agents","configMap":"pilot-template","configDigest":wire::runtime_config_revision(&data),"image":format!("registry.test/kernel@sha256:{}", "a".repeat(64)),"databaseSecret":"agent-store","storageClass":"local-path","storageGib":2,"cpuMillis":500,"memoryMib":512,"modelSecrets":[{"reference":"model-key","secret":"approved-model","key":"api-key"}]}
    })).unwrap();
    let tenant = veoveo_platform_store::deterministic_tenant_id("test")
        .unwrap()
        .record_id();
    let id = managed_agent_record(&tenant, "worker").unwrap();
    let definition =
        veoveo_agent_runtime::persistence::agent_definition_record(&tenant, "pilot").unwrap();
    let principal = veoveo_platform_store::deterministic_principal_id("test", "worker")
        .unwrap()
        .record_id();
    let instance = serde_json::from_value(json!({
        "id":id,"tenant":tenant,"work_context":veoveo_platform_store::deterministic_work_context_id("test", "operations").unwrap().record_id(),"owner":principal,"deployed_by":principal,"key":"worker","name":"Worker","definition":definition,"requested_revision":definition,"active_revision":definition,
        "generation":2,"active_generation":2,"admission_count":0,"dispatch_epoch":1,"desired":"running","observed":"workload","principal":principal,
        "identity":{"client_id":"worker-client","issuer":"https://gateway.test/oauth","authorization_server":"gateway","profile":"operator","resource":"https://gateway.test/mcp/operator","scopes":["operator:use"],"roles":[],"membership":"contributor"},
        "resources":{"namespace":"agents","workload":"agent-worker","credential_secret":"agent-worker-key","volume_claim":"retained-pilot-memory","template_config_map":"pilot-template","image":template.workload.image,"storage_gib":2},
        "public_key":{"kid":"same-key","n":"modulus","e":"AQAB"},"operation":id,"created_at":chrono::Utc::now(),"updated_at":chrono::Utc::now()
    })).unwrap();
    let content: veoveo_agent_runtime::persistence::AgentContent = serde_json::from_value(json!({
        "model":{"id":"approved","revision":model.revision().as_str().trim_start_matches("sha256:")},"instructions":"PRIVATE_AUTHORED_INSTRUCTIONS","tools":["time__resolve_time"],"budgets":{"max_output_tokens":64,"max_completion_calls":1,"max_tool_calls":2,"deadline_seconds":30},
        "execution":{"kind":"managed","template":"pilot","template_revision":wire::runtime_template_revision(&template).as_str().trim_start_matches("sha256:"),"parameters":{"vehicle":"vehicle-one"},"resource_subscriptions":[]}
    })).unwrap();
    let snapshot = ManagedAgentReconciliation {
        instance,
        revision: AgentRevision {
            id: definition.clone(),
            definition,
            digest: content.digest().unwrap(),
            execution: content.execution.clone(),
            model: content.model.clone(),
            tools: content.tools.clone(),
            template_revision: match &content.execution {
                veoveo_agent_runtime::persistence::AgentExecution::Managed {
                    template_revision,
                    ..
                } => Some(template_revision.clone()),
                veoveo_agent_runtime::persistence::AgentExecution::Chat => None,
            },
            content,
            created_by: principal,
            created_at: chrono::Utc::now(),
        },
        runtime: None,
        episode_running: false,
        tenant_key: "test".into(),
        context_key: "operations".into(),
        enabled: true,
    };
    let config = Config {
        namespace: "agents".into(),
        gateway_url: "https://gateway.test".into(),
        gateway_transport_url: "http://gateway.internal".into(),
        store_endpoint: "ws://store.internal:8000".into(),
        store_namespace: "veoveo".into(),
        store_database: "platform".into(),
        database_credential_revision: veoveo_modules::CredentialRevision::new("fixture-1").unwrap(),
        templates: vec![template],
        models: vec![model],
    };
    let config_map = ConfigMap {
        metadata: Metadata {
            name: "pilot-template".into(),
            ..Default::default()
        },
        immutable: true,
        data,
    };
    (config, snapshot, config_map)
}

#[test]
fn composition_retains_memory_and_never_embeds_instructions_or_secret_values() {
    let (config, snapshot, config_map) = fixture();
    let (template, model) = config.execution(&snapshot).unwrap();
    let items = resources::configuration_items(&config_map, template).unwrap();
    let deployment = resources::deployment(&config, &snapshot, template, model, items).unwrap();
    let body = serde_json::to_value(&deployment).unwrap();
    let pod = &body["spec"]["template"]["spec"];
    assert_eq!(pod["automountServiceAccountToken"], false);
    assert_eq!(pod["serviceAccountName"], "veoveo-agent-kernel");
    assert_eq!(pod["securityContext"]["runAsUser"], 10001);
    assert_eq!(
        pod["containers"][0]["securityContext"]["readOnlyRootFilesystem"],
        true
    );
    assert_eq!(
        pod["volumes"][1]["persistentVolumeClaim"]["claimName"],
        "retained-pilot-memory"
    );
    assert!(!body.to_string().contains("PRIVATE_AUTHORED_INSTRUCTIONS"));
    let env = pod["containers"][0]["env"].as_array().unwrap();
    let private_key = env
        .iter()
        .find(|v| v["name"] == "VEOVEO_MANAGED_PRIVATE_KEY")
        .unwrap();
    assert!(private_key.get("value").is_none());
    assert_eq!(
        private_key["valueFrom"]["secretKeyRef"]["name"],
        "agent-worker-key"
    );
    assert_eq!(body["spec"]["strategy"]["type"], "Recreate");
    assert_eq!(pod["terminationGracePeriodSeconds"], 45);
    let pvc = resources::volume_claim(&snapshot.instance, template);
    assert_eq!(pvc.metadata.name, "retained-pilot-memory");
}

#[test]
fn changed_templates_unapproved_tools_and_mutable_configuration_are_rejected() {
    let (mut config, mut snapshot, mut config_map) = fixture();
    snapshot
        .revision
        .content
        .tools
        .push("shell__execute".into());
    assert!(config.execution(&snapshot).is_err());
    snapshot.revision.content.tools.pop();
    config.models[0].base_url = "https://other.test/v1".into();
    assert!(config.execution(&snapshot).is_err());
    config_map.immutable = false;
    assert!(resources::configuration_items(&config_map, &config.templates[0]).is_err());
    config_map.immutable = true;
    config_map
        .data
        .insert("../../escape.sql".into(), "SELECT 1".into());
    config.templates[0].workload.config_digest = wire::runtime_config_revision(&config_map.data);
    assert!(resources::configuration_items(&config_map, &config.templates[0]).is_err());
}

#[test]
fn deployment_readback_accepts_kubernetes_default_volume_fields() {
    let (config, snapshot, config_map) = fixture();
    let template = &config.templates[0];
    let items = resources::configuration_items(&config_map, template).unwrap();
    let deployment =
        resources::deployment(&config, &snapshot, template, &config.models[0], items).unwrap();
    let mut body = serde_json::to_value(deployment).unwrap();
    for mount in body["spec"]["template"]["spec"]["containers"][0]["volumeMounts"]
        .as_array_mut()
        .unwrap()
    {
        if mount["readOnly"] == false {
            mount.as_object_mut().unwrap().remove("readOnly");
        }
    }
    let decoded: Deployment = serde_json::from_value(body).unwrap();
    assert!(decoded.spec.template.spec.containers[0].volume_mounts[0].read_only);
    assert!(!decoded.spec.template.spec.containers[0].volume_mounts[1].read_only);
}

#[test]
fn credential_encoding_matches_the_kernel_pkcs1_signing_boundary() {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, traits::PublicKeyParts};
    let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
    let secret = Secret {
        api_version: "v1".into(),
        kind: "Secret".into(),
        metadata: Metadata::default(),
        immutable: true,
        secret_type: "Opaque".into(),
        data: std::collections::BTreeMap::from([
            (
                "private-key-der-b64".into(),
                STANDARD.encode(STANDARD.encode(private.to_pkcs1_der().unwrap().as_bytes())),
            ),
            ("kid".into(), STANDARD.encode("managed-key")),
        ]),
    };
    let public = credentials::public_key(&secret).unwrap();
    assert_eq!(public.kid, "managed-key");
    assert_eq!(
        public.n,
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(private.n().to_bytes_be())
    );
}

#[test]
fn credential_rotation_retires_then_recovers_only_after_known_drain() {
    use reconcile::{CredentialRecovery as Action, credential_recovery};
    let (mut config, snapshot, config_map) = fixture();
    let template = &config.templates[0];
    let items = resources::configuration_items(&config_map, template).unwrap();
    let existing = resources::deployment(
        &config,
        &snapshot,
        template,
        &config.models[0],
        items.clone(),
    )
    .unwrap();
    assert_eq!(
        credential_recovery(Some(&existing), &snapshot.instance, &config, None).unwrap(),
        Action::Keep
    );
    config.database_credential_revision =
        veoveo_modules::CredentialRevision::new("fixture-2").unwrap();
    assert_eq!(
        credential_recovery(Some(&existing), &snapshot.instance, &config, None).unwrap(),
        Action::Retire
    );
    assert_eq!(
        credential_recovery(None, &snapshot.instance, &config, None).unwrap(),
        Action::WaitForDrain
    );
    assert_eq!(
        credential_recovery(None, &snapshot.instance, &config, Some(false)).unwrap(),
        Action::WaitForDrain
    );
    assert_eq!(
        credential_recovery(None, &snapshot.instance, &config, Some(true)).unwrap(),
        Action::Recover
    );
    let replacement = resources::deployment(
        &config,
        &snapshot,
        &config.templates[0],
        &config.models[0],
        items,
    )
    .unwrap();
    assert_eq!(
        credential_recovery(Some(&replacement), &snapshot.instance, &config, None).unwrap(),
        Action::Keep
    );
    assert_eq!(existing.metadata.name, replacement.metadata.name);
    assert_eq!(
        existing.metadata.annotations,
        replacement.metadata.annotations
    );
    assert_eq!(
        existing.spec.template.metadata.annotations[crate::kubernetes::GENERATION],
        replacement.spec.template.metadata.annotations[crate::kubernetes::GENERATION]
    );
    assert_eq!(
        serde_json::to_value(&existing.spec.template.spec.volumes).unwrap(),
        serde_json::to_value(&replacement.spec.template.spec.volumes).unwrap()
    );
    let mut foreign = replacement;
    foreign.metadata.labels.clear();
    assert!(credential_recovery(Some(&foreign), &snapshot.instance, &config, Some(true)).is_err());
}

#[test]
fn manager_configuration_refuses_retired_and_mixed_keys() {
    let (config, _, _) = fixture();
    let current = json!({
        "namespace":config.namespace,"gatewayUrl":config.gateway_url,
        "gatewayTransportUrl":config.gateway_transport_url,"storeEndpoint":config.store_endpoint,
        "storeNamespace":config.store_namespace,"storeDatabase":config.store_database,
        "databaseCredentialRevision":config.database_credential_revision,
        "templates":config.templates,"models":config.models,
    });
    serde_json::from_slice::<Config>(&serde_json::to_vec(&current).unwrap()).unwrap();
    for (key, old) in [
        ("gatewayUrl", "gateway_url"),
        ("gatewayTransportUrl", "gateway_transport_url"),
        ("storeEndpoint", "store_endpoint"),
        ("storeNamespace", "store_namespace"),
        ("storeDatabase", "store_database"),
        ("databaseCredentialRevision", "database_credential_revision"),
    ] {
        for keep in [false, true] {
            let mut bad = current.clone();
            let object = bad.as_object_mut().unwrap();
            object.insert(old.into(), object[key].clone());
            if !keep {
                object.remove(key);
            }
            assert!(
                serde_json::from_value::<Config>(bad.clone()).is_err(),
                "{old} mixed={keep}"
            );
            assert!(serde_json::from_slice::<Config>(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
}
