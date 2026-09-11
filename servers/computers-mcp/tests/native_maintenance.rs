#![allow(dead_code)] // Shared native fixture operations are scenario-specific.
#[path = "../../../platform/runtimes/computers/tests/native_support/docker_daemon.rs"]
mod docker_daemon;
#[path = "support/maintenance_initial_failure.rs"]
mod maintenance_initial_failure;
#[path = "support/maintenance_policy.rs"]
mod maintenance_policy;
#[path = "../../../platform/computers/storage/tests/native_support/service.rs"]
mod native_service_support;
#[path = "../../../platform/runtimes/computers/tests/native_support/mod.rs"]
mod provider;
#[path = "support/schedulers.rs"]
mod schedulers;
#[path = "../../../platform/computers/tests/support/mod.rs"]
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;

use futures::FutureExt;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputersStore, Reservation,
    api::{Action, ComputerPhase, ResumeUpdateInput},
    maintenance::*,
    secrets::*,
};
use veoveo_computers_mcp::{
    LifecycleWorker, MaintenanceProfiles, MaintenanceTransition, MaintenanceWorker, RetainedHomes,
    WorkerStep, config::Configuration,
};
use veoveo_computers_runtime::{Binding, ExecIntent, OpenShellRuntime};
use veoveo_mcp_contract::LocalToolName;
use veoveo_task_runtime::{TaskRuntime, TaskStatus};

const SCENARIO: &str = "installation_templates_upgrade_recover_and_rollback_retained_home";

async fn shell(runtime: &OpenShellRuntime, binding: &Binding, script: &str) {
    let intent = ExecIntent::new(
        vec!["/bin/sh".into(), "-eu".into(), "-c".into(), script.into()],
        "/sandbox/persistent".into(),
        10,
        4096,
        vec![],
    )
    .unwrap();
    assert_eq!(
        runtime
            .execute(binding, &intent, |_| async { Ok(()) })
            .await
            .unwrap()
            .exit_code,
        0
    );
}

async fn settle(
    store: &ComputersStore,
    tasks: &TaskRuntime,
    actor: &ComputerActor,
    operation: &MaintenanceOperation,
) -> MaintenanceOperation {
    tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let current = store
                .maintenance(actor.owner(), operation.operation_id)
                .await
                .unwrap();
            assert_ne!(
                current.stage,
                MaintenanceStage::RecoveryRequired,
                "maintenance paused at {:?}: {:?}",
                current.steps().last().map(|step| step.step),
                current.recovery()
            );
            let task = tasks
                .get(&operation.task_id().to_string())
                .await
                .unwrap()
                .unwrap();
            if task.is_terminal()
                && store
                    .pending_maintenance(None, 100)
                    .await
                    .unwrap()
                    .is_empty()
            {
                assert_eq!(task.status, TaskStatus::Succeeded);
                assert_eq!(current.stage, MaintenanceStage::Succeeded);
                return current;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("native maintenance exceeded its deadline")
}

#[tokio::test]
#[ignore = "requires exact installation template catalog, cached images and pinned native binaries; owns isolated provider/storage/database"]
async fn installation_templates_upgrade_recover_and_rollback_retained_home() {
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
        assert!(
            Path::new(&std::env::var_os(name).unwrap_or_else(|| panic!("requires {name}")))
                .is_file()
        );
    }
    let path =
        std::env::var_os("VEOVEO_COMPUTERS_TRANSITION_CONFIG").expect("installation config path");
    let catalog = Configuration::load(Path::new(&path))
        .await
        .unwrap()
        .template_catalog()
        .unwrap();
    let source_id =
        std::env::var("VEOVEO_COMPUTERS_TRANSITION_SOURCE").expect("source template ID");
    let target_id =
        std::env::var("VEOVEO_COMPUTERS_TRANSITION_TARGET").expect("target template ID");
    let source = catalog
        .select(Some(&source_id))
        .expect("admitted source")
        .runtime()
        .clone();
    let target = catalog
        .select(Some(&target_id))
        .expect("admitted target")
        .runtime()
        .clone();
    assert_ne!(
        source.image(),
        target.image(),
        "qualification requires an actual image transition"
    );
    let templates = vec![source.clone(), target.clone()];
    let _telemetry = veoveo_mcp_contract::init_server_telemetry(
        "veoveo-computers-native-maintenance",
        "veoveo_computers_mcp=debug",
    )
    .unwrap();
    let home =
        native_service_support::Fixture::start_with_templates(templates.clone(), SCENARIO).await;
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
    let db = support::TestDb::new().await;
    let mut control = support::automation::control();
    for name in ["update_template", "resume_update"] {
        let tool = LocalToolName::new(name).unwrap();
        control.servers[0].tools.push(tool.clone());
        control.policies[0].rules[0].tools.insert(tool);
    }
    support::policy::install(&db.a, control).await;
    let owner =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
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
    let computer = a
        .reserve(
            owner.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: source_id.clone(),
                template_fingerprint: source.fingerprint(),
            },
        )
        .await
        .unwrap();
    let homes = RetainedHomes::new(Uuid::from_u128(100), home.allocation_config(), &templates)
        .await
        .unwrap();
    let tasks_a = TaskRuntime::new(db.a.clone(), "computers", "maintenance-native-a");
    let tasks_b = TaskRuntime::new(db.b.clone(), "computers", "maintenance-native-b");
    let lifecycle = LifecycleWorker::new(
        a.clone(),
        tasks_a.clone(),
        provider.runtime.clone(),
        templates.clone(),
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
    let initial = Binding::new(computer.computer_id, source.fingerprint()).unwrap();
    shell(
        &provider.runtime,
        &initial,
        "printf original > transition-marker; test ! -e rollback-marker",
    )
    .await;
    maintenance_policy::add_grant(&provider, &initial).await;
    let keys = Arc::new(
        ComputerKeyRing::new(
            Uuid::from_u128(1),
            vec![
                ComputerSealingKey::new(Uuid::from_u128(1), zeroize::Zeroizing::new([23; 32]))
                    .unwrap(),
            ],
        )
        .unwrap(),
    );
    let profiles = MaintenanceProfiles::new(
        templates,
        vec![
            MaintenanceTransition {
                source_fingerprint: source.fingerprint(),
                target_fingerprint: target.fingerprint(),
            },
            MaintenanceTransition {
                source_fingerprint: target.fingerprint(),
                target_fingerprint: source.fingerprint(),
            },
        ],
    )
    .unwrap();
    let workers = [
        Arc::new(
            MaintenanceWorker::new(
                a.clone(),
                tasks_a.clone(),
                provider.runtime.clone(),
                profiles.clone(),
                homes.clone(),
                keys.clone(),
            )
            .unwrap(),
        ),
        Arc::new(
            MaintenanceWorker::new(
                b.clone(),
                tasks_b.clone(),
                provider.runtime.clone(),
                profiles,
                homes.clone(),
                keys,
            )
            .unwrap(),
        ),
    ];
    let operation = a
        .queue_maintenance(
            &owner,
            computer.computer_id,
            Uuid::now_v7(),
            &MaintenanceTarget {
                template_id: target_id.clone(),
                template_fingerprint: target.fingerprint(),
            },
        )
        .await
        .unwrap();
    a.ensure_maintenance_task(owner.owner(), operation.operation_id)
        .await
        .unwrap();
    let claim = tasks_a
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    assert_eq!(ticket.step(), MaintenanceStep::Stop);
    let dispatched = ticket.operation().clone();
    let before = provider.runtime.get(&initial).await.unwrap().unwrap();
    // Deliver the original Stop once, then discard its response at the journal
    // boundary. The test injects the exhausted budget; it does not claim eight
    // native reads occurred. Domain tests qualify the exact budget accounting.
    let _lost_response = provider.runtime.stop(&initial, &before).await;
    drop(ticket);
    let paused = a
        .pause_maintenance(&claim, MaintenanceRecovery::BudgetExhausted)
        .await
        .unwrap();
    tasks_a.release_observation(&claim).await.unwrap();
    let request = ResumeUpdateInput {
        computer_id: computer.computer_id,
        task_id: operation.operation_id,
        request_id: Uuid::now_v7(),
        expected_updated_at: paused.updated_at,
        acknowledged_cancellation_at: None,
    };
    let resumed = b.resume_maintenance(&owner, &request).await.unwrap();
    assert_eq!(
        resumed.steps()[0].dispatch_id,
        dispatched.steps()[0].dispatch_id
    );
    assert_eq!(resumed.target_instance_id, operation.target_instance_id);
    let started = Instant::now();
    let running = schedulers::Schedulers::maintenance(workers.clone());
    let completed = settle(&a, &tasks_a, &owner, &resumed).await;
    running.stop().await;
    assert_eq!(completed.steps().len(), 6);
    assert_eq!(
        completed.steps()[0].dispatch_id,
        dispatched.steps()[0].dispatch_id
    );
    assert!(completed.steps()[0].observation_reads > 0);
    assert!(
        matches!(completed.steps().last().unwrap().evidence, Some(MaintenanceEvidence::Restored { policy_version, .. }) if policy_version > 1)
    );
    let upgraded = b.get(owner.owner(), computer.computer_id).await.unwrap();
    assert_eq!(upgraded.phase, ComputerPhase::Ready);
    assert_eq!(upgraded.instance_id(), operation.target_instance_id);
    assert_eq!(upgraded.template_fingerprint, target.fingerprint());
    assert!(upgraded.active_operation.is_none());
    let current = Binding::from_instance(
        upgraded.computer_id,
        upgraded.instance_id(),
        upgraded.template_fingerprint,
    )
    .unwrap();
    shell(&provider.runtime, &current, "test \"$(cat transition-marker)\" = original; printf upgraded > rollback-marker; test -x /usr/local/bin/veoveo-computer-exec").await;
    assert!(provider.runtime.get(&initial).await.unwrap().is_none());
    assert!(
        home.worker(home.provider)
            .await
            .restore(&initial)
            .await
            .is_err()
    );
    eprintln!(
        "Native exact template upgrade and explicit recovery: {} -> {}, home {} MiB, {} ms",
        source.fingerprint(),
        target.fingerprint(),
        source.persistent_home().unwrap().capacity_mib(),
        started.elapsed().as_millis()
    );
    // Rollback is another fresh instance, using the same new worker/schema. It
    // never reinstates an obsolete reader or reuses an earlier physical writer.
    let owner =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap(); // Each new request has a fresh short-lived gateway assertion.
    let rollback = a
        .queue_maintenance(
            &owner,
            computer.computer_id,
            Uuid::now_v7(),
            &MaintenanceTarget {
                template_id: source_id.clone(),
                template_fingerprint: source.fingerprint(),
            },
        )
        .await
        .unwrap();
    assert_ne!(rollback.target_instance_id, initial.computer_id());
    assert_ne!(
        rollback.target_instance_id,
        current.replacement_instance_id().unwrap()
    );
    let started = Instant::now();
    let running = schedulers::Schedulers::maintenance(workers.clone());
    let completed = settle(&b, &tasks_b, &owner, &rollback).await;
    running.stop().await;
    assert_eq!(completed.steps().len(), 6);
    assert!(
        matches!(completed.steps().last().unwrap().evidence, Some(MaintenanceEvidence::Restored { policy_version, .. }) if policy_version > 1)
    );
    let restored = a.get(owner.owner(), computer.computer_id).await.unwrap();
    assert_eq!(restored.phase, ComputerPhase::Ready);
    assert_eq!(restored.instance_id(), rollback.target_instance_id);
    assert_eq!(restored.template_fingerprint, source.fingerprint());
    assert!(restored.active_operation.is_none());
    let final_binding = Binding::from_instance(
        restored.computer_id,
        restored.instance_id(),
        restored.template_fingerprint,
    )
    .unwrap();
    shell(
        &provider.runtime,
        &final_binding,
        "test \"$(cat transition-marker)\" = original; test \"$(cat rollback-marker)\" = upgraded",
    )
    .await;
    assert!(provider.runtime.get(&current).await.unwrap().is_none());
    assert!(
        home.worker_template(home.provider, &target.fingerprint())
            .await
            .restore(&current)
            .await
            .is_err()
    );
    let usage: Vec<i64> =
        db.a.client()
            .query("SELECT VALUE retained FROM computer_usage;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(usage, vec![1, 1, 1]);
    eprintln!(
        "Native exact template rollback: {} -> {}, {} ms",
        target.fingerprint(),
        source.fingerprint(),
        started.elapsed().as_millis()
    );
    let owner =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    maintenance_initial_failure::Scenario {
        db: &db,
        store: &a,
        tasks: &tasks_a,
        owner: &owner,
        source: (&source_id, &source),
        target: (&target_id, &target),
        allocator: home.worker(home.provider).await,
        runtime: &provider.runtime,
        workers,
    }
    .run()
    .await;
    provider.assert_running();
    drop(lifecycle);
    drop(provider);
    home.finish(None).await;
}
