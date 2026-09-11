#![allow(dead_code)] // Selected native fixture operations are scenario-specific.
#[path = "../../../platform/runtimes/computers/tests/native_support/docker_daemon.rs"]
mod docker_daemon;
#[path = "../../../platform/computers/storage/tests/native_support/service.rs"]
mod native_service_support;
#[path = "../../../platform/runtimes/computers/tests/native_support/mod.rs"]
mod provider;
#[path = "support/signing.rs"]
mod signing;
#[path = "../../../platform/computers/tests/support/mod.rs"]
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;
use futures::FutureExt;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use uuid::Uuid;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_service::{
    ArtifactService, ObjectStoreConfig, PlaneAuthenticator, SurrealArtifactRepository,
};
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputersStore, Reservation, api::*, files::*, secrets::*,
};
use veoveo_computers_mcp::{FileWorker, LifecycleWorker, RetainedHomes, WorkerStep};
use veoveo_computers_runtime::{Binding, ExecIntent, Phase};
use veoveo_mcp_contract::{
    ArtifactPlane, GATEWAY_INTERNAL_TOKEN_ISSUER, InvocationProvenance, PlaneCaller,
    PutArtifactRequest, ServerSlug, TokenIssuer,
};
use veoveo_task_runtime::{TaskRuntime, TaskStatus};

fn payload(transfer: FileTransfer) -> FileTransferPayload {
    FileTransferPayload::new(
        transfer,
        FileTransferLimits {
            maximum_bytes: 2 * 1024 * 1024,
            maximum_seconds: 30,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap()
}
fn path(value: &str) -> RetainedFilePath {
    RetainedFilePath::try_from(value.to_owned()).unwrap()
}
#[allow(clippy::too_many_arguments)] // Fixture separates caller, grant and Artifact authority.
async fn queue(
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: Uuid,
    grant: Uuid,
    payload: FileTransferPayload,
    keys: &ComputerKeyRing,
    plane: &HttpArtifactPlane,
    caller: &PlaneCaller,
) -> FileOperation {
    let authority = store
        .file_transfer_authority(actor, computer, Some(grant))
        .await
        .unwrap();
    let operation = store
        .queue_file_transfer(actor, authority, Uuid::now_v7(), &payload, keys)
        .await
        .unwrap();
    store.ensure_file_task(&operation).await.unwrap();
    let access = match operation.file_capability_request(keys).unwrap().unwrap() {
        FileCapabilityRequest::Import(request) => FileTransferAccess::Import {
            capability: plane.issue_read_capability(caller, &request).await.unwrap(),
        },
        FileCapabilityRequest::Export(request) => FileTransferAccess::Export {
            capability: plane
                .issue_write_capability(caller, &request)
                .await
                .unwrap(),
        },
    };
    store
        .attach_file_artifact_access(&operation, access, keys)
        .await
        .unwrap()
}
async fn task_result(tasks: &TaskRuntime, id: Uuid) -> FileTransferResult {
    let task = tasks.get(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(task.status, TaskStatus::Succeeded);
    let tool: rmcp::model::CallToolResult = serde_json::from_value(task.result.unwrap()).unwrap();
    assert_eq!(tool.is_error, Some(false));
    serde_json::from_value(tool.structured_content.unwrap()).unwrap()
}
#[tokio::test]
#[ignore = "requires qualified provider, template and allocator; owns an isolated retained home and Artifact service"]
async fn governed_file_worker_moves_real_artifacts_and_contains_lost_attempts() {
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
        "veoveo-computers-native-files",
        "veoveo_computers_mcp=debug",
    )
    .unwrap();
    let db = support::TestDb::new().await;
    let mut control = support::automation::control();
    let file_tool = veoveo_mcp_contract::LocalToolName::new("transfer_file").unwrap();
    control.servers[0].tools.push(file_tool.clone());
    control.policies[0].rules[0].tools.insert(file_tool);
    support::policy::install(&db.a, control).await;
    let selected = template::retained_template(image);
    let home = native_service_support::Fixture::start_with_templates(
        vec![selected.clone()],
        "governed_file_worker_moves_real_artifacts_and_contains_lost_attempts",
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
    a.install_automation_grant_policy(
        None,
        veoveo_computers::automation_grants::AutomationGrantPolicy {
            maximum_output_bytes: 4 * 1024 * 1024,
            ..support::automation::POLICY
        },
    )
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
    // Isolated fixture adoption: qualify file and lifecycle paths against a
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
    let tasks_a = TaskRuntime::new(db.a.clone(), "computers", "file-native-a");
    let tasks_b = TaskRuntime::new(db.b.clone(), "computers", "file-native-b");
    let lifecycle = LifecycleWorker::new(
        a.clone(),
        tasks_a.clone(),
        provider.runtime.clone(),
        vec![selected.clone()],
        homes.clone(),
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
    actor.data_labels.insert("cui".into());
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
    // The gateway normally materializes this current Work Context. Read
    // capabilities require that projection as well as the signed caller.
    let context_id = veoveo_platform_store::deterministic_work_context_id("test", "computers-test")
        .unwrap()
        .record_id();
    let context = veoveo_platform_store::WorkContextRecord {
        id: context_id.clone(),
        tenant: veoveo_platform_store::deterministic_tenant_id("test")
            .unwrap()
            .record_id(),
        context_key: "computers-test".into(),
        title: "Computers tests".into(),
        policy_revision: "test-1".into(),
        output_policy: veoveo_platform_store::WorkContextOutputPolicyRecord {
            owner_kind: veoveo_platform_store::ArtifactGrantSubjectKind::Principal,
            owner_key: "https://computers.test#alice".into(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![veoveo_platform_store::WorkContextMembershipRuleRecord {
            level: veoveo_platform_store::WorkContextMembershipLevel::Contributor,
            principals: vec![],
            groups: vec![],
            roles: vec![],
            oauth_clients: vec!["console".into(), "service".into(), "delegated".into()],
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _: Option<veoveo_platform_store::WorkContextRecord> =
        db.a.client()
            .create(context_id)
            .content(context)
            .await
            .unwrap();
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
    let worker = FileWorker::new(
        a.clone(),
        tasks_a.clone(),
        provider.runtime.clone(),
        keys.clone(),
        plane.clone(),
        BTreeSet::from([selected.fingerprint()]),
    )
    .unwrap();
    let successor = FileWorker::new(
        b.clone(),
        tasks_b,
        provider.runtime.clone(),
        keys.clone(),
        plane.clone(),
        BTreeSet::from([selected.fingerprint()]),
    )
    .unwrap();
    let mut grant_input = support::automation::input(computer.computer_id);
    grant_input
        .execution_limits
        .as_mut()
        .unwrap()
        .maximum_output_bytes = 2 * 1024 * 1024;
    let grant = a
        .issue_automation_grant(&owner, &grant_input)
        .await
        .unwrap();
    let bytes: Vec<u8> = (0..1_000_003).map(|i| (i % 251) as u8).collect();
    let artifact = plane
        .put(
            &caller,
            PutArtifactRequest {
                filename: Some("source.bin".into()),
                mime_type: Some("application/octet-stream".into()),
                ..Default::default()
            },
            bytes.clone(),
        )
        .await
        .unwrap();
    let import = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(FileTransfer::Import {
            artifact_id: artifact.artifact_id.as_uuid(),
            path: path("binary source.tar"),
        }),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let import_id = import.transfer_id();
    assert_eq!(
        worker.step(import).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    let imported = task_result(&tasks_a, import_id).await;
    assert_eq!(imported.artifact_id, artifact.artifact_id.as_uuid());
    assert_eq!(imported.bytes, bytes.len() as u64);
    let export = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(FileTransfer::Export {
            path: path("binary source.tar"),
            filename: "download.bin".into(),
            media_type: "application/octet-stream".into(),
        }),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let export_id = export.transfer_id();
    assert_eq!(
        worker.step(export).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    let exported = task_result(&tasks_a, export_id).await;
    assert_eq!(exported.sha256, imported.sha256);
    assert_ne!(exported.artifact_id, imported.artifact_id);
    let actual = plane
        .get(
            &caller,
            &veoveo_mcp_contract::ArtifactId::parse(exported.artifact_id.to_string()).unwrap(),
            veoveo_mcp_contract::AccessLevel::Read,
        )
        .await
        .unwrap();
    assert_eq!(actual.bytes, bytes);
    assert_eq!(actual.metadata.filename.as_deref(), Some("download.bin"));
    let duplicate = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(FileTransfer::Import {
            artifact_id: artifact.artifact_id.as_uuid(),
            path: path("binary source.tar"),
        }),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let duplicate_id = duplicate.transfer_id();
    assert_eq!(
        worker.step(duplicate).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    let duplicate_task = tasks_a
        .get(&duplicate_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(duplicate_task.status, TaskStatus::Failed);
    assert_eq!(duplicate_task.error.unwrap().code, "destination_exists");
    assert!(provider.runtime.get(&binding).await.unwrap().unwrap().phase == Phase::Ready);

    // Caller clearance alone cannot raise a retained Computer's data floor.
    let restricted = plane
        .put(
            &caller,
            PutArtifactRequest {
                data_labels: BTreeSet::from(
                    [veoveo_mcp_contract::DataLabelId::new("cui").unwrap()],
                ),
                ..Default::default()
            },
            b"sensitive fixture".to_vec(),
        )
        .await
        .unwrap();
    let denied = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(FileTransfer::Import {
            artifact_id: restricted.artifact_id.as_uuid(),
            path: path("restricted-import"),
        }),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let denied_id = denied.transfer_id();
    assert_eq!(
        worker.step(denied).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    let denied_task = tasks_a.get(&denied_id.to_string()).await.unwrap().unwrap();
    assert_eq!(denied_task.error.unwrap().code, "artifact_unavailable");
    assert!(provider.runtime.get(&binding).await.unwrap().unwrap().phase == Phase::Ready);
    let cancelled = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(FileTransfer::Import {
            artifact_id: artifact.artifact_id.as_uuid(),
            path: path("cancelled-import"),
        }),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let cancelled_id = cancelled.transfer_id();
    tasks_a.cancel(&cancelled_id.to_string()).await.unwrap();
    assert_eq!(
        worker.step(cancelled).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    assert_eq!(
        tasks_a
            .get(&cancelled_id.to_string())
            .await
            .unwrap()
            .unwrap()
            .status,
        TaskStatus::Cancelled
    );

    // Losing an original dispatch receipt cannot turn into a repeated import.
    let lost = queue(
        &a,
        &agent,
        computer.computer_id,
        grant.grant_id,
        payload(FileTransfer::Import {
            artifact_id: artifact.artifact_id.as_uuid(),
            path: path("must-not-be-replayed"),
        }),
        &keys,
        &plane,
        &caller,
    )
    .await;
    let claim = tasks_a
        .claim_observation(&lost.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_file_dispatch(&claim, &keys).await.unwrap());
    tasks_a.release_observation(&claim).await.unwrap();
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer.computer_id,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        successor.step(lost).boxed().await.unwrap(),
        WorkerStep::Settled
    );
    assert!(provider.runtime.get(&binding).await.unwrap().unwrap().phase == Phase::Stopped);
    let stopped = a.get(owner.owner(), computer.computer_id).await.unwrap();
    assert_eq!(stopped.phase, ComputerPhase::Stopped);
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
    let inspect=ExecIntent::new(vec!["/bin/sh".into(),"-eu".into(),"-c".into(),"test ! -e must-not-be-replayed; test ! -e restricted-import; test ! -e cancelled-import; test -f 'binary source.tar'; test $(wc -c < 'binary source.tar') = 1000003".into()],"/sandbox/persistent".into(),5,4096,vec![]).unwrap();
    assert_eq!(
        provider
            .runtime
            .execute(&binding, &inspect, |_| async { Ok(()) })
            .await
            .unwrap()
            .exit_code,
        0
    );
    assert!(
        b.pending_file_transfers(None, 100)
            .await
            .unwrap()
            .is_empty()
    );
    artifact_server.abort();
    let _ = artifact_server.await;
    provider.assert_running();
    drop(worker);
    drop(successor);
    drop(lifecycle);
    drop(provider);
    home.finish(None).await;
}
