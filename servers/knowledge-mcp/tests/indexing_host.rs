//! Real machine JWT/HTTP and Store lifetimes. Embeddings are synthetic protocol fixtures.
use veoveo_gateway_contract::ProtectedResourceId;
#[path = "../../../testing/fixtures/knowledge_control.rs"]
mod control_fixture;
#[path = "support/indexing.rs"]
#[allow(dead_code)]
mod embedding;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/hosted.rs"]
#[allow(dead_code)]
mod hosted;

use axum::{
    Router,
    extract::{Form, Request, State},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use rmcp::{
    RoleServer, ServerHandler,
    model::*,
    service::{RequestContext, SubscriptionContext},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, watch};
use tokio_util::{sync::CancellationToken, task::AbortOnDropHandle};
use veoveo_knowledge_mcp::{
    contract::*,
    coordinator::CoordinatorState,
    indexing::{IndexingConfig, IndexingReadiness, IndexingService, SigningAlgorithm},
    source::{MemberLink, SourcePage},
};
use veoveo_mcp_contract::{GatewayControlPlane, OAuthEndpointUrl, SubscriptionHub};
use veoveo_mcp_knowledge_extension::{self as knowledge, *};
use veoveo_types::{ResourceTemplateUri, ResourceUri};

const ROOT: &str = "media://records";
const MEMBER: &str = "media://record/one";
const TEXT: &str = "A completed inspection identifies the northern access route.";
struct StateData {
    public_key: Vec<u8>,
    audience: String,
    resource: String,
    assertions: Mutex<BTreeSet<uuid::Uuid>>,
    tokens: AtomicUsize,
    minted: Notify,
    hub: SubscriptionHub,
    member_reads: AtomicUsize,
    used_tokens: Mutex<BTreeSet<String>>,
    bad_token: AtomicU8,
    redirected_requests: AtomicUsize,
    disconnect: tokio::sync::broadcast::Sender<()>,
}
#[derive(Clone)]
struct Gateway(Arc<StateData>);
fn descriptor() -> CollectionDescriptor {
    CollectionDescriptor::new(
        "media.records".parse().unwrap(),
        "record".parse().unwrap(),
        ResourceTemplateUri::new("media://records{?cursor}").unwrap(),
        Freshness::max_age(120),
        ChangeSignal::Listen,
        AccessModel::Profile,
        IndexingMode::Content,
    )
    .unwrap()
}
impl ServerHandler for Gateway {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .build();
        knowledge::server::declare(&mut capabilities);
        ServerConfig::new(capabilities)
    }
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(requested.clone())
    }
    async fn listen(&self, context: SubscriptionContext) -> Result<(), ErrorData> {
        let mut disconnect = self.0.disconnect.subscribe();
        tokio::select! {
            _ = disconnect.recv() => Ok(()),
            result = veoveo_mcp_contract::listen_resources(context, &self.0.hub, None) => result,
        }
    }
    async fn list_resource_templates(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        let mut template = ResourceTemplate::new("media://record/{id}", "Records");
        knowledge::server::attach_collection(&mut template, &descriptor());
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![template],
            next_cursor: None,
            result_type: Some(ResultType::COMPLETE),
            ttl_ms: Some(0),
            cache_scope: Some(CacheScope::Private),
            meta: None,
        })
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let intent: IndexingReadIntent =
            serde_json::from_value(context.meta.get(INDEXING_READ_KEY).unwrap().clone()).unwrap();
        assert_eq!(intent.collection, *descriptor().collection());
        let result = match request.uri.as_str() {
            "media://contract" => {
                assert_eq!(intent.kind, IndexingReadKind::SourceContract);
                let declaration = veoveo_mcp_contract::docs::ContractDeclaration {
                    server: "media".into(),
                    contract_revision: 3,
                    compliance: vec![],
                };
                ReadResourceResult::new(vec![ResourceContents::text(
                    serde_json::to_string(&declaration).unwrap(),
                    request.uri,
                )])
            }
            ROOT => {
                assert_eq!(intent.kind, IndexingReadKind::Enumeration);
                let page = SourcePage::new(
                    vec![MemberLink {
                        uri: ResourceUri::new(MEMBER).unwrap(),
                        title: Some(MemberTitle::new("Inspection").unwrap()),
                    }],
                    None,
                )
                .unwrap();
                ReadResourceResult::new(vec![ResourceContents::text(
                    serde_json::to_string(&page).unwrap(),
                    request.uri,
                )])
            }
            MEMBER => {
                assert_eq!(intent.kind, IndexingReadKind::Member);
                self.0.member_reads.fetch_add(1, Ordering::SeqCst);
                let descriptor = descriptor();
                let observation = Observation::builder(
                    descriptor.collection().clone(),
                    Revision::new("r1").unwrap(),
                    knowledge::content_digest(TEXT),
                    chrono::Utc::now(),
                )
                .build(&descriptor)
                .unwrap();
                knowledge::server::member_result(
                    &ResourceUri::new(MEMBER).unwrap(),
                    "text/plain",
                    TEXT.to_owned(),
                    observation,
                    &descriptor,
                    Some(&context.meta),
                )
                .unwrap()
            }
            _ => return Err(ErrorData::invalid_params("unknown fixture resource", None)),
        };
        Ok(result.into())
    }
}
#[derive(Deserialize)]
struct Claims {
    iss: String,
    sub: String,
    aud: String,
    iat: i64,
    nbf: i64,
    exp: i64,
    jti: uuid::Uuid,
}
async fn token(
    State(state): State<Arc<StateData>>,
    Form(form): Form<BTreeMap<String, String>>,
) -> Response {
    assert_eq!(form["grant_type"], "client_credentials");
    assert_eq!(form["client_id"], "operator-service");
    assert_eq!(form["resource"], state.resource);
    assert_eq!(
        form["client_assertion_type"],
        "urn:ietf:params:oauth:client-assertion-type:jwt-bearer"
    );
    assert!(!form["work_context"].is_empty());
    let jwt = &form["client_assertion"];
    let header = jsonwebtoken::decode_header(jwt).unwrap();
    assert_eq!(header.kid.as_deref(), Some("indexing-test"));
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::EdDSA);
    validation.set_audience(&[&state.audience]);
    validation.set_issuer(&["operator-service"]);
    let claims = jsonwebtoken::decode::<Claims>(
        jwt,
        &jsonwebtoken::DecodingKey::from_ed_der(&state.public_key),
        &validation,
    )
    .unwrap()
    .claims;
    assert_eq!(claims.iss, claims.sub);
    assert_eq!(claims.aud, state.audience);
    assert_eq!(claims.iat, claims.nbf);
    assert_eq!(claims.exp - claims.iat, 120);
    assert!(state.assertions.lock().unwrap().insert(claims.jti));
    let round = state.tokens.fetch_add(1, Ordering::SeqCst) + 1;
    state.minted.notify_one();
    match state.bad_token.load(Ordering::SeqCst) {
        1 => return axum::Json(serde_json::json!({"access_token":"wrong-scope", "token_type":"Bearer", "expires_in":60, "scope":"unexpected:scope"})).into_response(),
        2 => return axum::Json(serde_json::json!({"access_token":"unbounded", "token_type":"Bearer", "expires_in":86_401})).into_response(),
        3 => return axum::Json(serde_json::json!({"access_token":"", "token_type":"Bearer", "expires_in":60})).into_response(),
        4 => return axum::response::Redirect::temporary("/credential-redirect-must-not-be-followed").into_response(),
        _ => {},
    }
    axum::Json(serde_json::json!({"access_token": format!("fixture-token-{round}"), "token_type":"Bearer", "expires_in": if round==1 {8} else {60}, "scope":form["scope"]})).into_response()
}
async fn authorize(State(state): State<Arc<StateData>>, request: Request, next: Next) -> Response {
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let valid = (1..=state.tokens.load(Ordering::SeqCst))
        .any(|round| bearer == format!("Bearer fixture-token-{round}"));
    if !valid {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    state.used_tokens.lock().unwrap().insert(bearer);
    next.run(request).await
}

struct KeyFile(std::path::PathBuf);
impl Drop for KeyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
async fn wait_ready(state: &mut watch::Receiver<CoordinatorState>) -> GenerationId {
    loop {
        if let CoordinatorState::Ready(generation) = *state.borrow_and_update() {
            return generation;
        }
        state.changed().await.unwrap();
    }
}

#[tokio::test]
async fn machine_connection_rotates_reconciles_current_catalog_and_releases_its_lease() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
        let db = fixture::TestDb::new().await;
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let file = KeyFile(
            std::env::temp_dir().join(format!("knowledge-indexing-{}.pem", uuid::Uuid::now_v7())),
        );
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&file.0)
            .unwrap()
            .write_all(key.serialize_pem().as_bytes())
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let resource = format!("http://{address}/mcp/knowledge-indexing");
        let audience = format!("http://{address}/oauth/token");
        let state = Arc::new(StateData {
            public_key: key.public_key_raw().to_vec(),
            audience: audience.clone(),
            resource: resource.clone(),
            assertions: Mutex::default(),
            tokens: AtomicUsize::new(0),
            minted: Notify::new(),
            hub: SubscriptionHub::new(),
            member_reads: AtomicUsize::new(0),
            used_tokens: Mutex::default(),
            bad_token: AtomicU8::new(0),
            redirected_requests: AtomicUsize::new(0),
            disconnect: tokio::sync::broadcast::channel(4).0,
        });
        let mut plane: GatewayControlPlane = control_fixture::plane();
        let profile = plane
            .profiles
            .iter_mut()
            .find(|profile| profile.id.as_str() == "knowledge-indexing")
            .unwrap();
        profile.protected_resource = ProtectedResourceId::new(&resource).unwrap();
        plane
            .oauth_clients
            .iter_mut()
            .find(|client| client.id.as_str() == "operator-service")
            .unwrap()
            .allowed_resources = [profile.protected_resource.clone()].into();
        plane.authorization_servers[0].token_endpoint = OAuthEndpointUrl::new(audience).unwrap();
        plane
            .validate(&veoveo_gateway_catalog::registry().unwrap())
            .unwrap();
        hosted::install(&db.a, &plane).await;
        let stop = CancellationToken::new();
        let _stop_guard = stop.clone().drop_guard();
        let gateway = Gateway(state.clone());
        let service = rmcp::transport::streamable_http_server::StreamableHttpService::new(
            move || Ok(gateway.clone()),
            veoveo_mcp_contract::stateless_session_manager(),
            veoveo_mcp_contract::canonical_streamable_http_server_config()
                .with_allowed_hosts([address.to_string()])
                .with_cancellation_token(stop.child_token()),
        );
        let router = Router::new()
            .nest(
                "/mcp/knowledge-indexing",
                Router::new()
                    .route_service("/", service)
                    .layer(middleware::from_fn_with_state(state.clone(), authorize)),
            )
            .route("/oauth/token", post(token))
            .route(
                "/credential-redirect-must-not-be-followed",
                post(|State(state): State<Arc<StateData>>| async move {
                    state.redirected_requests.fetch_add(1, Ordering::SeqCst);
                    axum::http::StatusCode::BAD_REQUEST
                }),
            )
            .with_state(state.clone());
        let shutdown = stop.clone();
        let http = AbortOnDropHandle::new(tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        }));
        let config = IndexingConfig {
            tenant: "tenant-a".parse().unwrap(),
            client_id: "operator-service".parse().unwrap(),
            key_id: "indexing-test".parse().unwrap(),
            signing_algorithm: SigningAlgorithm::EdDsa,
            private_key_file: file.0.clone(),
            trusted_ca_file: None,
            chunk_settings: ChunkSettings::new("structure-v1", 500, 0).unwrap(),
            query_task: veoveo_embedding_contract::EmbeddingTask::new("Find relevant passages")
                .unwrap(),
        };
        let embeddings = Arc::new(embedding::SyntheticEmbeddings::new());
        let (status, mut states) = watch::channel(CoordinatorState::Starting);
        let readiness = IndexingReadiness::new(vec![states.clone()]).unwrap();
        assert!(!readiness.is_ready());
        let worker_stop = CancellationToken::new();
        let _worker_guard = worker_stop.clone().drop_guard();
        let worker_store = db.b.clone();
        let worker_embeddings = embeddings.clone();
        let worker_cancel = worker_stop.clone();
        let worker_config = config.clone();
        let worker = AbortOnDropHandle::new(tokio::spawn(async move {
            IndexingService {
                catalog_registry: &veoveo_gateway_catalog::registry().expect("catalog recipe"),
                store: &worker_store,
                embeddings: worker_embeddings.as_ref(),
                config: &worker_config,
            }
            .run(worker_cancel, status)
            .await
        }));
        let generation = wait_ready(&mut states).await;
        assert!(readiness.is_ready());
        assert_eq!(
            db.a.active_knowledge_generation(&"tenant-a".parse().unwrap())
                .await
                .unwrap(),
            Some(generation)
        );
        while state.tokens.load(Ordering::SeqCst) < 2 {
            state.minted.notified().await;
        }
        // Wait for this new connection's revalidation, not the preceding Ready state.
        loop {
            states.changed().await.unwrap();
            if matches!(*states.borrow_and_update(), CoordinatorState::Ready(_)) {
                break;
            }
        }
        assert_eq!(wait_ready(&mut states).await, generation);
        assert!(
            state
                .used_tokens
                .lock()
                .unwrap()
                .contains("Bearer fixture-token-2")
        );
        assert!(state.member_reads.load(Ordering::SeqCst) >= 2);
        assert_eq!(
            embeddings.inputs.lock().unwrap().len(),
            1,
            "unchanged token rotation reuses existing vectors"
        );
        state.disconnect.send(()).unwrap();
        loop {
            states.changed().await.unwrap();
            if *states.borrow_and_update() == CoordinatorState::Failed {
                break;
            }
        }
        assert!(!readiness.is_ready());
        assert_eq!(
            wait_ready(&mut states).await,
            generation,
            "source reconnect reuses the active generation"
        );
        plane
            .profiles
            .iter_mut()
            .find(|profile| profile.id.as_str() == "knowledge-indexing")
            .unwrap()
            .metadata = serde_json::json!({"display":"new caption"});
        hosted::install(&db.a, &plane).await;
        loop {
            states.changed().await.unwrap();
            if matches!(*states.borrow_and_update(), CoordinatorState::Ready(_)) {
                break;
            }
        }
        assert_eq!(
            wait_ready(&mut states).await,
            generation,
            "unrelated control publication must preserve the content generation"
        );
        assert_eq!(embeddings.inputs.lock().unwrap().len(), 1);
        plane.servers[0].knowledge[0].mode = CollectionApproval::CatalogOnly;
        hosted::install(&db.a, &plane).await;
        loop {
            states.changed().await.unwrap();
            if *states.borrow_and_update() == CoordinatorState::CatalogReady {
                break;
            }
        }
        assert!(readiness.is_ready());
        let registration =
            db.a.knowledge_collection(
                &"tenant-a".parse().unwrap(),
                &"media.records".parse().unwrap(),
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(registration.approval.mode, CollectionApproval::CatalogOnly);
        worker_stop.cancel();
        worker.await.unwrap().unwrap();
        assert!(!readiness.is_ready());
        let lease =
            db.a.claim_knowledge_coordinator(
                &"tenant-a".parse().unwrap(),
                veoveo_platform_store::knowledge::CoordinatorId::new(),
            )
            .await
            .unwrap()
            .expect("departing worker releases lease");
        db.a.release_knowledge_coordinator(&lease).await.unwrap();
        for bad in 1..=4 {
            state.bad_token.store(bad, Ordering::SeqCst);
            let used = state.used_tokens.lock().unwrap().len();
            let (status, mut state_rx) = watch::channel(CoordinatorState::Starting);
            let stop = CancellationToken::new();
            let _guard = stop.clone().drop_guard();
            let store = db.b.clone();
            let embeddings = embeddings.clone();
            let config = config.clone();
            let worker_stop = stop.clone();
            let worker = AbortOnDropHandle::new(tokio::spawn(async move {
                IndexingService {
                    catalog_registry: &veoveo_gateway_catalog::registry().expect("catalog recipe"),
                    store: &store,
                    embeddings: embeddings.as_ref(),
                    config: &config,
                }
                .run(worker_stop, status)
                .await
            }));
            loop {
                state_rx.changed().await.unwrap();
                if *state_rx.borrow_and_update() == CoordinatorState::Failed {
                    break;
                }
            }
            assert_eq!(
                state.used_tokens.lock().unwrap().len(),
                used,
                "invalid token response cannot reach MCP"
            );
            stop.cancel();
            worker.await.unwrap().unwrap();
        }
        assert_eq!(state.redirected_requests.load(Ordering::SeqCst), 0);
        stop.cancel();
        http.await.unwrap();
    })
    .await
    .expect("machine indexing integration exceeded 120 seconds");
}
