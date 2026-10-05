//! Native fixture admission and key checks; no installed workload or provider calls.
use super::super::Args;
use super::Configuration;
use base64::{Engine, engine::general_purpose::STANDARD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde_json::{Value, json};
use std::{collections::BTreeSet, os::unix::fs::PermissionsExt};
use veoveo_agent_runtime::{catalog::installation_facts, contract::authoring as wire};
use veoveo_mcp_gateway::{
    ClientAssertionConfig, ClientAssertionVerifier, GatewayCatalog, GatewayCatalogAdmission,
};

#[test]
fn generated_configuration_is_admitted_and_der_key_signs_gateway_assertion() {
    let directory = tempfile::tempdir().unwrap();
    let image = |name: &str, digest: char| {
        format!(
            "registry.invalid/{name}@sha256:{}",
            digest.to_string().repeat(64)
        )
        .parse()
        .unwrap()
    };
    let args = Args {
        context: "fixture-not-used".into(),
        gateway_image: image("gateway", 'a'),
        manager_image: image("manager", 'b'),
        kernel_image: image("kernel", 'c'),
        evidence_output: directory.path().join("unused-evidence.json"),
    };
    let configuration = Configuration::create(&args, "fixture-recovery", directory.path()).unwrap();
    let registry = veoveo_gateway_catalog::registry().unwrap();
    GatewayCatalog::from_control_plane(
        configuration.plane.clone(),
        GatewayCatalogAdmission::unbound()
            .bind(registry.clone())
            .unwrap(),
    )
    .unwrap();
    assert!(configuration.plane.servers.is_empty());
    assert!(configuration.plane.profiles[0].servers.is_empty());
    assert!(configuration.plane.policies[0].rules.is_empty());
    // Empty authenticated discovery returns an empty catalog. A server-scoped
    // grant cannot manufacture capabilities for this recovery-only profile.
    for action in [
        "tools_list",
        "resources_list",
        "resources_templates_list",
        "prompts_list",
        "tasks_get",
    ] {
        let mut invalid = configuration.plane.clone();
        invalid.policies[0].rules.push(
            serde_json::from_value(json!({
                "id":"invalid-fixture-discovery", "effect":"allow", "actions":[action],
                "profiles":["operator"], "required_scopes":["operator:use"]
            }))
            .unwrap(),
        );
        match invalid.validate(&registry) {
            Err(veoveo_mcp_contract::GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {action: rejected, ..}) => assert_eq!(rejected.as_str(), action),
            other => panic!("unsupported empty-server grant was not rejected by capability admission: {other:?}"),
        }
    }
    let facts = installation_facts(&configuration.plane, &registry).unwrap();
    wire::validate_model_connections(std::slice::from_ref(&configuration.model), &facts).unwrap();
    configuration.template.validate(&facts).unwrap();
    assert!(
        configuration
            .template
            .models
            .contains(&configuration.model.id)
    );
    assert_eq!(
        configuration.template.workload.config_digest,
        wire::runtime_config_revision(&configuration.data)
    );
    assert_eq!(
        configuration.template.workload.image,
        args.kernel_image.reference()
    );
    assert_eq!(configuration.namespace, "fixture-recovery-agents");
    assert_eq!(configuration.model.base_url, "https://model.invalid/v1");
    assert!(configuration.template.tools.is_empty());
    assert!(configuration.template.resource_subscriptions.is_empty());
    assert!(configuration.template.parameters.is_empty());
    assert_eq!(
        configuration
            .template
            .roles
            .iter()
            .map(|role| role.as_str())
            .collect::<Vec<_>>(),
        vec!["fixture-managed"]
    );
    let manifest: Value = serde_json::from_str(&configuration.data["manifest.json"]).unwrap();
    assert_eq!(manifest["resource_subscriptions"], json!([]));
    assert_eq!(manifest["model"]["base_url"], "${VEOVEO_AGENT_MODEL_URL}");
    assert_eq!(manifest["model"]["api_key_env"], "VEOVEO_MANAGED_MODEL_KEY");
    assert_eq!(manifest["schedule"]["heartbeat_interval_s"], 3600);

    // Admission must depend on real registered secret/context facts.
    let mut missing_secret = configuration.plane.clone();
    missing_secret
        .secrets
        .retain(|secret| secret.id != configuration.model.api_key);
    let missing_facts = installation_facts(&missing_secret, &registry).unwrap();
    assert!(
        wire::validate_model_connections(
            std::slice::from_ref(&configuration.model),
            &missing_facts
        )
        .is_err()
    );
    let mut uninstalled_template = configuration.template.clone();
    uninstalled_template.profile = "uninstalled".parse().unwrap();
    assert!(uninstalled_template.validate(&facts).is_err());

    let private_der = STANDARD
        .decode(&configuration.installation_secrets["authorization-server-private-key-der-b64"])
        .unwrap();
    let internal_der = STANDARD
        .decode(&configuration.installation_secrets["internal-signing-key-der-b64"])
        .unwrap();
    veoveo_mcp_contract::GatewayInternalSigningKey::new("fixture-internal", internal_der)
        .expect("generated internal DER must pass the gateway signing-key constructor");
    assert_eq!(
        std::fs::metadata(directory.path().join("authorization.der"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(directory.path().join("internal.der"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("fixture-as".into());
    let now = chrono::Utc::now().timestamp();
    let claims = json!({"iss":"fixture-client","sub":"fixture-client", "aud":"https://gateway.invalid/oauth/token", "iat":now,"nbf":now-1,"exp":now+60,"jti":"fixture-key-qualification"});
    // Same DER admission/signing API used by the gateway authorization server.
    let token = encode(&header, &claims, &EncodingKey::from_rsa_der(&private_der))
        .expect("generated RSA DER must sign through gateway JWT library");
    let verifier = ClientAssertionVerifier::new(
        ClientAssertionConfig::new(
            "fixture-client".parse().unwrap(),
            "https://gateway.invalid/oauth/token",
            vec![Algorithm::RS256],
        )
        .unwrap(),
        serde_json::from_str(&configuration.jwks).unwrap(),
    );
    let verified = verifier.verify(&token).unwrap();
    assert_eq!(verified.client_id.as_str(), "fixture-client");
    let wrong_audience = ClientAssertionVerifier::new(
        ClientAssertionConfig::new(
            "fixture-client".parse().unwrap(),
            "https://gateway.invalid/another-token",
            vec![Algorithm::RS256],
        )
        .unwrap(),
        serde_json::from_str(&configuration.jwks).unwrap(),
    );
    assert!(wrong_audience.verify(&token).is_err());
    assert_eq!(
        configuration.template.required_deployer_scopes,
        BTreeSet::from(["operator:use".parse().unwrap()])
    );
}

#[path = "../../../../fixtures/store.rs"]
mod database;

#[test]
fn sdk_live_teardown_outside_block_on_preserves_setup_errors() -> anyhow::Result<()> {
    use super::{AgentLive, setup_failure};
    use std::{sync::Arc, time::Duration};
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?,
    );
    let fixture = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(180), database::TestDb::new()).await
    })?;
    let store = runtime.block_on(fixture.admin());
    // Real SDK streams are registered while entered, then all three teardown
    // paths execute on this ordinary test thread with no Tokio context.
    assert!(tokio::runtime::Handle::try_current().is_err());
    let mut explicit = AgentLive::start(Arc::clone(&runtime), store.clone())?;
    explicit.close()?;
    assert!(explicit.stream.is_none());
    explicit.close()?;
    drop(explicit);
    let mut failed = AgentLive::start(Arc::clone(&runtime), store.clone())?;
    let error = failed
        .finish_setup::<()>(Err(anyhow::anyhow!("watch setup sentinel")))
        .unwrap_err();
    assert_eq!(error.to_string(), "watch setup sentinel");
    assert!(failed.stream.is_none());
    drop(failed);
    let cleanup_error = setup_failure(
        anyhow::anyhow!("primary setup sentinel"),
        Err(anyhow::anyhow!("cleanup sentinel")),
    );
    let diagnostic = format!("{cleanup_error:#}");
    assert!(diagnostic.contains("primary setup sentinel"));
    assert!(diagnostic.contains("cleanup sentinel"));
    let automatic = AgentLive::start(Arc::clone(&runtime), store)?;
    drop(runtime); // The LIVE owner is now the sole runtime owner.
    drop(automatic);
    drop(fixture);
    assert!(tokio::runtime::Handle::try_current().is_err());
    Ok(())
}
