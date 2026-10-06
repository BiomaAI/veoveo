use super::*;
use veoveo_agent_runtime::persistence::instances::*;

const LIMITS: ManagedAgentLimits = ManagedAgentLimits {
    instances: 20,
    storage_gib: 100,
};

#[tokio::test]
async fn concurrent_instances_share_atomic_storage_and_instance_quota() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    assert!(!chat_revision_allowed(&db.b, &a, "pilot", &definition.draft_digest, true).await);
    assert!(!chat_revision_allowed(&db.b, &a, "pilot", &definition.draft_digest, false).await);
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .mutate_agent_definition(
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
    let repository_a = AgentRepository::new(db.a.clone());
    let repository_b = AgentRepository::new(db.b.clone());
    let (one, two) = tokio::join!(
        repository_a.mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            limits
        ),
        repository_b.mutate_managed_agent(
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
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agents(&a, None, 20)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn revision_update_retains_identity_and_waits_for_runtime_lease() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
    AgentRepository::new(db.a.clone())
        .observe_managed_agent(&initial, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    let first = AgentRepository::new(db.a.clone())
        .managed_agent(&a, "one")
        .await
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agent_episode_admission(first.id.clone(), 1)
            .await
            .unwrap(),
        Some(1)
    );
    let mut changed = definition.draft.clone();
    changed.instructions = "Changed instructions".into();
    let AgentExecution::Managed {
        template_revision, ..
    } = &mut changed.execution
    else {
        panic!("managed fixture");
    };
    *template_revision = "c".repeat(64);
    let image = format!("registry.test/kernel@sha256:{}", "d".repeat(64));
    let draft = AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
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
        AgentRepository::new(db.a.clone())
            .managed_agent(&a, "one")
            .await
            .unwrap()
            .requested_revision,
        first.requested_revision,
        "publish never adopts implicitly"
    );
    let update = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
            &a,
            "one",
            Uuid::now_v7(),
            Some(1),
            ManagedAgentMutation::Revision {
                digest: published.draft_digest,
                image: image.clone(),
            },
            LIMITS,
        )
        .await
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agent_episode_admission(first.id.clone(), 1)
            .await
            .unwrap(),
        None
    );
    assert!(
        AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(first.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    let next = claim(&db.a, &update).await;
    for phase in [
        ManagedAgentPhase::Credentials,
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
    ] {
        AgentRepository::new(db.a.clone())
            .observe_managed_agent(&next, phase, None)
            .await
            .unwrap();
    }
    db.a.client().query(include_str!("../queries/agent_management/instances/revision_update_retains_identity_and_waits_for_runtime_lease/statement_1.surql"))
        .bind(("tenant", a.tenant.clone())).bind(("context", a.work_context.clone())).await.unwrap().check().unwrap();
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .observe_managed_agent(&next, ManagedAgentPhase::Workload, None)
            .await,
        Err(AgentManagementError::Conflict)
    );
    db.a.client().query(include_str!("../queries/agent_management/instances/revision_update_retains_identity_and_waits_for_runtime_lease/statement_2.surql")).bind(("tenant", a.tenant.clone())).await.unwrap().check().unwrap();
    AgentRepository::new(db.a.clone())
        .observe_managed_agent(&next, ManagedAgentPhase::Workload, None)
        .await
        .unwrap();
    let updated = AgentRepository::new(db.a.clone())
        .managed_agent(&a, "one")
        .await
        .unwrap();
    assert_eq!(updated.principal, first.principal);
    let mut expected_resources = first.resources;
    expected_resources.image = image;
    assert_eq!(updated.resources, expected_resources);
    assert_eq!(updated.public_key, first.public_key);
    assert_eq!(updated.active_revision, Some(updated.requested_revision));
    assert!(
        !AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(first.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agent_episode_admission(first.id, 2)
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
    let draft = AgentRepository::new(store.clone())
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
    AgentRepository::new(store.clone())
        .claim_managed_agent_operation(operation.id.clone(), owner)
        .await
        .unwrap()
        .unwrap()
        .claim(owner)
        .unwrap()
}

async fn provision_through_workload(store: &PlatformStore, claim: &ManagedAgentClaim) {
    AgentRepository::new(store.clone())
        .observe_managed_agent(claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    let key = ManagedAgentPublicKey {
        kid: "managed-key".into(),
        n: "public-modulus".into(),
        e: "AQAB".into(),
    };
    AgentRepository::new(store.clone())
        .register_managed_agent_key(claim, key.clone())
        .await
        .unwrap();
    AgentRepository::new(store.clone())
        .register_managed_agent_key(claim, key)
        .await
        .unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        ManagedAgentPhase::Workload,
    ] {
        AgentRepository::new(store.clone())
            .observe_managed_agent(claim, phase, None)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn instance_admission_is_atomic_private_and_idempotent() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    let bob = identity(&db.a, "instances", "bob").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let b = authority(&db.b, &bob, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let request = Uuid::now_v7();
    let admission = plan(&definition, "one");
    let op = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(&a, "one", request, None, admission.clone(), LIMITS)
        .await
        .unwrap();
    let replay = AgentRepository::new(db.b.clone())
        .mutate_managed_agent(&a, "one", request, None, admission, LIMITS)
        .await
        .unwrap();
    assert_eq!(op, replay);
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agents(&a, None, 20)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        AgentRepository::new(db.b.clone())
            .managed_agents(&b, None, 20)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .managed_agent(&b, "one")
            .await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .managed_agent_operation(&b, op.id.clone())
            .await,
        Err(AgentManagementError::NotFound)
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .mutate_managed_agent(&a, "two", request, None, plan(&definition, "two"), LIMITS)
            .await,
        Err(AgentManagementError::Conflict)
    );
    assert!(
        AgentRepository::new(db.a.clone())
            .managed_agent_client("managed-one")
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
        AgentRepository::new(db.a.clone())
            .mutate_managed_agent(
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
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agents(&a, None, 20)
            .await
            .unwrap()
            .len(),
        1
    );
    // Failed quota admission must not reserve either identity or client ID.
    AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
        AgentRepository::new(db.a.clone())
            .pending_managed_agent_operations("agents", 20, false, None)
            .await
            .unwrap()
            .len(),
        1
    );
    let first = claim(&db.a, &op).await;
    assert!(
        AgentRepository::new(db.b.clone())
            .claim_managed_agent_operation(op.id.clone(), Uuid::now_v7())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .observe_managed_agent(&first, ManagedAgentPhase::Ready, None)
            .await,
        Err(AgentManagementError::Conflict)
    );
    db.a.client()
        .query(include_str!("../queries/agent_management/instances/controller_recovery_fences_stale_workers_and_never_rotates_an_uncertain_key/statement_1.surql"))
        .bind(("operation", op.id.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let second = claim(&db.b, &op).await;
    assert!(second.fence > first.fence);
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .renew_managed_agent_claim(&first)
            .await,
        Err(AgentManagementError::Conflict)
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .claimed_managed_agent(&first)
            .await,
        Err(AgentManagementError::Conflict)
    );
    provision_through_workload(&db.b, &second).await;
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .register_managed_agent_key(
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
    let instance = AgentRepository::new(db.a.clone())
        .managed_agent(&a, "one")
        .await
        .unwrap();
    assert_eq!((instance.active_generation, instance.generation), (1, 1));
    assert!(
        AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert!(
        !AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id.clone(), 2, 1)
            .await
            .unwrap()
    );
    assert!(
        AgentRepository::new(db.a.clone())
            .managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_some()
    );
    AgentRepository::new(db.b.clone())
        .observe_managed_agent(&second, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    assert!(
        AgentRepository::new(db.a.clone())
            .pending_managed_agent_operations("agents", 20, false, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .renew_managed_agent_claim(&second)
            .await,
        Err(AgentManagementError::Conflict)
    );
}

#[tokio::test]
async fn pause_drains_stop_fences_and_archive_retains_capacity() {
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
    AgentRepository::new(db.a.clone())
        .observe_managed_agent(&initial, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    let instance = AgentRepository::new(db.a.clone())
        .managed_agent(&a, "one")
        .await
        .unwrap();
    let pause = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
        AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap(),
        "an admitted episode drains"
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agent_episode_admission(instance.id.clone(), 1)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .mutate_managed_agent(
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
    AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
        !AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .renew_managed_agent_claim(&paused)
            .await,
        Err(AgentManagementError::Conflict)
    );
    AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
        AgentRepository::new(db.a.clone())
            .managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id.clone(), 1, 2)
            .await
            .unwrap()
    );
    let retained = AgentRepository::new(db.a.clone())
        .managed_agent(&a, "one")
        .await
        .unwrap();
    assert_eq!(retained.resources, instance.resources);
    assert_eq!(retained.principal, instance.principal);
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .mutate_managed_agent(
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
        AgentRepository::new(db.a.clone())
            .mutate_managed_agent(
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
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &a).await;
    let op = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
    let instance = AgentRepository::new(db.a.clone())
        .managed_agent(&a, "one")
        .await
        .unwrap();
    db.a.client()
        .query(include_str!("../queries/agent_management/instances/current_revocation_and_context_changes_override_retained_instance_revision/statement_1.surql"))
        .bind(("principal", instance.principal.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        AgentRepository::new(db.a.clone())
            .managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id.clone(), 1, 1)
            .await
            .unwrap()
    );
    db.a.client()
        .query(include_str!("../queries/agent_management/instances/current_revocation_and_context_changes_override_retained_instance_revision/statement_2.surql"))
        .bind(("principal", instance.principal.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    AgentRepository::new(db.a.clone())
        .mutate_agent_definition(
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
        AgentRepository::new(db.a.clone())
            .managed_agent_client("managed-one")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        !AgentRepository::new(db.a.clone())
            .managed_agent_dispatch(instance.id, 1, 1)
            .await
            .unwrap()
    );
    db.a.client()
        .query(include_str!("../queries/agent_management/instances/current_revocation_and_context_changes_override_retained_instance_revision/statement_3.surql"))
        .bind(("context", a.work_context.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .mutate_managed_agent(
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
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "instances", "alice").await;
    context(&db.a, &alice, "operations").await;
    let actor = authority(&db.a, &alice, "operations").await;
    let definition = managed_definition(&db.a, &actor).await;
    let operation = AgentRepository::new(db.a.clone())
        .mutate_managed_agent(
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
        AgentRepository::new(db.a.clone())
            .pending_managed_agent_operations("another-namespace", 20, true, None)
            .await
            .unwrap()
            .is_empty()
    );
    let first = claim(&db.a, &operation).await;
    let snapshot = AgentRepository::new(db.a.clone())
        .managed_agent_reconciliation(&first)
        .await
        .unwrap();
    assert_eq!(snapshot.instance.generation, 1);
    assert_eq!(snapshot.instance.admission_count, 0);
    assert!(snapshot.runtime.is_none());
    assert!(!snapshot.episode_running);
    assert_eq!(snapshot.revision.digest, definition.draft_digest);
    provision_through_workload(&db.a, &first).await;
    AgentRepository::new(db.a.clone())
        .observe_managed_agent(&first, ManagedAgentPhase::Ready, None)
        .await
        .unwrap();
    assert!(
        AgentRepository::new(db.a.clone())
            .pending_managed_agent_operations("agents", 20, false, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .pending_managed_agent_operations("agents", 20, true, None)
            .await
            .unwrap()
            .len(),
        1
    );
    let recovered = claim(&db.b, &operation).await;
    assert!(recovered.fence > first.fence);
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .managed_agent_reconciliation(&first)
            .await
            .unwrap_err(),
        AgentManagementError::Conflict
    );
    AgentRepository::new(db.b.clone())
        .observe_managed_agent(&recovered, ManagedAgentPhase::Workload, None)
        .await
        .unwrap();
    AgentRepository::new(db.b.clone())
        .release_managed_agent_claim(&recovered)
        .await
        .unwrap();
    assert_eq!(
        AgentRepository::new(db.a.clone())
            .pending_managed_agent_operations("agents", 20, false, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        AgentRepository::new(db.b.clone())
            .renew_managed_agent_claim(&recovered)
            .await,
        Err(AgentManagementError::Conflict)
    );
}

#[tokio::test]
async fn manager_deadlines_follow_claims_startup_and_draining_leases() {
    tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let db = TestDb::with_modules(vec![veoveo_agent_runtime::schema::module_setup(fixture::module_lanes::execution("agents").unwrap()).unwrap()]).await;
        let alice = identity(&db.a, "manager-deadlines", "alice").await;
        context(&db.a, &alice, "operations").await;
        let actor = authority(&db.a, &alice, "operations").await;
        let definition = managed_definition(&db.a, &actor).await;
        assert_eq!(AgentRepository::new(db.a.clone()).next_managed_agent_delay("agents").await.unwrap(), None);
        let mut operations = Vec::new();
        for key in ["one", "two", "three"] {
            operations.push(AgentRepository::new(db.a.clone()).mutate_managed_agent(&actor, key, Uuid::now_v7(), None, plan(&definition, key), LIMITS).await.unwrap());
        }
        let first = AgentRepository::new(db.a.clone()).pending_managed_agent_operations("agents", 2, true, None).await.unwrap();
        let second = AgentRepository::new(db.a.clone()).pending_managed_agent_operations("agents", 2, true, Some(first[1].cursor())).await.unwrap();
        let mut ids = first.into_iter().chain(second).map(|op| op.id).collect::<Vec<_>>();
        ids.sort(); ids.dedup();
        assert_eq!(ids.len(), 3);
        let operation = &operations[0];
        let held = claim(&db.a, operation).await;
        let due = AgentRepository::new(db.a.clone()).next_managed_agent_delay("agents").await.unwrap().unwrap();
        assert!(due > std::time::Duration::from_secs(20) && due <= std::time::Duration::from_secs(30));
        assert_eq!(AgentRepository::new(db.a.clone()).next_managed_agent_delay("other").await.unwrap(), None);
        provision_through_workload(&db.a, &held).await;
        AgentRepository::new(db.a.clone()).release_managed_agent_claim(&held).await.unwrap();
        let startup = AgentRepository::new(db.a.clone()).next_managed_agent_delay("agents").await.unwrap().unwrap();
        assert!(startup > std::time::Duration::from_secs(590) && startup <= std::time::Duration::from_secs(600));
        db.a.client().query(include_str!("../queries/agent_management/instances/manager_deadlines_follow_claims_startup_and_draining_leases/statement_1.surql"))
            .bind(("instance", operation.instance.clone())).await.unwrap().check().unwrap();
        assert_eq!(AgentRepository::new(db.a.clone()).next_managed_agent_delay("agents").await.unwrap(), Some(std::time::Duration::ZERO));
        db.a.client().query(include_str!("../queries/agent_management/instances/manager_deadlines_follow_claims_startup_and_draining_leases/statement_2.surql"))
            .bind(("instance", operation.instance.clone())).await.unwrap().check().unwrap();
        db.a.client().query(include_str!("../queries/agent_management/instances/manager_deadlines_follow_claims_startup_and_draining_leases/statement_3.surql"))
            .bind(("tenant", actor.tenant.clone())).bind(("context", actor.work_context.clone()))
            .await.unwrap().check().unwrap();
        let drain = AgentRepository::new(db.a.clone()).next_managed_agent_delay("agents").await.unwrap().unwrap();
        assert!(drain > std::time::Duration::from_secs(5) && drain <= std::time::Duration::from_secs(10));
        db.a.client().query(include_str!("../queries/agent_management/instances/manager_deadlines_follow_claims_startup_and_draining_leases/statement_4.surql"))
            .bind(("tenant", actor.tenant.clone())).await.unwrap().check().unwrap();
        assert_eq!(AgentRepository::new(db.a.clone()).next_managed_agent_delay("agents").await.unwrap(), None);
    }).await.expect("manager scheduling qualification deadline");
}

#[tokio::test]
async fn managed_records_reject_missing_unknown_nested_fields_and_duplicate_identities() {
    use surrealdb::types::{SurrealValue, Value};
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let actor = identity(&db.a, "managed-closed", "alice").await;
        context(&db.a, &actor, "operations").await;
        let authority = authority(&db.a, &actor, "operations").await;
        let definition = managed_definition(&db.a, &authority).await;
        let repo = AgentRepository::new(db.a.clone());
        repo.mutate_managed_agent(
            &authority,
            "one",
            Uuid::now_v7(),
            None,
            plan(&definition, "one"),
            LIMITS,
        )
        .await
        .unwrap();
        let instance = repo.managed_agent(&authority, "one").await.unwrap();
        let mut response =
            db.a.client()
                .query(include_str!(
                    "../queries/agent_management/closed_records/read.surql"
                ))
                .bind(("record", instance.id.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let original: Value = response.take(0).unwrap();
        for case in 0..7 {
            let mut content = original.clone();
            let Value::Object(ref mut fields) = content else {
                unreachable!()
            };
            let field = if case < 3 { "identity" } else { "resources" };
            let Value::Object(nested) = fields.get_mut(field).unwrap() else {
                unreachable!()
            };
            match case {
                0 => {
                    nested.remove("client_id");
                }
                1 => {
                    nested.insert("unknown", true.into_value());
                }
                2 => {
                    nested.insert("membership", "invented".into_value());
                }
                3 => {
                    nested.remove("namespace");
                }
                4 => {
                    nested.insert("unknown", true.into_value());
                }
                5 => {
                    nested.insert("storage_gib", (-1_i64).into_value());
                }
                _ => {
                    fields.insert("public_key", malformed_public_key());
                }
            }
            assert!(
                db.a.client()
                    .query(include_str!(
                        "../queries/agent_management/closed_records/write.surql"
                    ))
                    .bind(("record", instance.id.clone()))
                    .bind(("content", content))
                    .await
                    .unwrap()
                    .check()
                    .is_err(),
                "closed managed record {case}"
            );
            let mut response =
                db.b.client()
                    .query(include_str!(
                        "../queries/agent_management/closed_records/read.surql"
                    ))
                    .bind(("record", instance.id.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            assert_eq!(response.take::<Value>(0).unwrap(), original);
        }
        let ManagedAgentMutation::Provision { mut plan } = plan(&definition, "two") else {
            unreachable!()
        };
        plan.identity.client_id = "managed-one".into();
        assert!(
            repo.mutate_managed_agent(
                &authority,
                "two",
                Uuid::now_v7(),
                None,
                ManagedAgentMutation::Provision { plan },
                LIMITS
            )
            .await
            .is_err()
        );
        assert_eq!(
            repo.managed_agents(&authority, None, 20)
                .await
                .unwrap()
                .len(),
            1
        );
    })
    .await
    .unwrap();
}
fn malformed_public_key() -> surrealdb::types::Value {
    use surrealdb::types::{Object, SurrealValue};
    let mut key = Object::new();
    key.insert("kid", "fixture".into_value());
    key.insert("n", "modulus".into_value());
    key.insert("e", "AQAB".into_value());
    key.insert("unknown", true.into_value());
    key.into_value()
}

#[tokio::test]
async fn registration_and_reconciliation_reject_stored_revision_fields_and_projection_mismatch() {
    use surrealdb::types::{SurrealValue, Value};
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "managed-decoder", "alice").await;
        context(&db.a, &alice, "operations").await;
        let actor = authority(&db.a, &alice, "operations").await;
        let definition = managed_definition(&db.a, &actor).await;
        let repo = AgentRepository::new(db.a.clone());
        let operation = repo
            .mutate_managed_agent(
                &actor,
                "one",
                Uuid::now_v7(),
                None,
                plan(&definition, "one"),
                LIMITS,
            )
            .await
            .unwrap();
        let claim = claim(&db.a, &operation).await;
        let revision = definition.published.unwrap();
        let mut response =
            db.a.client()
                .query(include_str!(
                    "../queries/agent_management/closed_records/read.surql"
                ))
                .bind(("record", revision.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
        let original: Value = response.take(0).unwrap();
        for case in 0..6 {
            let mut value = original.clone();
            let Value::Object(fields) = &mut value else {
                unreachable!()
            };
            match case {
                0..=2 => {
                    let Value::Object(content) = fields.get_mut("content").unwrap() else {
                        unreachable!()
                    };
                    if case == 0 {
                        content.insert("undeclared", true.into_value());
                    } else {
                        let field = if case == 1 { "budgets" } else { "execution" };
                        let Value::Object(nested) = content.get_mut(field).unwrap() else {
                            unreachable!()
                        };
                        nested.insert("undeclared", true.into_value());
                    }
                }
                3 => {
                    let Value::Object(model) = fields.get_mut("model").unwrap() else {
                        unreachable!()
                    };
                    model.insert("id", "different".into_value());
                }
                4 => {
                    fields.insert("tools", vec!["time__now".to_owned()].into_value());
                }
                _ => {
                    let Value::Object(execution) = fields.get_mut("execution").unwrap() else {
                        unreachable!()
                    };
                    execution.insert(
                        "parameters",
                        std::collections::BTreeMap::from([("arbitrary".to_owned(), true)])
                            .into_value(),
                    );
                }
            }
            db.a.client()
                .query(include_str!(
                    "../queries/agent_management/closed_records/replace_revision.surql"
                ))
                .bind(("record", revision.clone()))
                .bind(("content", value))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                matches!(
                    repo.managed_agent_registration(&"managed-one".parse().unwrap())
                        .await,
                    Err(AgentManagementError::Unavailable)
                        | Err(AgentManagementError::Invalid("stored revision projections"))
                ),
                "registration {case}"
            );
            assert!(
                matches!(
                    repo.managed_agent_reconciliation(&claim).await,
                    Err(AgentManagementError::Unavailable)
                        | Err(AgentManagementError::Invalid("stored revision projections"))
                ),
                "reconciliation {case}"
            );
            let mut denied = claim.clone();
            denied.fence += 1;
            assert!(
                matches!(
                    repo.managed_agent_reconciliation(&denied).await,
                    Err(AgentManagementError::Conflict)
                ),
                "claim admission precedes decode {case}"
            );
        }
        db.a.client()
            .query(include_str!(
                "../queries/agent_management/closed_records/replace_revision.surql"
            ))
            .bind(("record", revision))
            .bind(("content", original))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            repo.managed_agent_registration(&"managed-one".parse().unwrap())
                .await
                .unwrap()
                .is_some()
        );
        assert!(repo.managed_agent_reconciliation(&claim).await.is_ok());
    })
    .await
    .expect("managed stored revision qualification deadline");
}

#[tokio::test]
async fn identity_collisions_leave_principal_instance_and_capacity_unchanged() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        for static_client in [false, true] {
            let tenant = if static_client {
                "oauth-collision"
            } else {
                "principal-collision"
            };
            let alice = identity(&db.a, tenant, "alice").await;
            context(&db.a, &alice, "operations").await;
            let actor = authority(&db.a, &alice, "operations").await;
            let definition = managed_definition(&db.a, &actor).await;
            if static_client {
                db.a.client()
                    .query(include_str!(
                        "../queries/agent_management/identity_ownership/oauth_collision.surql"
                    ))
                    .bind(("tenant", alice.tenant_id.record_id()))
                    .bind(("client", "managed-one"))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            } else {
                identity(&db.a, tenant, "https://test.example/oauth#managed-one").await;
            }
            let snapshot = async |store: &PlatformStore| {
                let mut response = store
                    .client()
                    .query(include_str!(
                        "../queries/agent_management/identity_ownership/collision_snapshot.surql"
                    ))
                    .bind(("tenant", alice.tenant_id.record_id()))
                    .bind(("context", actor.work_context.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
                response.take::<Vec<i64>>(0).unwrap()
            };
            let before = snapshot(&db.a).await;
            assert_eq!(
                AgentRepository::new(db.b.clone())
                    .mutate_managed_agent(
                        &actor,
                        "one",
                        Uuid::now_v7(),
                        None,
                        plan(&definition, "one"),
                        LIMITS
                    )
                    .await,
                Err(AgentManagementError::Conflict)
            );
            assert_eq!(snapshot(&db.b).await, before);
            assert_eq!(&before[1..], &[0, 0, 0, 0]);
        }
    })
    .await
    .expect("Identity collision Agent fixture exceeded 120 seconds");
}
