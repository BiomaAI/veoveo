use super::*;
use veoveo_agent_runtime::ManagedRuntimeBinding;
use veoveo_platform_store::agent_management::{instances::*, *};

const LIMITS: ManagedAgentLimits = ManagedAgentLimits {
    instances: 4,
    storage_gib: 8,
};

async fn admit(store: &PlatformStore) -> (AgentCatalogAuthority, ManagedAgentOperation) {
    let human = store
        .ensure_identity(
            "integration",
            "alice",
            "https://identity.test",
            "alice",
            PrincipalKind::User,
        )
        .await
        .unwrap();
    let context = deterministic_work_context_id("integration", "integration-mission").unwrap();
    store.client().query("CREATE ONLY $context SET tenant = $tenant, context_key = 'integration-mission', title = 'Mission', policy_revision = 'r1', memberships = [], output_policy = {owner_kind: 'principal', owner_key: 'alice', initial_grants: [], data_labels: []};")
        .bind(("context", context.record_id())).bind(("tenant", human.tenant_id.record_id())).await.unwrap().check().unwrap();
    let version = store
        .artifact_read_context_version("integration", "integration-mission")
        .await
        .unwrap()
        .unwrap();
    let authority = AgentCatalogAuthority::new(
        human.tenant_id,
        context,
        human.principal_id,
        version.digest,
        StoreMembership::Contributor,
        false,
        10,
    );
    let content = AgentContent {
        model: AgentModelReference {
            id: "approved".into(),
            revision: "a".repeat(64),
        },
        instructions: "Literal ${PRIVATE_KEY}".into(),
        tools: vec!["time__resolve_time".into()],
        budgets: AgentBudgets {
            max_output_tokens: 1024,
            max_completion_calls: 2,
            max_tool_calls: 2,
            deadline_seconds: 60,
        },
        execution: AgentExecution::Managed {
            template: "pilot".into(),
            template_revision: "b".repeat(64),
            parameters: Default::default(),
            resource_subscriptions: vec![],
        },
    };
    let draft = store
        .mutate_agent_definition(
            &authority,
            "pilot",
            Uuid::now_v7(),
            None,
            AgentDefinitionMutation::Create {
                name: "Pilot".into(),
                description: "Test".into(),
                content,
            },
        )
        .await
        .unwrap();
    let definition = store
        .mutate_agent_definition(
            &authority,
            "pilot",
            Uuid::now_v7(),
            Some(draft.revision),
            AgentDefinitionMutation::Publish {
                digest: draft.draft_digest,
                audience: vec![AgentPublicationContext {
                    work_context: authority.work_context.clone(),
                    context_digest: authority.context_digest.clone(),
                }],
            },
        )
        .await
        .unwrap();
    let operation = store
        .mutate_managed_agent(
            &authority,
            "durability-agent",
            Uuid::now_v7(),
            None,
            ManagedAgentMutation::Provision {
                plan: Box::new(ManagedAgentProvision {
                    name: "Managed durability".into(),
                    definition_key: definition.key,
                    revision: definition.draft_digest,
                    identity: ManagedAgentIdentity {
                        client_id: "managed-durability".into(),
                        issuer: "https://test.example/oauth".into(),
                        authorization_server: "gateway".into(),
                        profile: "integration".into(),
                        resource: "https://test.example/mcp/integration".into(),
                        scopes: vec!["mcp:read".into()],
                        roles: vec!["pilot".into()],
                        membership: StoreMembership::Contributor,
                    },
                    resources: ManagedAgentResources {
                        namespace: "agents".into(),
                        workload: "durability".into(),
                        credential_secret: "durability-key".into(),
                        volume_claim: "durability-memory".into(),
                        template_config_map: "pilot-template".into(),
                        image: format!("registry.test/kernel@sha256:{}", "c".repeat(64)),
                        storage_gib: 2,
                    },
                }),
            },
            LIMITS,
        )
        .await
        .unwrap();
    (authority, operation)
}

async fn converge(store: &PlatformStore, op: &ManagedAgentOperation, terminal: ManagedAgentPhase) {
    let owner = Uuid::now_v7();
    let claim = store
        .claim_managed_agent_operation(op.id.clone(), owner)
        .await
        .unwrap()
        .unwrap()
        .claim(owner)
        .unwrap();
    store
        .observe_managed_agent(&claim, ManagedAgentPhase::Credentials, None)
        .await
        .unwrap();
    store
        .register_managed_agent_key(
            &claim,
            ManagedAgentPublicKey {
                kid: "key".into(),
                n: "public-modulus".into(),
                e: "AQAB".into(),
            },
        )
        .await
        .unwrap();
    for phase in [
        ManagedAgentPhase::Storage,
        ManagedAgentPhase::Draining,
        terminal,
    ] {
        store
            .observe_managed_agent(&claim, phase, None)
            .await
            .unwrap();
    }
    if terminal == ManagedAgentPhase::Workload {
        store
            .observe_managed_agent(&claim, ManagedAgentPhase::Ready, None)
            .await
            .unwrap();
    }
}

fn completed() -> EpisodeCompletion {
    EpisodeCompletion {
        state: AgentEpisodeState::Completed,
        final_output: "late model response".into(),
        summary: None,
        input_tokens: 1,
        output_tokens: 1,
        completion_calls: 1,
        tool_calls: 0,
        error: None,
    }
}

#[tokio::test]
async fn atomic_admission_stop_and_restart_preserve_terminal_wakes() {
    let db = database::TestDb::new().await;
    let (authority, provision) = admit(&db.a).await;
    converge(&db.a, &provision, ManagedAgentPhase::Workload).await;
    let runtime = AgentRuntime::register(
        db.b.clone(),
        AgentSpec {
            tenant_key: "integration".into(),
            agent_key: "durability-agent".into(),
            display_name: "Pilot".into(),
            profile: "integration".into(),
            authority: agent_authority_record(),
            manifest: OpenObject::default(),
            memory_database: "memory.duckdb".into(),
        },
        AgentInstanceId::new(),
    )
    .await
    .unwrap();
    runtime
        .acquire_lease(Duration::from_secs(120))
        .await
        .unwrap()
        .unwrap();
    assert!(runtime.start_episode("unbound process").await.is_err());
    let runtime = runtime
        .with_managed_binding(ManagedRuntimeBinding {
            instance: provision.instance.clone(),
            generation: 1,
        })
        .unwrap();
    let pod = Uuid::now_v7();
    runtime.managed_ready(pod).await.unwrap();
    assert_eq!(
        runtime
            .agent_record()
            .await
            .unwrap()
            .managed_ready
            .unwrap()
            .pod_uid,
        pod
    );
    let wake = runtime
        .enqueue_wake(NewWake::now(
            WakeKind::OperatorMessage,
            None,
            OpenObject::default(),
        ))
        .await
        .unwrap();
    runtime.claim_wakes(10, DEFAULT_CLAIM_LEASE).await.unwrap();
    let episode = runtime.start_episode("operator work").await.unwrap();
    let binding = episode.managed.as_ref().expect("atomic provenance");
    assert_eq!((binding.generation, binding.epoch), (1, 1));
    let watcher_runtime = runtime.clone();
    let watcher_binding = binding.clone();
    let mut revoked = tokio::spawn(async move {
        watcher_runtime
            .wait_for_managed_dispatch_revocation(&watcher_binding)
            .await
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut revoked)
            .await
            .is_err()
    );
    assert_eq!(
        runtime
            .episode_record(episode.episode_id)
            .await
            .unwrap()
            .managed
            .as_ref(),
        Some(binding)
    );
    assert!(runtime.start_episode("overlapping work").await.is_err());
    let pause =
        db.a.mutate_managed_agent(
            &authority,
            "durability-agent",
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
        db.a.managed_agent_dispatch(provision.instance.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert!(runtime.start_episode("after pause").await.is_err());
    assert_eq!(
        runtime.managed_scheduler_mode().await.unwrap(),
        veoveo_agent_runtime::ManagedSchedulerMode::Paused
    );
    let retained = runtime
        .enqueue_wake(NewWake::now(
            WakeKind::OperatorMessage,
            None,
            OpenObject::default(),
        ))
        .await
        .unwrap();
    assert!(
        runtime
            .claim_wakes(10, DEFAULT_CLAIM_LEASE)
            .await
            .unwrap()
            .is_empty()
    );
    let stop =
        db.a.mutate_managed_agent(
            &authority,
            "durability-agent",
            Uuid::now_v7(),
            Some(2),
            ManagedAgentMutation::Stop,
            LIMITS,
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), revoked)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        !db.a
            .managed_agent_dispatch(provision.instance.clone(), 1, 1)
            .await
            .unwrap()
    );
    assert_eq!(
        runtime
            .episode_record(episode.episode_id)
            .await
            .unwrap()
            .state,
        AgentEpisodeState::Stopped
    );
    runtime
        .complete_episode(episode.episode_id, completed(), &[wake])
        .await
        .unwrap();
    let terminal = runtime.episode_record(episode.episode_id).await.unwrap();
    assert_eq!(terminal.state, AgentEpisodeState::Stopped);
    assert_eq!(
        terminal.final_output.as_deref(),
        Some("Stopped by an authorized operator.")
    );
    let wake_row: Option<WakeRecord> = db.a.client().select(wake.record_id()).await.unwrap();
    assert_eq!(wake_row.unwrap().state, WakeState::Acked);
    assert!(
        runtime
            .claim_wakes(10, DEFAULT_CLAIM_LEASE)
            .await
            .unwrap()
            .is_empty()
    );
    // A paused kernel keeps its lease for Task observation without model admission.
    converge(&db.a, &stop, ManagedAgentPhase::Paused).await;
    assert_eq!(
        db.a.managed_agent_operation(&authority, pause.id)
            .await
            .unwrap()
            .phase,
        ManagedAgentPhase::Superseded
    );
    runtime.release_lease().await.unwrap();
    let resume =
        db.a.mutate_managed_agent(
            &authority,
            "durability-agent",
            Uuid::now_v7(),
            Some(3),
            ManagedAgentMutation::State {
                desired: ManagedAgentDesired::Running,
            },
            LIMITS,
        )
        .await
        .unwrap();
    converge(&db.a, &resume, ManagedAgentPhase::Workload).await;
    runtime
        .acquire_lease(Duration::from_secs(120))
        .await
        .unwrap()
        .unwrap();
    assert!(
        runtime
            .agent_record()
            .await
            .unwrap()
            .managed_ready
            .is_none()
    );
    assert!(runtime.start_episode("stale generation").await.is_err());
    let current = runtime
        .with_managed_binding(ManagedRuntimeBinding {
            instance: provision.instance,
            generation: 4,
        })
        .unwrap();
    let pending = current.claim_wakes(10, DEFAULT_CLAIM_LEASE).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].wake_id, retained);
    let next = current.start_episode("new work").await.unwrap();
    assert_eq!(next.managed.unwrap().epoch, 2);
    assert!(
        current
            .claim_wakes(10, DEFAULT_CLAIM_LEASE)
            .await
            .unwrap()
            .is_empty()
    );
    current
        .complete_episode(next.episode_id, completed(), &[retained])
        .await
        .unwrap();
}
