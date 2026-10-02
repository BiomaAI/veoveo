use super::*;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{Implementation, ReadResourceResponse, ServerCapabilities, ServerConfig},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use veoveo_audit_contract::{AuditPartition, AuditQuery, AuditReadScope};
use veoveo_mcp_contract::GatewayAction;
use veoveo_types::{ResourceScheme, ResourceUri, ServerSlug};

fn projection() -> GatewayResourceProjection {
    GatewayResourceProjection {
        server: ServerSlug::new("time").unwrap(),
        gateway_uri: ResourceUri::new("time://docs/design").unwrap(),
        upstream_uri: ResourceUri::new("time://docs/design").unwrap(),
    }
}
fn observation() -> Observation {
    let descriptor =
        knowledge::docs::collection(&projection().server, &ResourceScheme::new("time").unwrap());
    Observation::builder(
        descriptor.collection().clone(),
        Revision::new("source-revision").unwrap(),
        knowledge::content_digest("source content"),
        chrono::Utc::now(),
    )
    .build(&descriptor)
    .unwrap()
}
fn result(meta: &rmcp::model::RequestMetaObject) -> ReadResourceResult {
    knowledge::server::member_result(
        &projection().upstream_uri,
        "text/plain",
        "source content".into(),
        observation(),
        &knowledge::docs::collection(&projection().server, &ResourceScheme::new("time").unwrap()),
        Some(meta),
    )
    .unwrap()
}

#[test]
fn source_validation_rejects_forged_bytes_owner_and_unsolicited_observations() {
    let mut meta = rmcp::model::RequestMetaObject::default();
    knowledge::client::declare_read(&mut meta, None);
    let valid = result(&meta);
    assert!(
        validate_source_read(&valid, &projection(), None, true)
            .unwrap()
            .is_some()
    );
    assert!(validate_source_read(&valid, &projection(), None, false).is_err());
    let mut wrong_owner = projection();
    wrong_owner.server = ServerSlug::new("map").unwrap();
    assert!(validate_source_read(&valid, &wrong_owner, None, true).is_err());
    let mut changed = valid;
    changed.contents = vec![rmcp::model::ResourceContents::text(
        "forged",
        projection().upstream_uri.as_str(),
    )];
    assert!(validate_source_read(&changed, &projection(), None, true).is_err());
}

#[test]
fn delivery_preserves_content_and_unknown_metadata_but_observations_require_negotiation() {
    let mut meta = rmcp::model::RequestMetaObject::default();
    knowledge::client::declare_read(&mut meta, None);
    let full = result(&meta);
    for declared in [false, true] {
        let mut delivered = full.clone();
        delivered
            .meta
            .as_mut()
            .unwrap()
            .insert("fixture/metadata".into(), serde_json::json!({"id": 7}));
        prepare_delivery(&mut delivered, &projection(), declared).unwrap();
        assert_eq!(delivered.contents, full.contents);
        assert_eq!(delivered.ttl_ms, Some(0));
        assert_eq!(delivered.cache_scope, Some(CacheScope::Private));
        assert_eq!(
            delivered.meta.as_ref().unwrap().get("fixture/metadata"),
            Some(&serde_json::json!({"id": 7}))
        );
        assert_eq!(
            knowledge::client::observation(&delivered)
                .unwrap()
                .is_some(),
            declared
        );
    }
}

#[derive(Clone)]
struct Source {
    calls: Arc<AtomicUsize>,
    denied: Arc<AtomicBool>,
}
impl ServerHandler for Source {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder().enable_resources().build();
        knowledge::server::declare(&mut capabilities);
        ServerConfig::new(capabilities).with_server_info(Implementation::new("source", "1"))
    }
    async fn read_resource(
        &self,
        _: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert!(knowledge::server::requested(Some(&context.meta)).unwrap());
        if self.denied.load(Ordering::SeqCst) {
            return Err(McpError::invalid_request("fixture authority revoked", None));
        }
        Ok(result(&context.meta).into())
    }
}

#[tokio::test]
async fn negotiated_read_commits_observation_before_delivery_and_fails_when_writer_stops() {
    tokio::time::timeout(Duration::from_secs(180), qualify())
        .await
        .expect("read audit qualification deadline");
}
async fn qualify() {
    let db = crate::test_store::TestDb::new().await;
    let state = crate::GatewayState::new(db.a.clone());
    let gateway = super::super::task_ownership_tests::gateway(
        state.clone(),
        serde_json::from_str(include_str!("../../../../../configs/gateway.local.json")).unwrap(),
    );
    let subject = super::super::task_ownership_tests::subject();
    let calls = Arc::new(AtomicUsize::new(0));
    let denied = Arc::new(AtomicBool::new(false));
    let (client_io, server_io) = tokio::io::duplex(32768);
    let (source, client) = tokio::join!(
        Source {
            calls: calls.clone(),
            denied: denied.clone()
        }
        .serve(server_io),
        ().serve(client_io)
    );
    let source = source.unwrap();
    let client = client.unwrap();
    let params = || ReadResourceRequestParams::new(projection().upstream_uri.to_string());
    let (full, observation) = read_upstream(client.peer().clone(), params(), &projection(), None)
        .await
        .unwrap();
    let observation = observation.unwrap();
    gateway
        .finish_resource_read(
            &subject,
            &projection(),
            None,
            Ok((full, Some(observation.clone()))),
        )
        .await
        .unwrap();
    let scope = AuditReadScope::new(subject.actor.tenant.clone(), false);
    let query = AuditQuery::new(AuditPartition::Tenant(
        subject.actor.tenant.clone().unwrap(),
    ));
    let page = db.b.audit_page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.records[0].draft.outcome(), AuditOutcome::Succeeded);
    assert!(
        matches!(page.records[0].draft.detail(), AuditDetail::KnowledgeRead { observation: Some(recorded), status: KnowledgeReadStatus::Read, .. } if **recorded == (&observation).into())
    );
    let (conditional, observed) = read_upstream(
        client.peer().clone(),
        params(),
        &projection(),
        Some(observation.revision()),
    )
    .await
    .unwrap();
    assert!(conditional.contents.is_empty());
    gateway
        .finish_resource_read(&subject, &projection(), None, Ok((conditional, observed)))
        .await
        .unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "conditional read must reach source authority again"
    );
    let page = db.b.audit_page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 2);
    assert!(matches!(
        page.records[1].draft.detail(),
        AuditDetail::KnowledgeRead {
            status: KnowledgeReadStatus::NotModified,
            ..
        }
    ));
    denied.store(true, Ordering::SeqCst);
    let denial = read_upstream(
        client.peer().clone(),
        params(),
        &projection(),
        Some(observation.revision()),
    )
    .await
    .map_err(crate::mcp_support::upstream_error)
    .unwrap_err();
    assert_eq!(denial.code, rmcp::model::ErrorCode::INVALID_REQUEST);
    let delivered = gateway
        .finish_resource_read(&subject, &projection(), None, Err(denial.clone()))
        .await
        .unwrap_err();
    assert_eq!(delivered, denial);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let page = db.b.audit_page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 3);
    assert_eq!(page.records[2].draft.outcome(), AuditOutcome::Denied);
    gateway
        .finish_resource_read(
            &subject,
            &projection(),
            None,
            Err(McpError::resource_not_found("fixture member missing", None)),
        )
        .await
        .unwrap_err();
    let page = db.b.audit_page(&scope, &query).await.unwrap();
    assert_eq!(page.records.len(), 4);
    assert_eq!(page.records[3].draft.reason(), AuditReason::NotFound);
    denied.store(false, Ordering::SeqCst);
    state
        .audit_writer()
        .await
        .shutdown(Duration::from_secs(5))
        .await
        .unwrap();
    let (full, observed) = read_upstream(client.peer().clone(), params(), &projection(), None)
        .await
        .unwrap();
    let error = gateway
        .finish_resource_read(&subject, &projection(), None, Ok((full, observed)))
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("source content"));
    assert_eq!(
        db.b.audit_page(&scope, &query).await.unwrap().records.len(),
        4
    );
    client.cancel().await.unwrap();
    source.cancel().await.unwrap();
}

#[path = "../../../../../testing/fixtures/knowledge_control.rs"]
mod indexing_fixture;

#[tokio::test]
async fn indexing_gate_binds_approval_enumeration_members_and_revocation() {
    use veoveo_knowledge_contract::CollectionRegistration;
    use veoveo_mcp_knowledge_extension::{
        AccessDescriptor, AccessModel, ChangeSignal, CollectionDescriptor, Freshness,
        INDEXING_READ_KEY, IndexingMode, IndexingReadIntent, IndexingReadKind, ReadPolicy,
    };
    use veoveo_types::{AccessSubject, InvocationMode, InvocationProvenance};
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = crate::test_store::TestDb::new().await;
        let plane = indexing_fixture::plane();
        plane.validate().unwrap();
        let mut gateway = super::super::task_ownership_tests::gateway(
            crate::GatewayState::new(db.a.clone()),
            plane.clone(),
        );
        gateway.profile_id = "knowledge-indexing".parse().unwrap();
        let mut subject = super::super::task_ownership_tests::subject();
        subject.principal.kind = veoveo_mcp_contract::PrincipalKind::Service;
        subject.principal.tenant = Some("tenant-a".parse().unwrap());
        subject.actor = subject.principal.clone();
        subject.access_token.oauth_client_id = "operator-service".parse().unwrap();
        subject.access_token.invocation_mode = InvocationMode::Automated;
        subject.access_token.initiator = None;
        subject.authority.tenant = "tenant-a".parse().unwrap();
        subject.authority.provenance = InvocationProvenance::Automated;
        let catalog = gateway.catalog.current();
        let target = veoveo_mcp_contract::PolicyTarget::Server {
            server: "media".parse().unwrap(),
        };
        assert!(super::super::knowledge_indexing::allows_action(
            &catalog,
            &subject,
            GatewayAction::ResourcesRead,
            &target
        ));
        assert!(!super::super::knowledge_indexing::allows_action(
            &catalog,
            &subject,
            GatewayAction::ToolsCall,
            &target
        ));
        assert!(!super::super::knowledge_indexing::allows_action(
            &catalog,
            &subject,
            GatewayAction::ResourcesRead,
            &veoveo_mcp_contract::PolicyTarget::Server {
                server: "foreign".parse().unwrap()
            }
        ));
        let registration = CollectionRegistration {
            source_contract_revision: 3,
            tenant: subject.authority.tenant.clone(),
            approval: plane.servers[0].knowledge[0].clone(),
            control_revision: veoveo_types::Sha256Digest::from_bytes([1; 32]),
            descriptor: CollectionDescriptor::new(
                "media.records".parse().unwrap(),
                "record".parse().unwrap(),
                veoveo_types::ResourceTemplateUri::new("media://records{?cursor}").unwrap(),
                Freshness::max_age(30),
                ChangeSignal::Listen,
                AccessModel::WorkContext,
                IndexingMode::Content,
            )
            .unwrap(),
        };
        let project = |text: &str| GatewayResourceProjection {
            server: "media".parse().unwrap(),
            gateway_uri: ResourceUri::new(text).unwrap(),
            upstream_uri: ResourceUri::new(text).unwrap(),
        };
        let meta = |kind| {
            let mut meta = rmcp::model::RequestMetaObject::default();
            meta.insert(
                INDEXING_READ_KEY.into(),
                serde_json::to_value(IndexingReadIntent {
                    collection: registration.descriptor.collection().clone(),
                    kind,
                })
                .unwrap(),
            );
            meta
        };
        let contract = project("media://contract");
        let contract_meta = meta(IndexingReadKind::SourceContract);
        let contract_permit = gateway
            .admit_indexing_read(&subject, &contract, &contract_meta)
            .await
            .unwrap()
            .unwrap();
        gateway
            .validate_indexing_delivery(&subject, &contract, &contract_permit, None)
            .await
            .unwrap();
        for uri in [
            "media://private",
            "media://contract?cursor=next",
            "media://contract/extra",
        ] {
            assert!(
                gateway
                    .admit_indexing_read(&subject, &project(uri), &contract_meta)
                    .await
                    .is_err()
            );
        }
        assert!(
            gateway
                .admit_indexing_subscription(&subject, &contract, &contract_meta)
                .await
                .is_err()
        );
        db.a.register_knowledge_collection(&registration, None)
            .await
            .unwrap();
        let enumeration = meta(IndexingReadKind::Enumeration);
        assert!(
            gateway
                .admit_indexing_read(
                    &subject,
                    &project("media://records?cursor=next"),
                    &enumeration
                )
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            gateway
                .admit_indexing_read(&subject, &project("media://private"), &enumeration)
                .await
                .is_err()
        );
        assert!(
            gateway
                .admit_indexing_read(
                    &subject,
                    &project("media://records?extra=secret"),
                    &enumeration
                )
                .await
                .is_err()
        );
        assert!(
            gateway
                .admit_indexing_read(&subject, &project("media://records"), &Default::default())
                .await
                .is_err()
        );
        gateway
            .admit_indexing_subscription(&subject, &project("media://records"), &enumeration)
            .await
            .unwrap();
        gateway
            .admit_indexing_subscription(&subject, &project("media://records"), &Default::default())
            .await
            .unwrap();
        for uri in [
            "media://private",
            "media://records?cursor=next",
            "media://record/one",
        ] {
            assert!(
                gateway
                    .admit_indexing_subscription(&subject, &project(uri), &Default::default())
                    .await
                    .is_err()
            );
        }
        let member = project("media://record/one");
        let member_meta = meta(IndexingReadKind::Member);
        assert!(
            gateway
                .admit_indexing_subscription(&subject, &member, &member_meta)
                .await
                .is_err(),
            "unobserved members cannot be subscribed"
        );
        let permit = gateway
            .admit_indexing_read(&subject, &member, &member_meta)
            .await
            .unwrap()
            .unwrap();
        let observation = |labels: Vec<veoveo_types::DataLabelId>| {
            Observation::builder(
                registration.descriptor.collection().clone(),
                Revision::new("v1").unwrap(),
                knowledge::content_digest("record"),
                chrono::Utc::now(),
            )
            .access(AccessDescriptor {
                tenant: subject.authority.tenant.clone(),
                work_context: subject.authority.work_context.clone(),
                read_policy: ReadPolicy::Tenant {},
                owner: AccessSubject::Principal(subject.actor.id.clone()),
                grants: vec![],
                data_labels: labels,
                expires_at: None,
            })
            .build(&registration.descriptor)
            .unwrap()
        };
        gateway
            .validate_indexing_delivery(&subject, &member, &permit, Some(&observation(vec![])))
            .await
            .unwrap();
        assert!(
            gateway
                .validate_indexing_delivery(&subject, &member, &permit, None)
                .await
                .is_err()
        );
        let error = gateway
            .validate_indexing_delivery(
                &subject,
                &member,
                &permit,
                Some(&observation(vec!["secret".parse().unwrap()])),
            )
            .await
            .unwrap_err();
        assert!(
            gateway
                .finish_resource_read(&subject, &member, Some(&permit), Err(error))
                .await
                .is_err()
        );
        let page =
            db.b.audit_page(
                &AuditReadScope::new(Some(subject.authority.tenant.clone()), false),
                &AuditQuery::new(AuditPartition::Tenant(subject.authority.tenant.clone())),
            )
            .await
            .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].draft.outcome(), AuditOutcome::Denied);
        gateway
            .finish_resource_read(
                &subject,
                &member,
                Some(&permit),
                Ok((ReadResourceResult::new(vec![]), Some(observation(vec![])))),
            )
            .await
            .unwrap();
        let mut committed =
            db.b.client()
                .query("SELECT VALUE reads FROM audit_indexing_window;")
                .await
                .unwrap()
                .check()
                .unwrap();
        assert_eq!(
            committed.take::<Vec<u64>>(0).unwrap(),
            vec![1],
            "delivery requires a committed window update"
        );
        let mut filter = rmcp::model::SubscriptionFilter::new();
        filter.tools_list_changed = Some(true);
        assert!(
            gateway
                .admit_indexing_subscription_filter(&subject, &filter)
                .await
                .is_err()
        );
        let page =
            db.b.audit_page(
                &AuditReadScope::new(Some(subject.authority.tenant.clone()), false),
                &AuditQuery::new(AuditPartition::Tenant(subject.authority.tenant.clone())),
            )
            .await
            .unwrap();
        assert_eq!(page.records.len(), 2);
        assert!(
            page.records
                .iter()
                .all(|record| record.draft.outcome() == AuditOutcome::Denied)
        );
        let mut changed = plane.clone();
        changed.servers[0].knowledge[0]
            .authoritative_for
            .insert(veoveo_knowledge_contract::KnowledgeSubject::new("New approval").unwrap());
        gateway.catalog.replace(Arc::new(
            crate::GatewayCatalog::from_control_plane(changed).unwrap(),
        ));
        assert!(
            gateway
                .validate_indexing_delivery(&subject, &member, &permit, Some(&observation(vec![])))
                .await
                .is_err(),
            "approval changed while source read was in flight"
        );
        assert!(
            gateway
                .validate_indexing_delivery(&subject, &contract, &contract_permit, None)
                .await
                .is_err()
        );
        let mut catalog_only = plane.clone();
        catalog_only.servers[0].knowledge[0].mode =
            veoveo_knowledge_contract::CollectionApproval::CatalogOnly;
        gateway.catalog.replace(Arc::new(
            crate::GatewayCatalog::from_control_plane(catalog_only).unwrap(),
        ));
        assert!(
            gateway
                .admit_indexing_read(&subject, &contract, &contract_meta)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            gateway
                .admit_indexing_read(&subject, &member, &member_meta)
                .await
                .is_err()
        );
        assert!(
            gateway
                .admit_indexing_subscription(
                    &subject,
                    &project("media://records"),
                    &Default::default()
                )
                .await
                .is_err()
        );
        let mut ordinary = subject.clone();
        ordinary.access_token.oauth_client_id = "operator-local-public".parse().unwrap();
        assert!(
            gateway
                .admit_indexing_read(&ordinary, &member, &member_meta)
                .await
                .is_err(),
            "ordinary clients cannot assert indexing authority"
        );
        gateway
            .state
            .audit_writer()
            .await
            .shutdown(Duration::from_secs(5))
            .await
            .unwrap();
    })
    .await
    .expect("indexing admission exceeded 180 seconds");
}
