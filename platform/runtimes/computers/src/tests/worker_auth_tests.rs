//! HTTPS OAuth fixtures establish worker transport, not native provider policy.
use super::*;
use std::sync::atomic::Ordering;
#[path = "../../tests/support/worker_issuer.rs"]
mod worker_issuer;
pub(super) use worker_issuer::TestIssuer;
#[tokio::test]
async fn provider_rpcs_require_bearer_and_share_predispatch_credentials() {
    let running = Running::start().await;
    assert_eq!(running.authentication.requests.load(Ordering::SeqCst), 1);
    running.runtime.ready().await.unwrap();
    running.runtime.get(&binding()).await.unwrap();
    assert_eq!(running.authentication.requests.load(Ordering::SeqCst), 1);
}

#[test]
fn worker_config_requires_external_identity_and_explicit_credential_method() {
    let input = serde_json::json!({"issuer":"https://issuer.fixture/", "audience":"resource", "clientId":"worker",
        "clientSecretFile":"/private/worker-secret", "tokenEndpointAuthMethod":"client_secret_post", "scopes":["https://resource.fixture/.default"], "caFile":null});
    assert!(serde_json::from_value::<WorkerOAuthConfig>(input.clone()).is_ok());
    for field in [
        "issuer",
        "audience",
        "clientId",
        "clientSecretFile",
        "tokenEndpointAuthMethod",
        "scopes",
    ] {
        let mut missing = input.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<WorkerOAuthConfig>(missing).is_err(),
            "missing {field}"
        );
    }
    for (field, value) in [
        ("scopes", serde_json::json!([])),
        ("audience", serde_json::json!("")),
        (
            "tokenEndpointAuthMethod",
            serde_json::json!("client_secret_basic"),
        ),
        ("clientSecretFile", serde_json::json!("relative")),
    ] {
        let mut invalid = input.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<WorkerOAuthConfig>(invalid).is_err());
    }
}

#[tokio::test]
async fn credentials_refresh_before_dispatch_and_reject_unsafe_short_expiry() {
    let tls = TlsFiles::new();
    let issuer = TestIssuer::start(&tls.dir, &tls.server_cert, &tls.server_key).await;
    issuer.lifetime.store(31, Ordering::SeqCst);
    let tokens = crate::worker_auth::WorkerTokens::connect(issuer.config.clone())
        .await
        .unwrap();
    tokens.token().await.unwrap();
    assert_eq!(issuer.requests.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    tokens.token().await.unwrap();
    assert_eq!(issuer.requests.load(Ordering::SeqCst), 2);
    issuer.lifetime.store(10, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert!(matches!(
        tokens.token().await,
        Err(RuntimeFailure::InvalidWorkerAuthentication)
    ));
    assert_eq!(issuer.requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn ambiguous_mutation_auth_reply_does_not_refresh_or_replay() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().deletion_reply = 3;
    let mut client = running.runtime.client.clone();
    let result = client
        .delete_sandbox(api::DeleteSandboxRequest {
            name: binding().name(),
            workspace_scope: crate::client::workspace_scope("computers"),
            ..Default::default()
        })
        .await;
    assert!(matches!(result, Err(status) if status.code() == tonic::Code::Unauthenticated));
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.deletes, 1);
    assert!(
        state.sandbox.is_none(),
        "mutation occurred before the ambiguous auth reply"
    );
    assert_eq!(running.authentication.requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn issuer_probes_preserve_known_key_and_selected_defects_then_shutdown() {
    use worker_issuer::WorkerTokenProbe;
    let tls = TlsFiles::new();
    let mut issuer = TestIssuer::start(&tls.dir, &tls.server_cert, &tls.server_key).await;
    let client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(tls.ca.as_bytes()).unwrap())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let jwks_url = issuer.config.issuer().as_url().join("jwks").unwrap();
    let jwks: jsonwebtoken::jwk::JwkSet = client
        .get(jwks_url.clone())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key = jsonwebtoken::DecodingKey::from_jwk(&jwks.keys[0]).unwrap();
    let mut signature_only = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    signature_only.validate_exp = false;
    signature_only.validate_aud = false;
    for probe in [
        WorkerTokenProbe::Authorized,
        WorkerTokenProbe::WrongSignature,
        WorkerTokenProbe::WrongIssuer,
        WorkerTokenProbe::WrongAudience,
        WorkerTokenProbe::Expired,
        WorkerTokenProbe::UnauthorizedRoles,
    ] {
        let token = issuer.probe_token(probe);
        assert_eq!(
            jsonwebtoken::decode_header(&token).unwrap().kid.as_deref(),
            Some("fixture-worker")
        );
        let result = jsonwebtoken::decode::<serde_json::Value>(&token, &key, &signature_only);
        if matches!(probe, WorkerTokenProbe::WrongSignature) {
            assert!(
                result.is_err(),
                "foreign key must fail the published JWKS key"
            );
            continue;
        }
        let claims = result.unwrap().claims;
        assert_eq!(
            claims["iss"].as_str() == Some(issuer.config.issuer().as_str()),
            !matches!(probe, WorkerTokenProbe::WrongIssuer)
        );
        assert_eq!(
            claims["aud"].as_str() == Some(issuer.config.audience()),
            !matches!(probe, WorkerTokenProbe::WrongAudience)
        );
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(
            claims["exp"].as_u64().unwrap() > now,
            !matches!(probe, WorkerTokenProbe::Expired)
        );
        assert_eq!(
            claims["roles"]
                .as_array()
                .unwrap()
                .iter()
                .any(|role| role == "openshell-user"),
            !matches!(probe, WorkerTokenProbe::UnauthorizedRoles)
        );
    }
    issuer.shutdown().await;
    assert!(
        client.get(jwks_url).send().await.is_err(),
        "owned issuer listener closed"
    );
}
