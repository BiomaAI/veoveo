use crate::{
    runtime::{AppState, current_catalog, public_authorization_server},
    tokens::authorization_server_jwks_from_signing_key,
};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::IntoResponse,
};
use veoveo_mcp_contract::GatewayProfileId;
pub(super) async fn protected_resource_metadata(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::parse(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let catalog = current_catalog(&state.catalog);
    match catalog.protected_resource_metadata(&profile_id) {
        Ok(metadata) => {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn authorization_server_metadata(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let catalog = current_catalog(&state.catalog);
    let Some(authorization_server) = public_authorization_server(&catalog, &state.public_base_url)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match catalog.authorization_server_metadata_for_server(&authorization_server.id) {
        Ok(mut metadata) => {
            metadata.jwks_uri = Some(format!(
                "{}/oauth/jwks.json",
                state.public_base_url.trim_end_matches('/')
            ));
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(super) async fn authorization_server_jwks(
    State(state): State<AppState>,
) -> axum::response::Response {
    let catalog = current_catalog(&state.catalog);
    let Some(authorization_server) = public_authorization_server(&catalog, &state.public_base_url)
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let jwks =
        match authorization_server_jwks_from_signing_key(&catalog, authorization_server).await {
            Ok(jwks) => jwks,
            Err(err) => {
                tracing::error!("failed to build authorization server JWKS: {err}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300, must-revalidate"),
    );
    (StatusCode::OK, headers, Json(jwks)).into_response()
}

pub(super) use veoveo_mcp_gateway::http::authenticate_profile as authenticate_mcp;

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::runtime::ProfileAuthState;
    use axum::http::{Request, header::AUTHORIZATION};
    use axum::{Router, body::Body, http::header::WWW_AUTHENTICATE, middleware, routing::post};
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use chrono::Utc;
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use parking_lot::RwLock;
    use serde::Serialize;
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use tower::ServiceExt;
    use veoveo_audit_contract::{
        AuditClass, AuditDetail, AuditOutcome, AuditPartition, AuditQuery, AuditReadScope,
    };
    use veoveo_mcp_contract::AuthReasonCode;
    use veoveo_mcp_contract::{GatewayControlPlane, JwksSource};
    use veoveo_mcp_gateway::GatewayCatalog;
    use veoveo_mcp_gateway::GatewayCatalogHandle;

    pub(crate) struct PublicKeyFile(PathBuf);
    impl Drop for PublicKeyFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[derive(Serialize)]
    struct Claims {
        iss: &'static str,
        sub: &'static str,
        principal_id: &'static str,
        client_id: &'static str,
        aud: &'static str,
        work_context: &'static str,
        invocation_mode: veoveo_types::InvocationMode,
        initiator: &'static str,
        tenant: &'static str,
        scope: &'static str,
        exp: i64,
    }

    pub(crate) async fn profile_fixture(
        empty: Option<veoveo_mcp_contract::DiscoveryFailureMode>,
    ) -> (
        crate::test_store::TestDb,
        ProfileAuthState,
        PublicKeyFile,
        rcgen::KeyPair,
    ) {
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
        let db = crate::test_store::TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                crate::test_store::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let key_file = PublicKeyFile(
            std::env::temp_dir().join(format!("gateway-expiry-{}.jwks.json", uuid::Uuid::now_v7())),
        );
        std::fs::write(
            &key_file.0,
            serde_json::to_vec(&serde_json::json!({"keys": [{
                "kty": "OKP", "crv": "Ed25519", "x": URL_SAFE_NO_PAD.encode(key.public_key_raw()),
                "alg": "EdDSA", "use": "sig", "kid": "expiry-test"
            }]}))
            .unwrap(),
        )
        .unwrap();
        let mut control: GatewayControlPlane = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../computers/tests/support/gateway.json"
        )))
        .unwrap();
        control.authorization_servers[0].jwks = JwksSource::File {
            path: veoveo_mcp_contract::JwksFilePath::new(key_file.0.to_str().unwrap()).unwrap(),
        };
        if let Some(mode) = empty {
            control.servers.clear();
            for profile in &mut control.profiles {
                profile.servers.clear();
                profile.discovery_failure_mode = mode;
            }
            for policy in &mut control.policies {
                policy.rules.clear();
            }
        }
        let state = ProfileAuthState {
            catalog: GatewayCatalogHandle::new(Arc::new(
                GatewayCatalog::from_control_plane(control, crate::catalog_admission().unwrap())
                    .unwrap(),
            )),
            gateway_state: crate::bindings::gateway_state(
                db.a.clone(),
                Arc::new(Default::default()),
            )
            .unwrap(),
            deployment: veoveo_mcp_contract::PublicDeployment::new("https://computers.test")
                .unwrap(),
            auth_http: Arc::new(RwLock::new(reqwest::Client::new())),
        };
        (db, state, key_file, key)
    }

    pub(crate) fn fixture_bearer(key: &rcgen::KeyPair, offset: i64) -> secrecy::SecretString {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some("expiry-test".to_owned());
        encode(
            &header,
            &Claims {
                iss: "https://computers.test",
                sub: "alice",
                principal_id: "https://computers.test#alice",
                client_id: "console",
                aud: "https://computers.test/mcp/operator",
                work_context: "computers-test",
                invocation_mode: veoveo_types::InvocationMode::Direct,
                initiator: "https://computers.test#alice",
                tenant: "test",
                scope: "operator:use",
                exp: Utc::now().timestamp() + offset,
            },
            &EncodingKey::from_ed_der(&key.serialize_der()),
        )
        .unwrap()
        .into()
    }

    #[tokio::test]
    async fn expired_bearer_returns_401_before_dispatch_and_commits_denial() {
        use secrecy::ExposeSecret;
        let (db, state, _key_file, key) = profile_fixture(None).await;
        let app = Router::new()
            .route(
                "/arbitrary/extension/{profile}/nested",
                post(|| async { StatusCode::NO_CONTENT }),
            )
            .route_layer(middleware::from_fn_with_state(state, authenticate_mcp));
        tokio::time::timeout(Duration::from_secs(30), async {
            // A valid control proves this fixture can pass all the real middleware gates.
            // Expired cases cover the default JWT leeway and its exact expiry second.
            for offset in [120, -41, -1, 0, -120] {
                let token = fixture_bearer(&key, offset);
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/arbitrary/extension/operator/nested")
                            .header(AUTHORIZATION, format!("Bearer {}", token.expose_secret()))
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                if offset > 0 {
                    assert_eq!(response.status(), StatusCode::NO_CONTENT);
                } else {
                    assert_eq!(
                        response.status(),
                        StatusCode::UNAUTHORIZED,
                        "expiry offset {offset}"
                    );
                    assert!(
                        response.headers()[WWW_AUTHENTICATE]
                            .to_str()
                            .unwrap()
                            .contains("/.well-known/oauth-protected-resource/mcp/operator")
                    );
                    let body = axum::body::to_bytes(response.into_body(), 1024)
                        .await
                        .unwrap();
                    assert_eq!(body.as_ref(), b"authorization required for gateway profile");
                }
            }
            let mut query = AuditQuery::new(AuditPartition::Installation);
            query.class = Some(AuditClass::Authentication);
            let page =
                db.b.audit_page(&AuditReadScope::new(None, true), &query)
                    .await
                    .unwrap();
            assert_eq!(page.records.len(), 4);
            for record in page.records {
                assert_eq!(record.draft.outcome(), AuditOutcome::Denied);
                assert!(matches!(
                    record.draft.detail(),
                    AuditDetail::Authentication {
                        reason: AuthReasonCode::InvalidBearerToken,
                        ..
                    }
                ));
            }
        })
        .await
        .expect("authentication regression exceeded 30 seconds");
    }
}
