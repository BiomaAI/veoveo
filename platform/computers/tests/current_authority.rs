mod support;
use std::time::Duration;
use support::{TestDb, policy};
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputerError, ComputersStore, Operation, OperationStage,
    Reservation, api::Action,
};
use veoveo_mcp_contract::*;
use veoveo_platform_store::{
    RecordId, deterministic_enterprise_id, deterministic_principal_id, deterministic_tenant_id,
};
use veoveo_task_runtime::{ClaimedTask, TaskOwner, TaskRuntime};

async fn queued(
    db: &TestDb,
    identity: GatewayInternalIdentity,
) -> (ComputersStore, TaskOwner, Operation, ClaimedTask) {
    let store = ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
    store
        .install_capacity(
            None,
            CapacityPolicy {
                per_owner: 4,
                per_tenant: 10,
                provider: 10,
            },
        )
        .await
        .unwrap();
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let owner = actor.owner().clone();
    let source = &identity.request_context.as_ref().unwrap().principal;
    db.a.ensure_identity(
        owner.tenant_key(),
        source.id.as_str(),
        source.issuer.as_str(),
        source.subject.as_str(),
        match source.kind {
            PrincipalKind::User => veoveo_platform_store::PrincipalKind::User,
            PrincipalKind::Service => veoveo_platform_store::PrincipalKind::Service,
        },
    )
    .await
    .unwrap();
    let computer = store
        .reserve(
            &owner,
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    let operation = store
        .queue_operation(actor, computer.computer_id, Uuid::now_v7(), Action::Create)
        .await
        .unwrap();
    store
        .ensure_operation_task(&owner, operation.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.b.clone(), "computers", "worker-b");
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    (store, owner, operation, claim)
}

#[tokio::test]
async fn queued_work_obeys_current_policy_and_preserves_the_actual_dispatch_decision() {
    let db = TestDb::new().await;
    let baseline = policy::control();
    policy::install(&db.a, baseline.clone()).await;
    let (store, owner, operation, claim) =
        queued(&db, support::identity(&support::owner("alice"))).await;
    for change in [
        "deny",
        "scope",
        "principal",
        "membership",
        "classification",
        "label",
        "exposure",
        "actions",
        "client",
    ] {
        let mut current = baseline.clone();
        match change {
            "deny" => current.policies[0].rules[0].effect = PolicyEffect::Deny,
            "scope" => {
                current.policies[0].rules[0]
                    .required_scopes
                    .insert(ScopeName::new("extra:required").unwrap());
                for client in &mut current.oauth_clients {
                    client
                        .allowed_scopes
                        .insert(ScopeName::new("extra:required").unwrap());
                }
            }
            "principal" => {
                current.policies[0].rules[0]
                    .principal_ids
                    .insert(PrincipalId::new("https://computers.test#bob").unwrap());
            }
            "membership" => {
                current.work_contexts[0].memberships[0].level = WorkContextMembershipLevel::Viewer
            }
            "classification" => {
                current.work_contexts[0].output_policy.classification =
                    Some(DataLabelId::new("cui").unwrap())
            }
            "label" => {
                current.work_contexts[0]
                    .output_policy
                    .data_labels
                    .insert(DataLabelId::new("pii").unwrap());
            }
            "exposure" => current.profiles[0].servers[0].tools = Exposure::None,
            "client" => {
                current.oauth_clients[0].id = OAuthClientId::new("replacement-console").unwrap();
                let clients = &mut current.work_contexts[0].memberships[0].oauth_clients;
                clients.remove(&OAuthClientId::new("console").unwrap());
                clients.insert(OAuthClientId::new("replacement-console").unwrap());
            }
            _ => {
                current.policies[0].rules[0].actions =
                    [GatewayAction::ResourcesRead].into_iter().collect();
            }
        }
        policy::install(&db.b, current).await;
        assert!(
            matches!(
                store.begin_dispatch(&claim).await,
                Err(ComputerError::Forbidden)
            ),
            "{change}"
        );
        let retained = store
            .operation(&owner, operation.operation_id)
            .await
            .unwrap();
        assert_eq!(retained.stage, OperationStage::Queued);
        assert!(retained.dispatch_id.is_none());
        assert_eq!(
            store
                .get(&owner, operation.computer_id)
                .await
                .unwrap()
                .active_operation,
            Some(operation.operation_id)
        );
    }
    let revision = policy::install(&db.b, baseline).await;
    let ticket = store.begin_dispatch(&claim).await.unwrap();
    assert!(ticket.authority_remaining() <= Duration::from_secs(30));
    let retained = store
        .operation(&owner, operation.operation_id)
        .await
        .unwrap();
    let evidence = retained.dispatch_authority.unwrap();
    assert_eq!(evidence.control_revision, revision);
    assert_eq!(
        evidence.decision.principal.as_ref().map(|p| p.as_str()),
        Some(owner.principal_key.as_str())
    );
    assert_eq!(
        evidence.decision.trace_id.as_str(),
        operation.operation_id.to_string()
    );
    assert!(store.begin_dispatch(&claim).await.is_err());
    // Corrupt accepted evidence cannot become observation authority on another replica.
    db.b.client()
        .query(
            "UPDATE ONLY $operation SET dispatch_authority.decision.trace_id = 'wrong-operation';",
        )
        .bind((
            "operation",
            RecordId::new(
                "computer_operation",
                surrealdb::types::Uuid::from(operation.operation_id),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        store.operation(&owner, operation.operation_id).await,
        Err(ComputerError::Unavailable)
    ));
}

#[tokio::test]
async fn accepted_work_outlives_its_token_but_not_current_account_authority() {
    let db = TestDb::new().await;
    policy::install_default(&db.a).await;
    let mut identity = support::identity(&support::owner("alice"));
    identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .expires_at = chrono::Utc::now() + chrono::TimeDelta::seconds(3);
    let expires_at = identity
        .request_context
        .as_ref()
        .unwrap()
        .access_token
        .expires_at;
    let (store, owner, operation, claim) = queued(&db, identity).await;
    for record in [
        deterministic_enterprise_id().record_id(),
        deterministic_tenant_id("test").unwrap().record_id(),
        deterministic_principal_id("test", &owner.principal_key)
            .unwrap()
            .record_id(),
    ] {
        db.b.client()
            .query("UPDATE ONLY $record SET enabled = false;")
            .bind(("record", record.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store.begin_dispatch(&claim).await,
            Err(ComputerError::Forbidden)
        ));
        db.b.client()
            .query("UPDATE ONLY $record SET enabled = true;")
            .bind(("record", record))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let remaining = (expires_at - chrono::Utc::now())
        .to_std()
        .unwrap_or_default();
    tokio::time::sleep(remaining + Duration::from_millis(10)).await;
    let ticket = store.begin_dispatch(&claim).await.unwrap();
    assert_eq!(ticket.operation().operation_id, operation.operation_id);
    assert!(ticket.operation().dispatch_authority.is_some());
}

#[tokio::test]
async fn unavailable_or_corrupt_current_policy_never_dispatches() {
    let db = TestDb::new().await;
    let (store, _, operation, claim) =
        queued(&db, support::identity(&support::owner("alice"))).await;
    assert!(matches!(
        store.begin_dispatch(&claim).await,
        Err(ComputerError::Unavailable)
    ));
    let revision = policy::install(&db.b, policy::control()).await;
    db.b.client()
        .query("BEGIN; LET $original = SELECT * FROM ONLY $revision; CREATE gateway_control_revision:corrupt CONTENT { revision_id: 'corrupt', sha256: $wrong, source: $original.source, applied_at: time::now(), applied_by: 'isolated-test', control_plane: $original.control_plane }; UPSERT gateway_control_active:current SET revision = gateway_control_revision:corrupt, revision_id = 'corrupt', updated_at = time::now(); COMMIT;")
        .bind((
            "revision",
            RecordId::new("gateway_control_revision", revision),
        ))
        .bind(("wrong", "a".repeat(64)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        store.begin_dispatch(&claim).await,
        Err(ComputerError::Unavailable)
    ));
    let original = store
        .operation(&support::owner("alice"), operation.operation_id)
        .await
        .unwrap();
    assert_eq!(original.stage, OperationStage::Queued);
    assert!(original.dispatch_authority.is_none());
}

#[tokio::test]
async fn services_use_current_actor_and_source_authority_without_a_browser_session() {
    for delegated in [false, true] {
        let db = TestDb::new().await;
        policy::install_default(&db.a).await;
        let mut owner = support::owner(if delegated { "delegated" } else { "service" });
        owner.principal_kind = veoveo_platform_store::PrincipalKind::Service;
        owner.authority.provenance = if delegated {
            InvocationProvenance::Delegated {
                initiator: PrincipalId::new("https://computers.test#alice").unwrap(),
                delegation_id: DelegationId::new("accepted-delegation").unwrap(),
            }
        } else {
            InvocationProvenance::Automated
        };
        let mut identity = support::identity(&owner);
        if delegated {
            let context = identity.request_context.as_mut().unwrap();
            context.principal = support::identity(&support::owner("alice")).actor;
            context.access_token.subject = context.principal.subject.clone();
            context.access_token.oauth_client_id = OAuthClientId::new("delegated").unwrap();
        }
        let expected_source = identity
            .request_context
            .as_ref()
            .unwrap()
            .principal
            .id
            .clone();
        let (store, owner, operation, claim) = queued(&db, identity).await;
        let actors = if delegated {
            vec![owner.principal_key.clone(), expected_source.to_string()]
        } else {
            vec![owner.principal_key.clone()]
        };
        for actor in actors {
            let id = deterministic_principal_id("test", &actor)
                .unwrap()
                .record_id();
            db.b.client()
                .query("UPDATE ONLY $principal SET enabled = false;")
                .bind(("principal", id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(matches!(
                store.begin_dispatch(&claim).await,
                Err(ComputerError::Forbidden)
            ));
            db.b.client()
                .query("UPDATE ONLY $principal SET enabled = true;")
                .bind(("principal", id))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let ticket = store.begin_dispatch(&claim).await.unwrap();
        assert_eq!(ticket.operation().operation_id, operation.operation_id);
        assert_eq!(
            ticket
                .operation()
                .dispatch_authority
                .as_ref()
                .unwrap()
                .decision
                .principal
                .as_ref(),
            Some(&expected_source)
        );
        assert_eq!(ticket.operation().actor.principal_key, owner.principal_key);
    }
}
