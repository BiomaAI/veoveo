mod admission;

use super::*;
use kubernetes_types::*;
use serde_json::json;
use veoveo_mcp_contract::agent_management as wire;
use veoveo_platform_store::agent_management::{AgentRevision, instances::*};

fn fixture() -> (Config, ManagedAgentReconciliation, ConfigMap) {
    let data = std::collections::BTreeMap::from([
        ("manifest.json".into(), "{}".into()),
        (
            "0001_memory.sql".into(),
            "CREATE TABLE notes (text VARCHAR);".into(),
        ),
    ]);
    let model: wire::ModelConnection = serde_json::from_value(json!({"id":"approved","name":"Approved","provider":"fixture","tenant":"test","work_contexts":["operations"],"base_url":"https://model.test/v1","model":"approved","api_key":"model-key","limits":{"maxOutputTokens":128,"maxCompletionCalls":2,"maxToolCalls":3,"deadlineSeconds":60}})).unwrap();
    let template: wire::RuntimeTemplate = serde_json::from_value(json!({
        "id":"pilot","name":"Pilot","tenant":"test","work_contexts":["operations"],"required_deployer_scopes":["operator:use"],"profile":"operator","scopes":["operator:use"],"roles":[],"membership":"contributor",
        "models":["approved"],"tools":["time__resolve_time"],"resource_subscriptions":[],"parameters":{"vehicle":{"label":"Vehicle","shape":{"kind":"identifier","maxLength":40},"environment_variable":"VEOVEO_PARAM_VEHICLE"}},
        "workload":{"namespace":"agents","config_map":"pilot-template","config_digest":wire::runtime_config_revision(&data),"image":format!("registry.test/kernel@sha256:{}", "a".repeat(64)),"database_secret":"agent-store","storage_class":"local-path","storage_gib":2,"cpu_millis":500,"memory_mib":512,"model_secrets":[{"reference":"model-key","secret":"approved-model","key":"api-key"}]}
    })).unwrap();
    let tenant = veoveo_platform_store::deterministic_tenant_id("test")
        .unwrap()
        .record_id();
    let id = managed_agent_record(&tenant, "worker").unwrap();
    let definition =
        veoveo_platform_store::agent_management::agent_definition_record(&tenant, "pilot").unwrap();
    let principal = veoveo_platform_store::deterministic_principal_id("test", "worker")
        .unwrap()
        .record_id();
    let instance = serde_json::from_value(json!({
        "id":id,"tenant":tenant,"work_context":veoveo_platform_store::deterministic_work_context_id("test", "operations").unwrap().record_id(),"owner":principal,"deployed_by":principal,"key":"worker","name":"Worker","definition":definition,"requested_revision":definition,"active_revision":definition,
        "generation":2,"active_generation":2,"dispatch_epoch":1,"desired":"running","observed":"workload","principal":principal,
        "identity":{"client_id":"worker-client","issuer":"https://gateway.test/oauth","authorization_server":"gateway","profile":"operator","resource":"https://gateway.test/mcp/operator","scopes":["operator:use"],"roles":[],"membership":"contributor"},
        "resources":{"namespace":"agents","workload":"agent-worker","credential_secret":"agent-worker-key","volume_claim":"retained-pilot-memory","template_config_map":"pilot-template","image":template.workload.image,"storage_gib":2},
        "public_key":{"kid":"same-key","n":"modulus","e":"AQAB"},"operation":id,"created_at":chrono::Utc::now(),"updated_at":chrono::Utc::now()
    })).unwrap();
    let content = serde_json::from_value(json!({
        "model":{"id":"approved","revision":model.revision().as_str().trim_start_matches("sha256:")},"instructions":"PRIVATE_AUTHORED_INSTRUCTIONS","tools":["time__resolve_time"],"budgets":{"max_output_tokens":64,"max_completion_calls":1,"max_tool_calls":2,"deadline_seconds":30},
        "execution":{"kind":"managed","template":"pilot","template_revision":wire::runtime_template_revision(&template).as_str().trim_start_matches("sha256:"),"parameters":{"vehicle":"vehicle-one"},"resource_subscriptions":[]}
    })).unwrap();
    let snapshot = ManagedAgentReconciliation {
        instance,
        revision: AgentRevision {
            id: definition.clone(),
            definition,
            digest: "a".repeat(64),
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
