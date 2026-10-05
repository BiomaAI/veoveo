use super::*;
use std::collections::BTreeMap;
use std::time::Duration;
use surrealdb::types::RecordId;
use uuid::Uuid;
use veoveo_agent_runtime::persistence::AgentRepository;
use veoveo_agent_runtime::persistence::{instances::*, *};
use veoveo_platform_store::{PrincipalKind, WorkContextMembershipLevel};

async fn pilot(
    store: &PlatformStore,
    tenant_key: &str,
    context_key: &str,
    key: &str,
) -> ManagedAgentInstance {
    let owner = store
        .ensure_identity(
            tenant_key,
            "owner",
            "https://identity.test",
            "owner",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    let tenant = owner.tenant_id.record_id();
    let context = deterministic_work_context_id(tenant_key, context_key)
        .unwrap()
        .record_id();
    store
        .client()
        .query(include_str!(
            "../../queries/server/agent_targets_tests/pilot.surql"
        ))
        .bind(("context", context.clone()))
        .bind(("tenant", tenant.clone()))
        .bind(("key", context_key.to_owned()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let version = store
        .artifact_read_context_version(tenant_key, context_key)
        .await
        .unwrap()
        .unwrap();
    let authority = AgentCatalogAuthority::new(
        owner.tenant_id,
        deterministic_work_context_id(tenant_key, context_key).unwrap(),
        owner.principal_id,
        version.digest,
        WorkContextMembershipLevel::Contributor,
        false,
        100,
    );
    let content = AgentContent {
        model: AgentModelReference {
            id: "approved".into(),
            revision: "a".repeat(64),
        },
        instructions: "Use only your current domain grant.".into(),
        tools: vec![],
        budgets: AgentBudgets {
            max_output_tokens: 128,
            max_completion_calls: 2,
            max_tool_calls: 4,
            deadline_seconds: 60,
        },
        execution: AgentExecution::Managed {
            template: "pilot".into(),
            template_revision: "b".repeat(64),
            // A requested assignment alone must never make an App target.
            parameters: BTreeMap::from([
                (
                    "session".into(),
                    AgentTemplateParameter::Text("session-one".into()),
                ),
                (
                    "vehicle".into(),
                    AgentTemplateParameter::Text("vehicle-one".into()),
                ),
            ]),
            resource_subscriptions: vec![],
        },
    };
    let draft = AgentRepository::new(store.clone())
        .mutate_agent_definition(
            &authority,
            key,
            Uuid::now_v7(),
            None,
            AgentDefinitionMutation::Create {
                name: key.into(),
                description: "Fixture pilot".into(),
                content,
            },
        )
        .await
        .unwrap();
    let published = AgentRepository::new(store.clone())
        .mutate_agent_definition(
            &authority,
            key,
            Uuid::now_v7(),
            Some(draft.revision),
            AgentDefinitionMutation::Publish {
                digest: draft.draft_digest,
                audience: vec![AgentPublicationContext {
                    work_context: context.clone(),
                    context_digest: authority.context_digest.clone(),
                }],
            },
        )
        .await
        .unwrap();
    let client = format!("{tenant_key}-{key}");
    let operation = AgentRepository::new(store.clone())
        .mutate_managed_agent(
            &authority,
            key,
            Uuid::now_v7(),
            None,
            ManagedAgentMutation::Provision {
                plan: Box::new(ManagedAgentProvision {
                    name: key.into(),
                    definition_key: key.into(),
                    revision: published.draft_digest,
                    identity: ManagedAgentIdentity {
                        client_id: client.clone(),
                        issuer: "https://gateway.test/oauth".into(),
                        authorization_server: "gateway".into(),
                        profile: "agent".into(),
                        resource: "https://gateway.test/mcp/agent".into(),
                        scopes: vec!["operator:use".into()],
                        roles: vec![],
                        membership: WorkContextMembershipLevel::Contributor,
                    },
                    resources: ManagedAgentResources {
                        namespace: "agents".into(),
                        workload: client.clone(),
                        credential_secret: format!("{client}-key"),
                        volume_claim: format!("{client}-memory"),
                        template_config_map: "pilot-template".into(),
                        image: format!("registry.test/kernel@sha256:{}", "c".repeat(64)),
                        storage_gib: 2,
                    },
                }),
            },
            ManagedAgentLimits {
                instances: 32,
                storage_gib: 128,
            },
        )
        .await
        .unwrap();
    let manager = Uuid::now_v7();
    let claim = AgentRepository::new(store.clone())
        .claim_managed_agent_operation(operation.id, manager)
        .await
        .unwrap()
        .unwrap()
        .claim(manager)
        .unwrap();
    AgentRepository::new(store.clone())
        .observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    AgentRepository::new(store.clone())
        .register_managed_agent_key(
            &claim,
            ManagedAgentPublicKey {
                kid: "fixture".into(),
                n: "public-modulus".into(),
                e: "AQAB".into(),
            },
        )
        .await
        .unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        ManagedAgentPhase::Workload,
        ManagedAgentPhase::Ready,
    ] {
        AgentRepository::new(store.clone())
            .observe_managed_agent(&claim, phase, None)
            .await
            .unwrap();
    }
    AgentRepository::new(store.clone())
        .managed_agent(&authority, key)
        .await
        .unwrap()
}

async fn grant(store: &PlatformStore, pilot: &ManagedAgentInstance, session: &str) -> RecordId {
    let id = RecordId::new("uav_vehicle_control_grant", Uuid::now_v7().to_string());
    store
        .client()
        .query(include_str!(
            "../../queries/server/agent_targets_tests/grant.surql"
        ))
        .bind((
            "profile",
            veoveo_map_mcp::contract::MapMobilityProfileUri::new(
                veoveo_map_mcp::contract::MobilityProfileId::from_stable_key(b"fixture"),
                veoveo_map_mcp::contract::MobilityProfileVersion::FIRST,
            )
            .as_str()
            .to_owned(),
        ))
        .bind(("id", id.clone()))
        .bind(("tenant", pilot.tenant.clone()))
        .bind(("context", pilot.work_context.clone()))
        .bind(("key", pilot.key.clone()))
        .bind(("simulation_session", session.to_owned()))
        .bind((
            "principal",
            format!("{}#{}", pilot.identity.issuer, pilot.identity.client_id),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    id
}
async fn listed(store: &PlatformStore, pilot: &ManagedAgentInstance, session: &str) -> Vec<String> {
    scoped_targets(
        store,
        pilot.tenant.clone(),
        pilot.work_context.clone(),
        &SessionId::parse(session).unwrap(),
    )
    .await
    .unwrap()
}
async fn change(store: &PlatformStore, sql: &str, id: RecordId) {
    store
        .client()
        .query(sql)
        .bind(("id", id))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn message_targets_require_current_scoped_grants_and_managed_authority() {
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    let one = pilot(&db.a, "one", "operations", "pilot").await;
    let other = pilot(&db.a, "two", "operations", "pilot").await;
    let private = pilot(&db.a, "one", "private", "private-pilot").await;
    grant(&db.a, &other, "session-one").await;
    grant(&db.a, &private, "session-one").await;
    assert!(
        listed(&db.a, &one, "session-one").await.is_empty(),
        "a requested template assignment is not a grant; other contexts and tenants stay private"
    );
    let grant = grant(&db.a, &one, "session-one").await;
    assert_eq!(listed(&db.b, &one, "session-one").await, ["pilot"]);
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority.surql"),
        one.id.clone(),
    )
    .await;
    assert!(
        listed(&db.a, &one, "session-one").await.is_empty(),
        "a worker must finish admission before the App advertises it"
    );
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_2.surql"),
        one.id.clone(),
    )
    .await;
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_3.surql"),
        one.definition.clone(),
    )
    .await;
    assert!(listed(&db.a, &one, "session-one").await.is_empty());
    db.b.client()
        .query(include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_4.surql"))
        .bind(("id", one.definition.clone()))
        .bind(("context", one.work_context.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(listed(&db.b, &one, "session-two").await.is_empty());
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_5.surql"),
        grant.clone(),
    )
    .await;
    assert!(listed(&db.a, &one, "session-one").await.is_empty());
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_6.surql"),
        grant.clone(),
    )
    .await;
    assert!(listed(&db.a, &one, "session-one").await.is_empty());
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_7.surql"),
        grant.clone(),
    )
    .await;
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_8.surql"),
        one.definition.clone(),
    )
    .await;
    assert!(listed(&db.a, &one, "session-one").await.is_empty());
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_9.surql"),
        one.definition.clone(),
    )
    .await;
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_10.surql"),
        one.principal.clone(),
    )
    .await;
    assert!(listed(&db.a, &one, "session-one").await.is_empty());
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_11.surql"),
        one.principal.clone(),
    )
    .await;
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_12.surql"),
        one.id.clone(),
    )
    .await;
    assert_eq!(
        listed(&db.a, &one, "session-one").await,
        ["pilot"],
        "paused pilots retain their inbox"
    );
    change(
        &db.b,
        include_str!("../../queries/server/agent_targets_tests/message_targets_require_current_scoped_grants_and_managed_authority_13.surql"),
        one.id.clone(),
    )
    .await;
    assert!(
        listed(&db.a, &one, "session-one")
            .await
            .iter()
            .all(|key| key != "pilot")
    );
}

#[tokio::test]
async fn cross_replica_grant_revocation_invalidates_catalog_without_domain_wakes() {
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    let one = pilot(&db.a, "reactive", "operations", "pilot").await;
    let grant = grant(&db.a, &one, "session-one").await;
    let hub = Arc::new(SubscriptionHub::new());
    let mut lists = hub.listen_resource_list_changes();
    let mut resources = hub.listen();
    let stop = CancellationToken::new();
    let task = tokio::spawn(observe(db.a.clone(), hub, stop.clone()));
    tokio::time::timeout(Duration::from_secs(10), lists.recv())
        .await
        .unwrap()
        .unwrap();
    change(&db.b, include_str!("../../queries/server/agent_targets_tests/cross_replica_grant_revocation_invalidates_catalog_without_domain_wakes.surql"), grant).await;
    tokio::time::timeout(Duration::from_secs(10), lists.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(listed(&db.a, &one, "session-one").await.is_empty());
    assert!(
        matches!(
            resources.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ),
        "App metadata changes must not produce agent domain-resource wakes"
    );
    stop.cancel();
    task.await.unwrap();
}
