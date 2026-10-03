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
        let db=fixture::TestDb::new().await;
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
        db.a.client().query("UPDATE knowledge_collection SET document.sourceContractRevision = 'invalid' WHERE collection = 'media.private';").await.unwrap().check().unwrap();
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
        let db = fixture::TestDb::new().await;
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
                .query("UPDATE $record SET enabled=false;")
                .bind(("record", record.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(client.list_tools(None).await.is_err());
            db.a.client()
                .query("UPDATE $record SET enabled=true;")
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
        let db = fixture::TestDb::new().await;
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
            principal: veoveo_platform_store::OpenObject::new(
                [(
                    "principal".into(),
                    serde_json::to_value(&identity.actor).unwrap(),
                )]
                .into(),
            ),
            current_generation: 1,
            issued_at: Utc::now(),
            expires_at: identity.expires_at,
            revoked_at: None,
            revocation_reason: None,
        };
        db.a.client()
            .query("CREATE ONLY $id CONTENT $record;")
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
            .query("UPDATE $family SET revoked_at=time::now();")
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
