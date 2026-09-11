#![allow(dead_code)] // Shared native fixture operations are scenario-specific.
#[path = "../../../platform/runtimes/computers/tests/native_support/docker_daemon.rs"]
mod docker_daemon;
#[path = "../../../platform/computers/storage/tests/native_support/service.rs"]
mod native_service_support;
#[path = "support/command_projection.rs"]
mod projection;
#[path = "../../../platform/runtimes/computers/tests/native_support/mod.rs"]
mod provider;
#[path = "support/signing.rs"]
mod signing;
#[path = "../../../platform/computers/tests/support/mod.rs"]
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;

use futures::FutureExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_service::{
    ArtifactService, ObjectStoreConfig, PlaneAuthenticator, SurrealArtifactRepository,
};
use veoveo_computer_execution::ExecutionRequest;
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputersStore, Reservation, api::*, commands::*, secrets::*,
};
use veoveo_computers_mcp::{CommandWorker, LifecycleWorker, RetainedHomes, WorkerStep};
use veoveo_computers_runtime::{Binding, ExecIntent, Phase};
use veoveo_mcp_contract::{
    ArtifactPlane, GATEWAY_INTERNAL_TOKEN_ISSUER, InvocationProvenance, PlaneCaller, ServerSlug,
    TokenIssuer,
};
use veoveo_task_runtime::{TaskRuntime, TaskStatus};

fn payload(script: &str, seconds: u32) -> CommandPayload {
    CommandPayload::new(
        ExecutionRequest::new(
            vec!["/bin/sh".into(), "-c".into(), script.into()],
            ".".into(),
            BTreeMap::from([(
                "PRIVATE_TOKEN".into(),
                "native-private-command-value".into(),
            )]),
            b"native-private-stdin\0\xff".to_vec(),
        )
        .unwrap(),
        AutomationExecutionLimits {
            maximum_seconds: seconds,
            maximum_output_bytes: 1024,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap()
}
#[allow(clippy::too_many_arguments)] // Explicit fixture authority and output boundaries.
async fn queue(
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: Uuid,
    grant: Uuid,
    payload: CommandPayload,
    keys: &ComputerKeyRing,
    plane: &HttpArtifactPlane,
    caller: &PlaneCaller,
) -> CommandOperation {
    let permission = store
        .authorize_automation_grant(actor, computer, grant, AutomationPermission::Execute)
        .await
        .unwrap();
    let command = store
        .queue_command(actor, permission, Uuid::now_v7(), &payload, keys)
        .await
        .unwrap();
    store.ensure_command_task(&command).await.unwrap();
    let request = command.output_capability_request(keys).unwrap().unwrap();
    let capability = plane
        .issue_write_capability(caller, &request)
        .await
        .unwrap();
    store
        .attach_command_output(&command, capability, keys)
        .await
        .unwrap()
}
async fn inspect(
    runtime: &veoveo_computers_runtime::OpenShellRuntime,
    binding: &Binding,
    script: &str,
) {
    let request = ExecIntent::new(
        vec!["/bin/sh".into(), "-eu".into(), "-c".into(), script.into()],
        "/sandbox/persistent".into(),
        5,
        4096,
        vec![],
    )
    .unwrap();
    assert_eq!(
        runtime
            .execute(binding, &request, |_| async { Ok(()) })
            .await
            .unwrap()
            .exit_code,
        0
    );
}

#[tokio::test]
#[ignore = "requires pinned provider/image; owns isolated database, Artifact service and retained compute fixture"]
async fn governed_command_worker_publishes_real_outputs_and_contains_revoked_execution() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    if std::env::var_os(docker_daemon::registry_relay::CHILD_ENV).is_some() {
        docker_daemon::registry_relay::child().await.unwrap();
        return;
    }
    if std::env::var_os("VEOVEO_STORAGE_SERVICE_CLEANUP").is_some() {
        native_service_support::cleanup();
        return;
    }
    for name in [
        "VEOVEO_COMPUTERS_NATIVE_ALLOCATOR",
        "VEOVEO_COMPUTERS_NATIVE_GATEWAY",
        "VEOVEO_COMPUTERS_NATIVE_SUPERVISOR",
    ] {
        let path = std::env::var_os(name).unwrap_or_else(|| panic!("requires {name}"));
        assert!(
            std::path::Path::new(&path).is_file(),
            "requires {name} binary"
        );
    }
    let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE").expect("qualified launcher image");
    // Publication pushes to the registry; native fixture enrollment also needs
    // this exact digest loaded in the host cache. Check before allocating a DB,
    // diagnostic certificates or daemon directories.
    docker_daemon::checked(
        docker_daemon::host().args(["image", "inspect", &image, "--format", "{{.Id}}"]),
    )
    .await;
    let _telemetry = veoveo_mcp_contract::init_server_telemetry(
        "veoveo-computers-native-command",
        "veoveo_computers_mcp=debug",
    )
    .unwrap();
    let db = support::TestDb::new().await;
    support::policy::install(&db.a, support::automation::control()).await;
    let selected = template::retained_template(image);
    let home = native_service_support::Fixture::start_with_template(
        Some(selected.clone()),
        "governed_command_worker_publishes_real_outputs_and_contains_revoked_execution",
    )
    .await;
    let gateway_ip = docker_daemon::checked(docker_daemon::host().args([
        "network",
        "inspect",
        "bridge",
        "--format",
        "{{(index .IPAM.Config 0).Gateway}}",
    ]))
    .await
    .parse()
    .unwrap();
    let mut provider = provider::Provider::start_on_compute_host(provider::ComputeHost {
        socket: home.docker_socket(),
        output: home.dir.clone(),
        namespace: "storage-fixture".into(),
        gateway_ip,
    })
    .await;
    let a = ComputersStore::new(db.a.clone(), Uuid::from_u128(100)).unwrap();
    let b = ComputersStore::new(db.b.clone(), Uuid::from_u128(100)).unwrap();
    a.install_capacity(
        None,
        CapacityPolicy {
            per_owner: 2,
            per_tenant: 4,
            provider: 4,
        },
    )
    .await
    .unwrap();
    a.install_automation_grant_policy(None, support::automation::POLICY)
        .await
        .unwrap();
    let owner =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let computer = a
        .reserve(
            owner.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: selected.fingerprint(),
            },
        )
        .await
        .unwrap();
    let homes = RetainedHomes::new(
        Uuid::from_u128(100),
        home.allocation_config(),
        std::slice::from_ref(&selected),
    )
    .await
    .unwrap();
    // Isolated fixture adoption: qualify all command/lifecycle paths against a
    // real replacement home. Durable product maintenance admission is separate.
    let initial = Binding::new(computer.computer_id, selected.fingerprint()).unwrap();
    let replacement_id = Uuid::now_v7();
    let binding =
        Binding::replacement(computer.computer_id, replacement_id, selected.fingerprint()).unwrap();
    let allocator = home.worker(home.provider).await;
    allocator.prepare(&initial).await.unwrap();
    allocator
        .abandon(Uuid::now_v7(), &initial, &binding)
        .await
        .unwrap();
    db.a.client()
        .query("UPDATE $computer SET replacement_instance_id=$instance;")
        .bind((
            "computer",
            surrealdb::types::RecordId::new(
                "computer",
                surrealdb::types::Uuid::from(computer.computer_id),
            ),
        ))
        .bind(("instance", replacement_id))
        .await
        .unwrap()
        .check()
        .unwrap();
    let tasks_a = TaskRuntime::new(db.a.clone(), "computers", "command-native-a");
    let tasks_b = TaskRuntime::new(db.b.clone(), "computers", "command-native-b");
    let lifecycle = LifecycleWorker::new(
        a.clone(),
        tasks_a.clone(),
        provider.runtime.clone(),
        vec![selected.clone()],
        homes,
    )
    .unwrap();
    let create = a
        .queue_operation(
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap(),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Create,
        )
        .await
        .unwrap();
    assert_eq!(
        lifecycle.step(create).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    assert_eq!(
        a.get(owner.owner(), computer.computer_id)
            .await
            .unwrap()
            .instance_id(),
        replacement_id
    );

    let signer = signing::Signing::new();
    let mut actor = support::owner("service");
    actor.authority.output_policy = support::automation::control().work_contexts[0]
        .output_policy
        .clone();
    actor.principal_kind = veoveo_platform_store::PrincipalKind::Service;
    actor.authority.provenance = InvocationProvenance::Automated;
    db.a.ensure_identity(
        actor.tenant_key(),
        &actor.principal_key,
        &actor.issuer,
        &actor.subject,
        actor.principal_kind,
    )
    .await
    .unwrap();
    let identity = support::identity(&actor);
    let agent = ComputerActor::from_verified(&identity).unwrap();
    let bearer = signer.identity(
        identity.clone(),
        "computers",
        chrono::Utc::now() + chrono::TimeDelta::minutes(10),
    );
    let auth = PlaneAuthenticator::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
        vec![ServerSlug::new("computers").unwrap()],
        signer.trust.clone(),
    );
    let caller = PlaneCaller {
        bearer_token: bearer,
        memberships: identity.actor.group_memberships(),
        identity,
    };
    let artifacts = ArtifactService::new(
        SurrealArtifactRepository::new(db.a.clone()),
        ObjectStoreConfig::Filesystem {
            root: home.dir.join("artifact-output"),
        }
        .build()
        .unwrap(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let artifact_server = tokio::spawn(async move {
        axum::serve(
            listener,
            veoveo_artifact_service::http::router(veoveo_artifact_service::http::AppState::new(
                artifacts, auth,
            )),
        )
        .await
        .unwrap();
    });
    let plane = HttpArtifactPlane::new(endpoint);
    let key_id = Uuid::from_u128(1);
    let keys = Arc::new(
        ComputerKeyRing::new(
            key_id,
            vec![ComputerSealingKey::new(key_id, zeroize::Zeroizing::new([23; 32])).unwrap()],
        )
        .unwrap(),
    );
    let worker_a = Arc::new(
        CommandWorker::new(
            a.clone(),
            tasks_a.clone(),
            provider.runtime.clone(),
            keys.clone(),
            plane.clone(),
            BTreeSet::from([selected.fingerprint()]),
        )
        .unwrap(),
    );
    let worker_b = Arc::new(
        CommandWorker::new(
            b.clone(),
            tasks_b.clone(),
            provider.runtime.clone(),
            keys.clone(),
            plane.clone(),
            BTreeSet::from([selected.fingerprint()]),
        )
        .unwrap(),
    );
    let (_health, health) = tokio::sync::watch::channel(veoveo_computers_mcp::CapacityHealth {
        availability: CapacityAvailability::Available,
        observed_at: Instant::now(),
    });
    let app = veoveo_computers_mcp::Application::new(
        a.clone(),
        tasks_a.clone(),
        veoveo_computers_mcp::Templates::new(
            vec![
                veoveo_computers_mcp::NamedTemplate::new("development".into(), selected.clone())
                    .unwrap(),
            ],
            Some(selected.fingerprint()),
        )
        .unwrap(),
        health,
        veoveo_computers_mcp::RuntimeAccess::unavailable(),
    )
    .unwrap()
    .with_execution(keys.clone(), plane.clone(), [selected.fingerprint()].into())
    .unwrap();
    let projection = projection::Projection::new(app, &signer).await;
    let owner_peer = projection
        .client(signer.identity(
            support::identity(owner.owner()),
            "computers",
            chrono::Utc::now() + chrono::TimeDelta::minutes(5),
        ))
        .await;
    let agent_peer = projection.client(caller.bearer_token.clone()).await;
    let rmcp::model::CallToolResponse::Complete(issued) = owner_peer
        .call_tool_once(projection::call(
            "grant_automation",
            &support::automation::input(computer.computer_id),
        ))
        .await
        .unwrap()
    else {
        panic!("grant result missing");
    };
    let grant = serde_json::from_value::<AutomationGrantResult>(issued.structured_content.unwrap())
        .unwrap()
        .grant;
    use base64::Engine;
    let input = ExecuteInput {
        computer_id: computer.computer_id, grant_id: grant.grant_id, request_id: Uuid::now_v7(),
        arguments: vec!["/bin/sh".into(), "-c".into(), "printf native-stdout; printf native-stderr >&2; printf x >> invocation-count; cat > received-stdin; printf '%s' \"$PRIVATE_TOKEN\" > received-env; exit 7".into()],
        directory: ".".into(), environment: BTreeMap::from([("PRIVATE_TOKEN".into(), "native-private-command-value".into())]),
        stdin: base64::engine::general_purpose::STANDARD.encode(b"native-private-stdin\0\xff"),
        limits: AutomationExecutionLimits { maximum_seconds: 30, maximum_output_bytes: 1024, on_interruption: AutomationInterruption::StopComputer },
    };
    let rmcp::model::CallToolResponse::Task(created) = agent_peer
        .call_tool_once(projection::call("execute", &input))
        .await
        .unwrap()
    else {
        panic!("command Task missing");
    };
    let id = created.task.task_id;
    let mut updates = agent_peer
        .listen(
            rmcp::model::SubscriptionFilter::builder()
                .task_id(&id)
                .build(),
        )
        .await
        .unwrap();
    let schedulers = projection::Schedulers::start([worker_a.clone(), worker_b.clone()]);
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let update = updates.next().await.unwrap().expect("Task stream ended");
            if let rmcp::model::ServerNotification::TaskStatusNotification(update) = update
                && update.params.task.status().is_terminal()
            {
                break;
            }
        }
    })
    .await
    .expect("public native command did not settle");
    let public = agent_peer
        .get_task(rmcp::model::GetTaskParams::new(&id))
        .await
        .unwrap();
    assert_eq!(public.task.status(), rmcp::model::TaskStatus::Completed);
    // Wait for the domain-first result's final retention acknowledgement before
    // ending the continuously running workers used by this journey.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !a.pending_commands(None, 100).await.unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    schedulers.stop().await;
    let rmcp::model::CallToolResponse::Task(retried) = agent_peer
        .call_tool_once(projection::call("execute", &input))
        .await
        .unwrap()
    else {
        panic!("completed retry lost its Task");
    };
    assert_eq!(retried.task.task_id, id);
    let task = tasks_a.get(&id).await.unwrap().unwrap();
    assert_eq!(
        task.status,
        TaskStatus::Succeeded,
        "command status: {:?}",
        task.status_message
    );
    let response: rmcp::model::CallToolResult =
        serde_json::from_value(task.result.unwrap()).unwrap();
    assert_eq!(response.is_error, Some(true));
    let result: ExecutionResult =
        serde_json::from_value(response.structured_content.unwrap()).unwrap();
    assert_eq!(result.exit_code, 7);
    let resource = agent_peer
        .read_resource(rmcp::model::ReadResourceRequestParams::new(String::from(
            result.result_uri,
        )))
        .await
        .unwrap();
    let rmcp::model::ResourceContents::TextResourceContents { text, .. } = &resource.contents[0]
    else {
        panic!("command result was not JSON");
    };
    assert_eq!(
        serde_json::from_str::<ExecutionResult>(text).unwrap(),
        result
    );
    for (output, expected) in [
        (result.stdout, b"native-stdout".as_slice()),
        (result.stderr, b"native-stderr".as_slice()),
    ] {
        let artifact = plane
            .resolve(&caller, &format!("artifact://{}", output.artifact_id))
            .await
            .unwrap();
        assert_eq!(artifact.bytes, expected);
        assert_eq!(
            artifact.metadata.compliance.owner.as_ref(),
            Some(&actor.authority.output_policy.owner)
        );
        assert_eq!(
            artifact
                .metadata
                .compliance
                .provenance
                .unwrap()
                .producer
                .as_str(),
            actor.principal_key
        );
    }
    inspect(&provider.runtime, &binding, "test \"$(cat invocation-count)\" = x; test \"$(cat received-env)\" = native-private-command-value; test $(wc -c < received-stdin) = 22").await;

    let command = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(
            "printf active > active-command; sleep 120; printf forbidden > after-revocation",
            30,
        ),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let id = command.task_id().to_string();
    let worker = worker_a.clone();
    let job = tokio::spawn(async move { worker.step(command).boxed().await });
    // One native fixture observer waits for the command's start marker. This is
    // neither provider completion polling nor a substitute for Stop evidence.
    inspect(
        &provider.runtime,
        &binding,
        "while ! test -f active-command; do sleep 0.02; done",
    )
    .await;
    let revoked = Instant::now();
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer.computer_id,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    // Containment starts only after the foreground I/O future has ended. The
    // database journal proves that boundary independently of Docker's Stop grace.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let operations = b.pending_commands(None, 100).await.unwrap();
            if operations
                .iter()
                .any(|op| op.task_id().to_string() == id && op.stage() != CommandStage::Dispatched)
            {
                break;
            }
            if tasks_a.get(&id).await.unwrap().unwrap().is_terminal() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let authority_cutoff = revoked.elapsed();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(60), job)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        WorkerStep::Settled
    );
    assert!(authority_cutoff < Duration::from_secs(5));
    eprintln!(
        "Native revocation: I/O containment began in {} ms; positive Stop settled in {} ms",
        authority_cutoff.as_millis(),
        revoked.elapsed().as_millis()
    );
    assert_eq!(
        a.get(owner.owner(), computer.computer_id)
            .await
            .unwrap()
            .phase,
        ComputerPhase::Stopped
    );
    assert_eq!(
        tasks_a.get(&id).await.unwrap().unwrap().status,
        TaskStatus::Failed
    );
    assert!(matches!(
        provider.runtime.get(&binding).await.unwrap().unwrap().phase,
        Phase::Stopped
    ));
    let start = a
        .queue_operation(
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap(),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Start,
        )
        .await
        .unwrap();
    assert_eq!(
        lifecycle.step(start).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    inspect(
        &provider.runtime,
        &binding,
        "test -f active-command; test ! -e after-revocation; test \"$(cat invocation-count)\" = x",
    )
    .await;
    // Task cancellation uses the same containment journal and preserved home.
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer.computer_id))
        .await
        .unwrap();
    let command = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(
            "printf active > active-cancel; sleep 120; printf forbidden > after-cancel",
            30,
        ),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let cancelled_id = command.task_id().to_string();
    let worker = worker_a.clone();
    let job = tokio::spawn(async move { worker.step(command).boxed().await });
    inspect(
        &provider.runtime,
        &binding,
        "while ! test -f active-cancel; do sleep 0.02; done",
    )
    .await;
    tasks_b.cancel(&cancelled_id).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(60), job)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        WorkerStep::Settled
    );
    assert_eq!(
        tasks_b.get(&cancelled_id).await.unwrap().unwrap().status,
        TaskStatus::Cancelled
    );
    let start = a
        .queue_operation(
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap(),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Start,
        )
        .await
        .unwrap();
    assert_eq!(
        lifecycle.step(start).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    inspect(
        &provider.runtime,
        &binding,
        "test -f active-cancel; test ! -e after-cancel",
    )
    .await;

    // Drop a committed dispatch receipt before submitting native work. A new
    // worker cannot infer that the command did not run, and contains without replay.
    let command = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload("printf forbidden > replayed-command", 30),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let lost_id = command.task_id().to_string();
    let claim = tasks_a
        .claim_observation(&lost_id, Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_command_dispatch(&claim, &keys).await.unwrap());
    let dispatched = a.command_for_claim(&claim).await.unwrap();
    tasks_a.release_observation(&claim).await.unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(60), worker_b.step(dispatched).boxed())
            .await
            .unwrap()
            .unwrap(),
        WorkerStep::Settled
    );
    let lost = tasks_a.get(&lost_id).await.unwrap().unwrap();
    assert_eq!(lost.status, TaskStatus::Failed);
    assert_eq!(lost.error.unwrap().code, "execution_unknown");
    let start = a
        .queue_operation(
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap(),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Start,
        )
        .await
        .unwrap();
    assert_eq!(
        lifecycle.step(start).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    inspect(&provider.runtime, &binding, "test ! -e replayed-command; test ! -e after-cancel; test ! -e after-revocation; test \"$(cat invocation-count)\" = x").await;
    let stop = a
        .queue_operation(
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap(),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Stop,
        )
        .await
        .unwrap();
    assert_eq!(
        lifecycle.step(stop).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    // Retire the exact stopped provider resource, then require the allocator's
    // independent physical handoff before any fresh instance may use its files.
    let stopped = provider.runtime.get(&binding).await.unwrap().unwrap();
    let captured = provider
        .runtime
        .capture_replacement_policy(&binding, &selected)
        .await
        .unwrap();
    assert_eq!(captured.fingerprint().len(), 64);
    let _acknowledgement = provider.runtime.retire(&binding, &stopped).await.unwrap();
    let absent = tokio::time::timeout(Duration::from_secs(10), async {
        for attempt in 0..8 {
            if provider.runtime.get(&binding).await.unwrap().is_none() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100 << attempt.min(4))).await;
        }
        false
    })
    .await
    .unwrap();
    assert!(
        absent,
        "retired provider resource did not disappear within its observation budget"
    );
    let replacement =
        Binding::replacement(computer.computer_id, Uuid::now_v7(), selected.fingerprint()).unwrap();
    allocator
        .handoff(Uuid::now_v7(), &binding, &replacement, &stopped.sandbox_id)
        .await
        .unwrap();
    let checkpoint = veoveo_computers_runtime::LifecycleCheckpoint::create(
        provider.runtime.provider_instance_id(),
        Uuid::now_v7(),
        replacement.clone(),
    )
    .unwrap();
    let created = provider
        .runtime
        .create(&replacement, &selected)
        .await
        .unwrap();
    provider
        .runtime
        .wait_for_lifecycle(&checkpoint, &created, Duration::from_secs(30))
        .await
        .unwrap();
    inspect(
        &provider.runtime,
        &replacement,
        "test \"$(cat invocation-count)\" = x; test ! -e replayed-command",
    )
    .await;
    assert!(allocator.restore(&binding).await.is_err());
    // This base-policy fixture does not qualify restoring captured dynamic grants
    // after retirement; that requires a durable protected policy checkpoint.
    artifact_server.abort();
    let _ = artifact_server.await;
    provider.assert_running();
    drop(worker_a);
    drop(worker_b);
    drop(lifecycle);
    drop(provider);
    home.finish(None).await;
}
