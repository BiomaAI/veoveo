//! Native HTTP, real JWT and SQL authorization tests. Synthetic vectors are not GPU evidence.
#[path = "support/catalog.rs"]
mod catalog;
#[path = "support/conformance.rs"]
mod conformance;
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/hosted.rs"]
mod hosted;
#[path = "support/indexing.rs"]
#[allow(dead_code)]
mod indexing;
#[path = "support/installed.rs"]
mod installed;
#[path = "support/managed.rs"]
mod managed;
use hosted::*;
use indexing::*;
use rmcp::model::{CallToolRequestParams, ReadResourceRequestParams};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use veoveo_embedding_contract::*;
use veoveo_knowledge_mcp::{ServiceError, contract::*, embed::Embeddings, index::Indexer};
use veoveo_mcp_knowledge_extension::*;
use veoveo_types::{ResourceAddress, ResourceUri, ScopeDefinition};

fn collection(name: &str) -> CollectionRegistration {
    let mut r = registration(name, IndexingMode::Content);
    r.tenant = "tenant-a".parse().unwrap();
    r.descriptor = CollectionDescriptor::new(
        format!("media.{name}").parse().unwrap(),
        "finding".parse().unwrap(),
        veoveo_types::ResourceTemplateUri::new(format!("media://{name}{{?cursor}}")).unwrap(),
        Freshness::max_age(300),
        ChangeSignal::Listen,
        AccessModel::WorkContext,
        IndexingMode::Content,
    )
    .unwrap()
    .with_required_scopes(["operator:use".parse().unwrap()]);
    r.approval.collection = r.descriptor.collection().clone();
    r.approval.data_labels.clear();
    r
}
fn input(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    CallToolRequestParams::new(name.to_owned()).with_arguments(args.as_object().unwrap().clone())
}

#[tokio::test]
async fn hosted_search_links_catalog_and_embedding_follow_current_sql_authority() {
    tokio::time::timeout(Duration::from_secs(180),async {
        let db=native_database().await;
        let content=collection("records");
        let lease = db.a.claim_knowledge_coordinator(&content.tenant, veoveo_platform_store::knowledge::CoordinatorId::new()).await.unwrap().unwrap();
        let mut hidden=collection("private");
        hidden.descriptor=hidden.descriptor.with_required_scopes(["media:private".parse().unwrap()]);
        let registrations=vec![content.clone(),hidden.clone()];
        let plane=plane(&registrations); install(&db.a,&plane).await;
        for r in &registrations { db.a.register_knowledge_collection(r,None).await.unwrap(); }
        let signing=Signing::new(); let identity=identity(&plane);directory(&db.a,&identity).await;
        let embedding=Arc::new(SyntheticEmbeddings::new());
        let member=ResourceUri::new("media://members/flood-inspection").unwrap();
        let source=Source(Mutex::new(BTreeMap::from([(member.clone(),record(&content,"Flood inspection identifies safe access routes"))])));
        let spec=GenerationSpec::new(embedding.space().clone(),"Find passages",ChunkSettings::new("structure-v1",500,0).unwrap(),registrations.iter().map(|r|(r.descriptor.collection().clone(),r.revision())).collect()).unwrap();
        let generation=Indexer { lease: &lease,store:&db.a, source:&source,embeddings:embedding.as_ref()}.build(&content.tenant,&registrations,&spec).await.unwrap();
        db.a.activate_knowledge_generation(&lease, &content.tenant,generation,None).await.unwrap();
        // Approval and scope predicates must exclude an undecodable hidden row.
        db.a.client().query(include_str!("queries/http/hosted_search_links_catalog_and_embedding_follow_current_sql_authority.surql")).await.unwrap().check().unwrap();
        let server=Server::new(db.b.clone(),embedding,&signing).await;
        let health = format!("{}/readyz", server.base);
        assert_eq!(reqwest::get(&health).await.unwrap().status(), reqwest::StatusCode::OK);
        server.indexing.send_replace(veoveo_knowledge_mcp::coordinator::CoordinatorState::Starting);
        assert_eq!(reqwest::get(&health).await.unwrap().status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(reqwest::get(format!("{}/healthz", server.base)).await.unwrap().status(), reqwest::StatusCode::OK);
        server.indexing.send_replace(veoveo_knowledge_mcp::coordinator::CoordinatorState::Updating(generation));
        assert_eq!(reqwest::get(&health).await.unwrap().status(), reqwest::StatusCode::OK);
        server.indexing.send_replace(veoveo_knowledge_mcp::coordinator::CoordinatorState::CatalogReady);
        let mut client=server.sdk(signing.issue(identity.clone()).bearer_token).await;
        let list=client.list_tools(None).await.unwrap();assert_eq!(list.tools.len(),2);
        for tool in &list.tools {
            veoveo_mcp_conformance::validate_tool_input_schema(tool).unwrap();
            let malformed = client.call_tool(input(&tool.name, serde_json::json!({}))).await.unwrap();
            assert_eq!(malformed.is_error, Some(true), "typed arguments required: {malformed:?}");
        }
        for (name, mut arguments) in [
            ("search", serde_json::json!({"query":"flood","limit":5})),
            ("embed", serde_json::json!({"mode":"query","task":"Find passages","texts":["flood","access"]})),
        ] {
            match name {
                "search" => { let _: SearchRequest = serde_json::from_value(arguments.clone()).unwrap(); }
                "embed" => { let _: EmbedRequest = serde_json::from_value(arguments.clone()).unwrap(); }
                _ => unreachable!(),
            }
            arguments["undeclared"] = true.into();
            let response = client.call_tool_once(input(name, arguments)).await.unwrap();
            let rmcp::model::CallToolResponse::Complete(malformed) = response else { panic!("malformed input must complete"); };
            assert_eq!(malformed.is_error, Some(true));
            assert!(serde_json::to_string(&malformed.content).unwrap().contains("undeclared"));
        }
        let sources=client.read_resource(ReadResourceRequestParams::new(KnowledgeResource::Sources {after:None}.to_uri().unwrap().to_string())).await.unwrap();
        let body=serde_json::to_string(&sources).unwrap();assert!(body.contains("media.records"));assert!(!body.contains("media.private"));
        let exact=client.read_resource(ReadResourceRequestParams::new(KnowledgeResource::Collection(content.descriptor.collection().clone()).to_uri().unwrap().to_string())).await.unwrap();
        assert!(serde_json::to_string(&exact).unwrap().contains(&generation.to_string()));
        assert!(client.read_resource(ReadResourceRequestParams::new(KnowledgeResource::Collection(hidden.descriptor.collection().clone()).to_uri().unwrap().to_string())).await.is_err());
        let result=client.call_tool(input("search",serde_json::json!({"query":"flood","limit":5}))).await.unwrap();
        let search:SearchResponse=serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(search.results.len(),1);assert_eq!(search.results[0].uri,member);
        assert!(serde_json::to_string(&result.content).unwrap().contains("resource_link"));
        let embedded=client.call_tool(input("embed",serde_json::json!({"mode":"query","task":"Find passages","texts":["flood","access"]}))).await.unwrap();
        assert_eq!(embedded.structured_content.unwrap()["vectors"].as_array().unwrap().len(),2);
        let replica = Server::new(db.a.clone(), Arc::new(SyntheticEmbeddings::new()), &signing).await;
        let mut replica_client = replica.sdk(signing.issue(identity.clone()).bearer_token).await;
        let replay = replica_client.call_tool(input("search", serde_json::json!({"query":"flood","limit":5}))).await.unwrap();
        let replay: SearchResponse = serde_json::from_value(replay.structured_content.unwrap()).unwrap();
        assert_eq!(replay.results[0].uri, member, "another replica reads the active persisted index");
        replica_client.close().await.unwrap();
        // A current control-plane scope reduction invalidates the still-signed token.
        let mut revoked=plane.clone();
        revoked.oauth_clients.iter_mut().find(|c| c.id.as_str()=="operator-service").unwrap().allowed_scopes.remove(KnowledgeScope::Search.name());
        install(&db.a,&revoked).await;
        assert!(client.call_tool(input("search",serde_json::json!({"query":"flood","limit":5}))).await.is_err());
        client.close().await.unwrap();
    }).await.expect("hosted retrieval exceeded 180 seconds");
}

struct BlockingEmbeddings {
    inner: SyntheticEmbeddings,
    entered: tokio::sync::Semaphore,
    release: tokio::sync::Semaphore,
}
impl Embeddings for BlockingEmbeddings {
    fn space(&self) -> &EmbeddingSpace {
        self.inner.space()
    }
    async fn documents(&self, batch: EmbeddingBatch) -> Result<Vec<EmbeddingVector>, ServiceError> {
        self.entered.add_permits(1);
        self.release.acquire().await.unwrap().forget();
        self.inner.documents(batch).await
    }
    async fn queries(
        &self,
        _: EmbeddingTask,
        batch: EmbeddingBatch,
    ) -> Result<Vec<EmbeddingVector>, ServiceError> {
        self.documents(batch).await
    }
    async fn query(
        &self,
        task: EmbeddingTask,
        text: EmbeddingText,
    ) -> Result<EmbeddingVector, ServiceError> {
        self.inner.query(task, text).await
    }
}
#[tokio::test]
async fn delivery_rechecks_revocation_and_disabled_directory_identity() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = native_database().await;
        let plane = plane(&[]);
        install(&db.a, &plane).await;
        let identity = identity(&plane);
        directory(&db.a, &identity).await;
        let signing = Signing::new();
        let embedding = Arc::new(BlockingEmbeddings {
            inner: SyntheticEmbeddings::new(),
            entered: tokio::sync::Semaphore::new(0),
            release: tokio::sync::Semaphore::new(0),
        });
        let server = Server::new(db.b.clone(), embedding.clone(), &signing).await;
        let mut client = server
            .sdk(signing.issue(identity.clone()).bearer_token)
            .await;
        let peer = client.peer().clone();
        let pending = tokio::spawn(async move {
            peer.call_tool(input(
                "embed",
                serde_json::json!({"mode":"document","texts":["must not be delivered"]}),
            ))
            .await
        });
        tokio::time::timeout(Duration::from_secs(10), embedding.entered.acquire())
            .await
            .unwrap()
            .unwrap()
            .forget();
        revoke(&db.a, &identity).await;
        embedding.release.add_permits(1);
        assert!(
            pending.await.unwrap().is_err(),
            "revoked request cannot deliver computed vectors"
        );
        client.close().await.unwrap();
        let mut fresh = identity.clone();
        fresh.request_context.as_mut().unwrap().access_token.jwt_id =
            Some("fresh-token".parse().unwrap());
        let mut client = server.sdk(signing.issue(fresh.clone()).bearer_token).await;
        assert!(client.list_tools(None).await.is_ok());
        let principal = veoveo_platform_store::deterministic_principal_id(
            fresh.authority.tenant.as_str(),
            fresh.actor.id.as_str(),
        )
        .unwrap()
        .record_id();
        for record in [
            principal,
            veoveo_platform_store::deterministic_tenant_id(fresh.authority.tenant.as_str())
                .unwrap()
                .record_id(),
            veoveo_platform_store::deterministic_enterprise_id().record_id(),
        ] {
            db.a.client()
                .query(include_str!("queries/http/delivery_rechecks_revocation_and_disabled_directory_identity.surql"))
                .bind(("record", record.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(client.list_tools(None).await.is_err());
            db.a.client()
                .query(include_str!("queries/http/delivery_rechecks_revocation_and_disabled_directory_identity_2.surql"))
                .bind(("record", record))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(client.list_tools(None).await.is_ok());
        }
        client.close().await.unwrap();
        let response = reqwest::Client::new()
            .post(format!("{}/mcp", server.base))
            .bearer_auth("invalid-signature")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
    })
    .await
    .expect("hosted revocation exceeded 180 seconds");
}

#[tokio::test]
async fn browser_session_revocation_and_scope_and_time_boundaries_are_current() {
    tokio::time::timeout(Duration::from_secs(180), async {
        use chrono::{TimeDelta, Utc};
        use veoveo_mcp_contract::PrincipalKind;
        use veoveo_types::{InvocationMode, InvocationProvenance};
        let db = native_database().await;
        let plane = plane(&[]);
        install(&db.a, &plane).await;
        let mut identity = identity(&plane);
        identity.actor.kind = PrincipalKind::User;
        identity.actor.id = "https://veoveo.example/oauth#reader".parse().unwrap();
        identity.actor.subject = "reader".parse().unwrap();
        identity.authority.provenance = InvocationProvenance::Direct {
            initiator: identity.actor.id.clone(),
        };
        let request = identity.request_context.as_mut().unwrap();
        request.principal = identity.actor.clone();
        request.access_token.subject = identity.actor.subject.clone();
        request.access_token.oauth_client_id = "operator-local-public".parse().unwrap();
        request.access_token.invocation_mode = InvocationMode::Direct;
        request.access_token.initiator = Some(identity.actor.id.clone());
        let family = uuid::Uuid::now_v7();
        request.access_token.session_family = Some(family.to_string().parse().unwrap());
        let record = veoveo_platform_store::GatewayRefreshFamilyRecord {
            id: veoveo_platform_store::gateway_refresh_family_record_id(family),
            authorization_server: plane.authorization_servers[0].id.to_string(),
            profile: identity.profile.to_string(),
            oauth_client_id: request.access_token.oauth_client_id.to_string(),
            work_context: identity.authority.work_context.to_string(),
            principal_id: identity.actor.id.to_string(),
            tenant: Some(identity.authority.tenant.to_string()),
            scopes: identity
                .actor
                .scopes
                .iter()
                .map(ToString::to_string)
                .collect(),
            principal: serde_json::from_value(serde_json::json!({
                "principal": &identity.actor,
                "principal_display_name": "Reader",
            }))
            .unwrap(),
            current_generation: 1,
            issued_at: Utc::now(),
            expires_at: identity.expires_at,
            revoked_at: None,
            revocation_reason: None,
        };
        db.a.client()
            .query(include_str!("queries/http/browser_session_revocation_and_scope_and_time_boundaries_are_current.surql"))
            .bind(("id", record.id.clone()))
            .bind(("record", record))
            .await
            .unwrap()
            .check()
            .unwrap();
        directory(&db.a, &identity).await;
        let signing = Signing::new();
        let server =
            Server::new(db.b.clone(), Arc::new(SyntheticEmbeddings::new()), &signing).await;
        let bearer = signing.issue(identity.clone()).bearer_token;
        let mut client = server.sdk(bearer.clone()).await;
        assert_eq!(client.list_tools(None).await.unwrap().tools.len(), 2);
        let http = reqwest::Client::new();
        assert_eq!(
            http.get(format!("{}/admin/docs/llms.txt", server.base))
                .bearer_auth(&bearer)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        let mut limited = identity.clone();
        limited.actor.scopes.remove(KnowledgeScope::Embed.name());
        limited.request_context.as_mut().unwrap().principal = limited.actor.clone();
        limited
            .request_context
            .as_mut()
            .unwrap()
            .access_token
            .scopes = limited.actor.scopes.clone();
        let mut limited_client = server.sdk(signing.issue(limited).bearer_token).await;
        assert_eq!(
            limited_client.list_tools(None).await.unwrap().tools.len(),
            1
        );
        assert!(
            limited_client
                .call_tool(input(
                    "embed",
                    serde_json::json!({"mode":"document","texts":["denied"]})
                ))
                .await
                .is_err()
        );
        limited_client.close().await.unwrap();
        let mut early = identity.clone();
        early
            .request_context
            .as_mut()
            .unwrap()
            .access_token
            .not_before = Some(Utc::now() + TimeDelta::minutes(1));
        let mut early_client = server.sdk(signing.issue(early).bearer_token).await;
        assert!(early_client.list_tools(None).await.is_err());
        early_client.close().await.unwrap();
        db.a.client()
            .query(include_str!("queries/http/browser_session_revocation_and_scope_and_time_boundaries_are_current_2.surql"))
            .bind((
                "family",
                veoveo_platform_store::gateway_refresh_family_record_id(family),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(client.list_tools(None).await.is_err());
        client.close().await.unwrap();
        assert_eq!(
            http.get(format!("{}/admin/docs/llms.txt", server.base))
                .bearer_auth(bearer)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    })
    .await
    .expect("browser session test exceeded 180 seconds");
}

#[tokio::test]
async fn managed_execution_requires_signed_attribution_and_current_registration() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = managed_native_database().await;
        let mut plane = plane(&[]);
        let mut identity = identity(&plane);
        let (_, definition, instance) = managed::provision(&db.a, &plane, &identity).await;
        identity.actor.roles = ["managed-pilot".parse().unwrap()].into();
        let request = identity.request_context.as_mut().unwrap();
        request.principal.roles = identity.actor.roles.clone();
        request.access_token.managed_execution =
            Some(veoveo_mcp_contract::audit::AuditManagedExecution {
                instance: "one".parse().unwrap(),
                generation: u64::try_from(instance.active_generation)
                    .unwrap()
                    .try_into()
                    .unwrap(),
                dispatch_epoch: u64::try_from(instance.dispatch_epoch)
                    .unwrap()
                    .try_into()
                    .unwrap(),
                episode: None,
            });
        // A static/dynamic collision is refused even for a well-signed attribution.
        install(&db.a, &plane).await;
        let target = veoveo_mcp_contract::PolicyTarget::Tool {
            server: "knowledge".parse().unwrap(),
            tool: "search".parse().unwrap(),
        };
        async fn admit(
            store: &veoveo_platform_store::PlatformStore,
            identity: &veoveo_mcp_contract::GatewayInternalIdentity,
            target: &veoveo_mcp_contract::PolicyTarget,
        ) -> bool {
            veoveo_knowledge_mcp::authority::authorize(
                store,
                &veoveo_gateway_catalog::registry().unwrap(),
                &veoveo_agent_runtime::internal_clients::ManagedInternalClientAuthorityResolver::new(store.clone()),
                identity,
                KnowledgeScope::Search,
                veoveo_gateway_contract::GatewayAction::ToolsCall,
                target,
            )
            .await
            .is_ok()
        }
        assert!(!admit(&db.a, &identity, &target).await);
        plane
            .oauth_clients
            .retain(|client| client.id.as_str() != "operator-service");
        install(&db.a, &plane).await;
        let signing = Signing::new();
        let issued = signing.issue(identity.clone());
        let verifier = veoveo_mcp_contract::GatewayInternalTokenVerifier::new(
            veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER
                .parse()
                .unwrap(),
            "knowledge".parse().unwrap(),
            signing.trust.clone(),
        );
        let identity = verifier.verify(&issued.bearer_token).unwrap();
        assert!(admit(&db.a, &identity, &target).await);
        assert!(veoveo_knowledge_mcp::authority::authorize(&db.a, &veoveo_gateway_catalog::registry().unwrap(), &veoveo_policy::internal_clients::StaticInternalClientAuthorityResolver, &identity, KnowledgeScope::Search, veoveo_gateway_contract::GatewayAction::ToolsCall, &target).await.is_err(), "kernel-only resolver rejects managed attribution");
        let tables = veoveo_policy::internal_clients::InternalClientAuthorityResolver::observation_tables(&veoveo_agent_runtime::internal_clients::ManagedInternalClientAuthorityResolver::new(db.a.clone()));
        assert_eq!(tables.iter().map(|table| table.as_str()).collect::<Vec<_>>(), ["managed_agent", "agent_definition"]);
        use std::sync::atomic::{AtomicU64, Ordering};
        use veoveo_policy::internal_clients::{
            AuthorityFuture, InternalClientAuthorityRequest, InternalClientAuthorityResolver,
        };
        struct RejectionProbe {
            delegate: veoveo_agent_runtime::internal_clients::ManagedInternalClientAuthorityResolver,
            started: AtomicU64,
            completed_rejections: tokio::sync::mpsc::Sender<u64>,
        }
        impl InternalClientAuthorityResolver for RejectionProbe {
            fn resolve<'a>(&'a self, request: InternalClientAuthorityRequest<'a>) -> AuthorityFuture<'a> {
                // A call that started before the mutation cannot satisfy its wake proof,
                // even when its rejected result completes after the mutation.
                let sequence = self.started.fetch_add(1, Ordering::SeqCst) + 1;
                Box::pin(async move {
                    let result = self.delegate.resolve(request).await;
                    if result.is_err() {
                        let _ = self.completed_rejections.send(sequence).await;
                    }
                    result
                })
            }
            fn observation_tables(&self) -> Vec<veoveo_modules::ObservationTable> {
                self.delegate.observation_tables()
            }
        }
        async fn fresh_rejection(receiver: &mut tokio::sync::mpsc::Receiver<u64>, after: u64) {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let sequence = receiver.recv().await.expect("hosted resolver probe is alive");
                    if sequence > after {
                        break;
                    }
                }
            })
            .await
            .expect("owner mutation did not cause a completed hosted authority rejection");
        }
        let (completed_rejections, mut rejections) = tokio::sync::mpsc::channel(8);
        let probe = Arc::new(RejectionProbe {
            delegate: veoveo_agent_runtime::internal_clients::ManagedInternalClientAuthorityResolver::new(db.b.clone()),
            started: AtomicU64::new(0),
            completed_rejections,
        });
        let server = Server::with_authority(db.b.clone(), Arc::new(SyntheticEmbeddings::new()), &signing, probe.clone()).await;
        let mut client = server.sdk(signing.issue(identity.clone()).bearer_token).await;
        let filter = rmcp::model::SubscriptionFilter::builder().resources_list_changed().build();
        let mut definition_listener = client.listen(filter.clone()).await.unwrap();
        assert!(matches!(tokio::time::timeout(Duration::from_secs(10), definition_listener.next()).await.unwrap().unwrap(), Some(rmcp::model::ServerNotification::ResourceListChangedNotification(_))));
        let before_definition_mutation = probe.started.load(Ordering::SeqCst);
        db.a.client().query(include_str!("queries/http/definition_disabled.surql")).bind(("definition", definition.id.clone())).bind(("disabled", true)).await.unwrap().check().unwrap();
        fresh_rejection(&mut rejections, before_definition_mutation).await;
        assert!(tokio::time::timeout(Duration::from_secs(10), definition_listener.next()).await.unwrap().is_err(), "managed definition revocation wakes and revalidates the listener");
        drop(definition_listener);
        db.a.client().query(include_str!("queries/http/definition_disabled.surql")).bind(("definition", definition.id.clone())).bind(("disabled", false)).await.unwrap().check().unwrap();
        let mut registration_listener = client.listen(filter).await.unwrap();
        assert!(matches!(tokio::time::timeout(Duration::from_secs(10), registration_listener.next()).await.unwrap().unwrap(), Some(rmcp::model::ServerNotification::ResourceListChangedNotification(_))));
        for mutation in 0..8 {
            let mut changed = identity.clone();
            let request = changed.request_context.as_mut().unwrap();
            match mutation {
                0 => request.access_token.managed_execution = None,
                1 => {
                    request
                        .access_token
                        .managed_execution
                        .as_mut()
                        .unwrap()
                        .generation = std::num::NonZeroU64::new(u64::MAX).unwrap()
                }
                2 => {
                    request
                        .access_token
                        .managed_execution
                        .as_mut()
                        .unwrap()
                        .dispatch_epoch = std::num::NonZeroU64::new(u64::MAX).unwrap()
                }
                3 => {
                    request
                        .access_token
                        .managed_execution
                        .as_mut()
                        .unwrap()
                        .instance = "other".parse().unwrap()
                }
                4 => {
                    request.access_token.session_family =
                        Some(uuid::Uuid::now_v7().to_string().parse().unwrap())
                }
                5 => request.principal.roles.clear(),
                6 => {
                    request
                        .access_token
                        .scopes
                        .insert("ungranted:scope".parse().unwrap());
                }
                _ => request.access_token.issuer = "https://foreign.example/oauth".parse().unwrap(),
            };
            assert!(
                !admit(&db.a, &changed, &target).await,
                "mutation {mutation}"
            );
        }
        let denied_tool = veoveo_mcp_contract::PolicyTarget::Tool {
            server: "knowledge".parse().unwrap(),
            tool: "embed".parse().unwrap(),
        };
        assert!(!admit(&db.a, &identity, &denied_tool).await);
        let before_registration_mutation = probe.started.load(Ordering::SeqCst);
        db.a.client()
            .query(include_str!("queries/http/admit.surql"))
            .bind(("instance", instance.id.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(!admit(&db.a, &identity, &target).await);
        fresh_rejection(&mut rejections, before_registration_mutation).await;
        assert!(tokio::time::timeout(Duration::from_secs(10), registration_listener.next()).await.unwrap().is_err(), "managed dispatch epoch change wakes and revalidates the listener");
        client.close().await.unwrap();
    })
    .await
    .expect("managed receiver authority deadline");
}

#[cfg(feature = "server")]
#[tokio::test]
async fn startup_uses_selected_lanes_and_requires_committed_preparation() {
    tokio::time::timeout(Duration::from_secs(180), async {
        for managed in [false, true] {
            let db = if managed {
                managed_native_database().await
            } else {
                native_database().await
            };
            let optional = if managed {
                vec![
                    veoveo_agent_runtime::schema::module_setup(
                        fixture::module_lanes::execution("agents").unwrap(),
                    )
                    .unwrap(),
                ]
            } else {
                vec![]
            };
            let registry = fixture::module_lanes::registry(optional).unwrap();
            let enabled = if managed {
                vec![veoveo_modules::ModuleName::new("agents").unwrap()]
            } else {
                vec![]
            };
            let selection = veoveo_modules::ModuleSelectionDocument::new(
                enabled.clone(),
                "1".parse().unwrap(),
                veoveo_modules::CredentialRevision::new("fixture-v1").unwrap(),
            )
            .unwrap();
            let plan = veoveo_modules::ModulePlanDocument::generate(
                &registry,
                &selection,
                veoveo_modules::CompositionIdentity::new(format!("sha256:{}", "a".repeat(64)))
                    .unwrap(),
                vec![],
            )
            .unwrap();
            let startup = veoveo_knowledge_mcp::composition::RuntimeInstallation {
                plan: &plan,
                composition: plan.composition(),
                generation: plan.generation(),
                credential_revision: plan.credential_revision(),
                runtime_username: db.a.config().username(),
            };
            assert!(
                startup.authority(&db.a).await.is_err(),
                "missing committed preparation"
            );
            let prepared =
                veoveo_modules::runner::prepare(registry.select(enabled).unwrap()).unwrap();
            let key = veoveo_modules::preparation_key(&plan, db.a.config().username()).unwrap();
            let admin = db.admin().await;
            prepared
                .claim_preparation(admin.client(), &key)
                .await
                .unwrap();
            assert!(
                startup.authority(&db.a).await.is_err(),
                "claimed preparation is incomplete"
            );
            prepared
                .complete_preparation(
                    admin.client(),
                    &key,
                    veoveo_modules::runner::DatabaseEditorCredentials::new(
                        db.a.config().username(),
                        "isolated-fixture-runtime-password",
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
            // Preparation replaces the runtime account; authenticate its new session
            // before inspecting prerequisites rather than reusing the invalidated one.
            let runtime = veoveo_platform_store::PlatformStore::connect(
                veoveo_platform_store::StoreConfig::builder(
                    db.a.config().endpoint().as_str(),
                    db.a.config().namespace(),
                    db.a.config().database(),
                    veoveo_platform_store::StoreCredentials::database(
                        db.a.config().username(),
                        "isolated-fixture-runtime-password",
                    ),
                )
                .audit_targets(Arc::new(db.a.audit_targets().clone()))
                .build()
                .unwrap(),
            )
            .await
            .unwrap();
            if managed && !cfg!(feature = "managed-clients") {
                let Err(error) = startup.authority(&runtime).await else {
                    panic!("selected managed adapter must be unavailable");
                };
                assert!(
                    error
                        .to_string()
                        .contains("requires the Knowledge managed-clients adapter"),
                    "selected adapter must fail with the configuration diagnostic"
                );
                continue;
            }
            let adapter = startup.authority(&runtime).await.unwrap();
            assert_eq!(
                adapter.observation_tables().len(),
                if managed { 2 } else { 0 }
            );
            let wrong_composition =
                veoveo_modules::CompositionIdentity::new(format!("sha256:{}", "b".repeat(64)))
                    .unwrap();
            assert!(
                veoveo_knowledge_mcp::composition::RuntimeInstallation {
                    composition: &wrong_composition,
                    ..startup
                }
                .authority(&runtime)
                .await
                .is_err()
            );
            assert!(
                veoveo_knowledge_mcp::composition::RuntimeInstallation {
                    generation: "2".parse().unwrap(),
                    ..startup
                }
                .authority(&runtime)
                .await
                .is_err()
            );
            let wrong_revision =
                veoveo_modules::CredentialRevision::new("different-rotation").unwrap();
            assert!(
                veoveo_knowledge_mcp::composition::RuntimeInstallation {
                    credential_revision: &wrong_revision,
                    ..startup
                }
                .authority(&runtime)
                .await
                .is_err()
            );
            let wrong = veoveo_knowledge_mcp::composition::RuntimeInstallation {
                runtime_username: "different-runtime",
                ..startup
            };
            assert!(wrong.authority(&runtime).await.is_err());
        }
    })
    .await
    .expect("startup plan qualification exceeded 180 seconds");
}
