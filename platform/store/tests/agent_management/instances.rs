use super::*;
use veoveo_platform_store::agent_management::instances::*;

const LIMITS: ManagedAgentLimits = ManagedAgentLimits {
    instances: 20,
    storage_gib: 100,
};

#[tokio::test]
async fn concurrent_instances_share_atomic_storage_and_instance_quota() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    assert_eq!(
        db.a.mutate_agent_definition(
            &a,
            "pilot",
            Uuid::now_v7(),
            Some(definition.revision),
            AgentDefinitionMutation::Draft {
                content: content("A different execution mode")
            }
        )
        .await,
        Err(AgentManagementError::Conflict),
        "execution mode belongs to the stable definition"
    );
    let limits = ManagedAgentLimits {
        instances: 20,
        storage_gib: 2,
    };
    let (one, two) = tokio::join!(
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            limits
        ),
        db.b.mutate_managed_agent(
            &a,
            "two",
            Uuid::now_v7(),
            None,
            plan(&definition, "two"),
            limits
        )
    );
    assert_ne!(one.is_ok(), two.is_ok());
    assert_eq!(
        one.err().or(two.err()),
        Some(AgentManagementError::Capacity)
    );
    assert_eq!(db.a.managed_agents(&a, None, 20).await.unwrap().len(), 1);
}

#[tokio::test]
async fn revision_update_retains_identity_and_waits_for_runtime_lease() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op =
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            LIMITS,
        )
        .await
        .unwrap();
    let initial = claim(&db.a, &op).await;
    provision_through_workload(&db.a, &initial).await;
    db.a.observe_managed_agent(&initial, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    let first = db.a.managed_agent(&a, "one").await.unwrap();
    assert_eq!(
        db.a.managed_agent_episode_admission(first.id.clone(), 1)
            .await
            .unwrap(),
        Some(1)
    );
    let mut changed = definition.draft.clone();
    changed.instructions = "Changed instructions".into();
    let draft =
        db.a.mutate_agent_definition(
            &a,
            "pilot",
            Uuid::now_v7(),
            Some(definition.revision),
            AgentDefinitionMutation::Draft { content: changed },
        )
        .await
        .unwrap();
    let published = publish(&db.a, &a, &draft).await;
    assert_eq!(
        db.a.managed_agent(&a, "one")
            .await
            .unwrap()
            .requested_revision,
        first.requested_revision,
        "publish never adopts implicitly"
    );
    let update =
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            Some(1),
            ManagedAgentMutation::Revision {
                digest: published.draft_digest,
            },
            LIMITS,
        )
        .await
        .unwrap();
    assert_eq!(
        db.a.managed_agent_episode_admission(first.id.clone(), 1)
            .await
            .unwrap(),
        None
    );
    assert!(
        db.a.managed_agent_dispatch(first.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    let next = claim(&db.a, &update).await;
    for phase in [
        ManagedAgentPhase::Credentials,
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
    ] {
        db.a.observe_managed_agent(&next, phase, None)
            .await
            .unwrap();
    }
    db.a.client().query("CREATE agent SET tenant = $tenant, agent_key = 'one', display_name = 'One', profile = type::record('profile', rand::uuid::v7()), state = 'idle', manifest = {}, memory_database = 'memory.duckdb', work_context = $context, policy_revision = 'test-v1', authority = {context_key:'operations', membership:'contributor', policy_revision:'test-v1', owner_kind:'principal', owner_key:'alice', initial_grants:[], data_labels:[], invocation_mode:'automated'}, lease_owner = 'prior-kernel', lease_expires_at = time::now() + 1h;")
        .bind(("tenant", a.tenant.clone())).bind(("context", a.work_context.clone())).await.unwrap().check().unwrap();
    assert_eq!(
        db.a.observe_managed_agent(&next, ManagedAgentPhase::Workload, None)
            .await,
        Err(AgentManagementError::Conflict)
    );
    db.a.client().query("UPDATE agent SET lease_expires_at = time::now() - 1s WHERE tenant = $tenant AND agent_key = 'one';").bind(("tenant", a.tenant.clone())).await.unwrap().check().unwrap();
    db.a.observe_managed_agent(&next, ManagedAgentPhase::Workload, None)
        .await
        .unwrap();
    let updated = db.a.managed_agent(&a, "one").await.unwrap();
    assert_eq!(updated.principal, first.principal);
    assert_eq!(updated.resources, first.resources);
    assert_eq!(updated.public_key, first.public_key);
    assert_eq!(updated.active_revision, Some(updated.requested_revision));
    assert!(
        !db.a
            .managed_agent_dispatch(first.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert_eq!(
        db.a.managed_agent_episode_admission(first.id, 2)
            .await
            .unwrap(),
        Some(1)
    );
}

async fn managed_definition(
    store: &PlatformStore,
    actor: &AgentCatalogAuthority,
) -> AgentDefinition {
    let mut candidate = content("Managed instructions remain literal: ${PRIVATE_KEY}");
    candidate.execution = AgentExecution::Managed {
        template: "pilot".into(),
        template_revision: "b".repeat(64),
        parameters: Default::default(),
        resource_subscriptions: vec![],
    };
    let draft = store
        .mutate_agent_definition(
            actor,
            "pilot",
            Uuid::now_v7(),
            None,
            AgentDefinitionMutation::Create {
                name: "Pilot".into(),
                description: "A managed test".into(),
                content: candidate,
            },
        )
        .await
        .unwrap();
    publish(store, actor, &draft).await
}

fn plan(definition: &AgentDefinition, key: &str) -> ManagedAgentMutation {
    ManagedAgentMutation::Provision {
        plan: Box::new(ManagedAgentProvision {
            name: format!("Pilot {key}"),
            definition_key: definition.key.clone(),
            revision: definition.draft_digest.clone(),
            identity: ManagedAgentIdentity {
                client_id: format!("managed-{key}"),
                issuer: "https://test.example/oauth".into(),
                authorization_server: "gateway".into(),
                profile: "pilot".into(),
                resource: "https://test.example/mcp/pilot".into(),
                scopes: vec!["mcp:read".into()],
                roles: vec!["managed-pilot".into()],
                membership: WorkContextMembershipLevel::Contributor,
            },
            resources: ManagedAgentResources {
                namespace: "agents".into(),
                workload: key.into(),
                credential_secret: format!("{key}-key"),
                volume_claim: format!("{key}-memory"),
                template_config_map: "pilot-template".into(),
                image: format!("registry.test/kernel@sha256:{}", "c".repeat(64)),
                storage_gib: 2,
            },
        }),
    }
}

async fn claim(store: &PlatformStore, operation: &ManagedAgentOperation) -> ManagedAgentClaim {
    let owner = Uuid::now_v7();
    store
        .claim_managed_agent_operation(operation.id.clone(), owner)
        .await
        .unwrap()
        .unwrap()
        .claim(owner)
        .unwrap()
}

async fn provision_through_workload(store: &PlatformStore, claim: &ManagedAgentClaim) {
    store
        .observe_managed_agent(claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    let key = ManagedAgentPublicKey {
        kid: "managed-key".into(),
        n: "public-modulus".into(),
        e: "AQAB".into(),
    };
    store
        .register_managed_agent_key(claim, key.clone())
        .await
        .unwrap();
    store.register_managed_agent_key(claim, key).await.unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        ManagedAgentPhase::Workload,
    ] {
        store
            .observe_managed_agent(claim, phase, None)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn instance_admission_is_atomic_private_and_idempotent() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    let bob = identity(&db.a, "instances", "bob").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let b = authority(&db.b, &bob, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let request = Uuid::now_v7();
    let admission = plan(&definition, "one");
    let op =
        db.a.mutate_managed_agent(&a, "one", request, None, admission.clone(), LIMITS)
            .await
            .unwrap();
    let replay =
        db.b.mutate_managed_agent(&a, "one", request, None, admission, LIMITS)
            .await
            .unwrap();
    assert_eq!(op, replay);
    assert_eq!(db.a.managed_agents(&a, None, 20).await.unwrap().len(), 1);
    assert!(db.b.managed_agents(&b, None, 20).await.unwrap().is_empty());
    assert_eq!(
        db.b.managed_agent(&b, "one").await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        db.b.managed_agent_operation(&b, op.id.clone()).await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        db.b.mutate_managed_agent(&a, "two", request, None, plan(&definition, "two"), LIMITS)
            .await,
        Err(AgentManagementError::Conflict)
    );
    assert!(
        db.a.managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none(),
        "registration awaits public key"
    );
    let limits = ManagedAgentLimits {
        instances: 1,
        storage_gib: 2,
    };
    assert_eq!(
        db.a.mutate_managed_agent(
            &a,
            "two",
            Uuid::now_v7(),
            None,
            plan(&definition, "two"),
            limits
        )
        .await,
        Err(AgentManagementError::Capacity)
    );
    assert_eq!(db.a.managed_agents(&a, None, 20).await.unwrap().len(), 1);
    // Failed quota admission must not reserve either identity or client ID.
    db.a.mutate_managed_agent(
        &a,
        "two",
        Uuid::now_v7(),
        None,
        plan(&definition, "two"),
        LIMITS,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn controller_recovery_fences_stale_workers_and_never_rotates_an_uncertain_key() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op =
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            LIMITS,
        )
        .await
        .unwrap();
    assert_eq!(
        db.a.pending_managed_agent_operations("agents", 20, false)
            .await
            .unwrap()
            .len(),
        1
    );
    let first = claim(&db.a, &op).await;
    assert!(
        db.b.claim_managed_agent_operation(op.id.clone(), Uuid::now_v7())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.a.observe_managed_agent(&first, ManagedAgentPhase::Ready, None)
            .await,
        Err(AgentManagementError::Conflict)
    );
    db.a.client()
        .query("UPDATE ONLY $operation SET lease_expires_at = time::now() - 1s;")
        .bind(("operation", op.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let second = claim(&db.b, &op).await;
    assert!(second.fence > first.fence);
    assert_eq!(
        db.a.renew_managed_agent_claim(&first).await,
        Err(AgentManagementError::Conflict)
    );
    assert_eq!(
        db.a.claimed_managed_agent(&first).await,
        Err(AgentManagementError::Conflict)
    );
    provision_through_workload(&db.b, &second).await;
    assert_eq!(
        db.b.register_managed_agent_key(
            &second,
            ManagedAgentPublicKey {
                kid: "new".into(),
                n: "new-modulus".into(),
                e: "AQAB".into()
            }
        )
        .await,
        Err(AgentManagementError::Conflict)
    );
    let instance = db.a.managed_agent(&a, "one").await.unwrap();
    assert_eq!((instance.active_generation, instance.generation), (1, 1));
    assert!(
        db.a.managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert!(
        !db.a
            .managed_agent_dispatch(instance.id.clone(), 2, 1)
            .await
            .unwrap()
    );
    assert!(
        db.a.managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_some()
    );
    db.b.observe_managed_agent(&second, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    assert!(
        db.a.pending_managed_agent_operations("agents", 20, false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.b.renew_managed_agent_claim(&second).await,
        Err(AgentManagementError::Conflict)
    );
}

#[tokio::test]
async fn pause_drains_stop_fences_and_archive_retains_capacity() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op =
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            LIMITS,
        )
        .await
        .unwrap();
    let initial = claim(&db.a, &op).await;
    provision_through_workload(&db.a, &initial).await;
    db.a.observe_managed_agent(&initial, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    let instance = db.a.managed_agent(&a, "one").await.unwrap();
    let pause =
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            Some(1),
            ManagedAgentMutation::State {
                desired: ManagedAgentDesired::Paused,
            },
            LIMITS,
        )
        .await
        .unwrap();
    assert!(
        db.a.managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap(),
        "an admitted episode drains"
    );
    assert_eq!(
        db.a.managed_agent_episode_admission(instance.id.clone(), 1)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        db.b.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            Some(1),
            ManagedAgentMutation::Stop,
            LIMITS
        )
        .await,
        Err(AgentManagementError::Conflict)
    );
    let paused = claim(&db.a, &pause).await;
    db.a.mutate_managed_agent(
        &a,
        "one",
        Uuid::now_v7(),
        Some(2),
        ManagedAgentMutation::Stop,
        LIMITS,
    )
    .await
    .unwrap();
    assert!(
        !db.a
            .managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert_eq!(
        db.a.renew_managed_agent_claim(&paused).await,
        Err(AgentManagementError::Conflict)
    );
    db.a.mutate_managed_agent(
        &a,
        "one",
        Uuid::now_v7(),
        Some(3),
        ManagedAgentMutation::State {
            desired: ManagedAgentDesired::Archived,
        },
        LIMITS,
    )
    .await
    .unwrap();
    assert!(
        db.a.managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !db.a
            .managed_agent_dispatch(instance.id.clone(), 1, 2)
            .await
            .unwrap()
    );
    let retained = db.a.managed_agent(&a, "one").await.unwrap();
    assert_eq!(retained.resources, instance.resources);
    assert_eq!(retained.principal, instance.principal);
    assert_eq!(
        db.a.mutate_managed_agent(
            &a,
            "two",
            Uuid::now_v7(),
            None,
            plan(&definition, "two"),
            ManagedAgentLimits {
                instances: 1,
                storage_gib: 2
            }
        )
        .await,
        Err(AgentManagementError::Capacity)
    );
    assert_eq!(
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            Some(4),
            ManagedAgentMutation::State {
                desired: ManagedAgentDesired::Running
            },
            LIMITS
        )
        .await,
        Err(AgentManagementError::Conflict)
    );
}

#[tokio::test]
async fn current_revocation_and_context_changes_override_retained_instance_revision() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op =
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            LIMITS,
        )
        .await
        .unwrap();
    let first = claim(&db.a, &op).await;
    provision_through_workload(&db.a, &first).await;
    let instance = db.a.managed_agent(&a, "one").await.unwrap();
    db.a.client()
        .query("UPDATE ONLY $principal SET enabled = false;")
        .bind(("principal", instance.principal.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        db.a.managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !db.a
            .managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    db.a.client()
        .query("UPDATE ONLY $principal SET enabled = true;")
        .bind(("principal", instance.principal.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    db.a.mutate_agent_definition(
        &a,
        "pilot",
        Uuid::now_v7(),
        Some(definition.revision),
        AgentDefinitionMutation::Status {
            status: AgentDefinitionStatus::Disabled,
        },
    )
    .await
    .unwrap();
    assert!(
        db.a.managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !db.a
            .managed_agent_dispatch(instance.id, 1, 1)
            .await
            .unwrap()
    );
    db.a.client()
        .query("UPDATE ONLY $context SET policy_revision = 'changed';")
        .bind(("context", a.work_context.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        db.a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            Some(1),
            ManagedAgentMutation::Stop,
            LIMITS
        )
        .await,
        Err(AgentManagementError::Forbidden)
    );
}

#[tokio::test]
async fn controller_inventory_is_namespace_scoped_and_recovers_settled_generations() {
    let db = TestDb::new().await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let actor = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &actor).await;
    let operation =
        db.a.mutate_managed_agent(
            &actor,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            LIMITS,
        )
        .await
        .unwrap();
    assert!(
        db.a.pending_managed_agent_operations("another-namespace", 20, true)
            .await
            .unwrap()
            .is_empty()
    );
    let first = claim(&db.a, &operation).await;
    let snapshot = db.a.managed_agent_reconciliation(&first).await.unwrap();
    assert_eq!(snapshot.instance.generation, 1);
    assert!(snapshot.runtime.is_none());
    assert!(!snapshot.episode_running);
    assert_eq!(snapshot.revision.digest, definition.draft_digest);
    provision_through_workload(&db.a, &first).await;
    db.a.observe_managed_agent(&first, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    assert!(
        db.a.pending_managed_agent_operations("agents", 20, false)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        db.a.pending_managed_agent_operations("agents", 20, true)
            .await
            .unwrap()
            .len(),
        1
    );
    let recovered = claim(&db.b, &operation).await;
    assert!(recovered.fence > first.fence);
    assert_eq!(
        db.a.managed_agent_reconciliation(&first).await.unwrap_err(),
        AgentManagementError::Conflict
    );
    db.b.observe_managed_agent(&recovered, ManagedAgentPhase::Workload, None)
        .await
        .unwrap();
    db.b.release_managed_agent_claim(&recovered).await.unwrap();
    assert_eq!(
        db.a.pending_managed_agent_operations("agents", 20, false)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.b.renew_managed_agent_claim(&recovered).await,
        Err(AgentManagementError::Conflict)
    );
}
