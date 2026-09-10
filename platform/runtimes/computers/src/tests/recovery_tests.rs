use super::*;
use crate::{LifecycleCheckpoint, LifecycleObservation};

fn before(phase: Phase) -> Observation {
    Observation {
        sandbox_id: "sandbox-1".into(),
        phase,
        main_process_instance_id: "main-1".into(),
        exit_code: None,
    }
}

fn start_checkpoint() -> LifecycleCheckpoint {
    LifecycleCheckpoint::start(
        Uuid::from_u128(100),
        Uuid::from_u128(101),
        binding(),
        &before(Phase::Stopped),
    )
    .unwrap()
}

#[tokio::test]
async fn a_stale_resource_or_process_cannot_receive_start_or_stop() {
    let running = Running::start().await;
    for phase in [Phase::Ready, Phase::Stopped] {
        running.fake.0.lock().unwrap().sandbox = Some(sandbox(phase));
        for resource_changed in [false, true] {
            let mut stale = before(phase);
            if resource_changed {
                stale.sandbox_id = "old-resource".into();
            } else {
                stale.main_process_instance_id = "old-process".into();
            }
            let result = if phase == Phase::Ready {
                running.runtime.stop(&binding(), &stale).await
            } else {
                running.runtime.start(&binding(), &stale).await
            };
            assert!(matches!(result, Err(RuntimeFailure::BindingMismatch)));
        }
    }
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.starts, state.stops), (0, 0));
}

#[tokio::test]
async fn healthy_watch_rejects_previous_ready_epoch_then_accepts_a_new_run() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let old = before(Phase::Ready);
    assert!(matches!(
        running
            .runtime
            .wait_for_lifecycle(&start_checkpoint(), &old, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::WatchFailed)
    ));
    {
        let mut state = running.fake.0.lock().unwrap();
        state
            .sandbox
            .as_mut()
            .unwrap()
            .status
            .as_mut()
            .unwrap()
            .main_process_instance_id = "main-2".into();
    }
    let seen = running
        .runtime
        .wait_for_lifecycle(&start_checkpoint(), &old, Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(seen.main_process_instance_id, "main-2");
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.gets, state.watches), (0, 2));
    assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
}

#[tokio::test]
async fn healthy_watch_has_a_caller_budget_without_recovery_or_redispatch() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().watch = 9;
    let started = tokio::time::Instant::now();
    assert!(matches!(
        running
            .runtime
            .wait_for_lifecycle(
                &start_checkpoint(),
                &before(Phase::Starting),
                Duration::from_millis(20)
            )
            .await,
        Err(RuntimeFailure::WatchFailed)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.gets, state.watches), (0, 1));
    assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
}

#[tokio::test]
async fn synchronous_completion_cannot_bypass_source_epoch_or_deadline() {
    let running = Running::start().await;
    let stop = LifecycleCheckpoint::stop(
        Uuid::from_u128(100),
        Uuid::now_v7(),
        binding(),
        &before(Phase::Ready),
    )
    .unwrap();
    let mut wrong = before(Phase::Stopped);
    wrong.main_process_instance_id = "another-run".into();
    assert!(matches!(
        running
            .runtime
            .wait_for_lifecycle(&stop, &wrong, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::BindingMismatch)
    ));
    let other_provider =
        LifecycleCheckpoint::create(Uuid::from_u128(200), Uuid::now_v7(), binding()).unwrap();
    assert!(matches!(
        running
            .runtime
            .wait_for_lifecycle(
                &other_provider,
                &before(Phase::Ready),
                Duration::from_secs(1)
            )
            .await,
        Err(RuntimeFailure::BindingMismatch)
    ));
    assert!(matches!(
        running
            .runtime
            .wait_for_lifecycle(&stop, &before(Phase::Stopped), Duration::ZERO)
            .await,
        Err(RuntimeFailure::WatchFailed)
    ));
    let create =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), binding()).unwrap();
    for process in ["main-1", ""] {
        let mut invalid = before(Phase::Ready);
        invalid.main_process_instance_id = process.into();
        invalid.exit_code = if process.is_empty() { None } else { Some(0) };
        assert!(matches!(
            running
                .runtime
                .wait_for_lifecycle(&create, &invalid, Duration::from_secs(1))
                .await,
            Err(RuntimeFailure::LifecycleUnknown)
        ));
    }
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.gets, state.watches), (0, 0));
}

#[tokio::test]
async fn remaining_budget_bounds_a_stalled_status_read() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().get_delay = Duration::from_secs(5);
    let started = tokio::time::Instant::now();
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&start_checkpoint(), Duration::from_millis(20))
            .await,
        Err(RuntimeFailure::LifecycleUnknown)
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
}

#[tokio::test]
async fn ready_with_an_exit_code_or_missing_process_cannot_complete_create() {
    let running = Running::start().await;
    let checkpoint =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::from_u128(103), binding()).unwrap();
    for (process, exit_code) in [("main-1", Some(0)), ("", None)] {
        {
            let mut state = running.fake.0.lock().unwrap();
            let status = state.sandbox.as_mut().unwrap().status.as_mut().unwrap();
            status.phase = Phase::Ready as i32;
            status.main_process_instance_id = process.into();
            status.exit_code = exit_code;
        }
        assert!(matches!(
            running
                .runtime
                .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
                .await,
            Err(RuntimeFailure::LifecycleUnknown)
        ));
    }
}

#[test]
fn persisted_recovery_revalidates_identity_and_preserves_operation() {
    let checkpoint = start_checkpoint();
    let value = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(
        serde_json::from_value::<LifecycleCheckpoint>(value.clone()).unwrap(),
        checkpoint
    );
    assert_eq!(checkpoint.operation_id(), Uuid::from_u128(101));
    for key in ["operationId", "computerId", "providerInstanceId"] {
        let mut invalid = value.clone();
        invalid[key] = serde_json::json!(Uuid::nil());
        assert!(serde_json::from_value::<LifecycleCheckpoint>(invalid).is_err());
    }
    for (key, bad) in [
        ("version", serde_json::json!(2)),
        ("templateFingerprint", serde_json::json!("invalid")),
        ("unownedAuthority", serde_json::json!(true)),
    ] {
        let mut invalid = value.clone();
        invalid[key] = bad;
        assert!(serde_json::from_value::<LifecycleCheckpoint>(invalid).is_err());
    }
    let mut invalid = value;
    invalid["goal"]["previous_process_id"] = serde_json::json!("");
    assert!(serde_json::from_value::<LifecycleCheckpoint>(invalid).is_err());
    assert!(Binding::new(Uuid::nil(), "a".repeat(64)).is_err());
}

#[tokio::test]
async fn lost_watch_reconciles_one_new_run_without_any_mutation() {
    let running = Running::start().await;
    let checkpoint = start_checkpoint();
    let current = running
        .runtime
        .start(&binding(), &before(Phase::Stopped))
        .await
        .unwrap();
    running.fake.0.lock().unwrap().watch = 1;
    assert!(matches!(
        running
            .runtime
            .wait_for_lifecycle(&checkpoint, &current, Duration::from_secs(10))
            .await,
        Err(RuntimeFailure::WatchFailed)
    ));
    {
        let mut state = running.fake.0.lock().unwrap();
        let status = state.sandbox.as_mut().unwrap().status.as_mut().unwrap();
        status.phase = Phase::Ready as i32;
        status.main_process_instance_id = "main-2".into();
    }
    let calls = running.fake.0.lock().unwrap().gets;
    let result = running
        .runtime
        .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
        .await
        .unwrap();
    assert!(
        matches!(result, LifecycleObservation::Reached(seen) if seen.main_process_instance_id == "main-2")
    );
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.gets, calls + 1);
    assert_eq!((state.creates, state.starts, state.stops), (0, 1, 0));
}

#[tokio::test]
async fn previous_ready_run_and_old_stopped_state_do_not_settle_start() {
    let running = Running::start().await;
    for phase in [Phase::Stopped, Phase::Starting, Phase::Ready] {
        running
            .fake
            .0
            .lock()
            .unwrap()
            .sandbox
            .as_mut()
            .unwrap()
            .status
            .as_mut()
            .unwrap()
            .phase = phase as i32;
        assert!(matches!(
            running
                .runtime
                .reconcile_lifecycle(&start_checkpoint(), Duration::from_secs(1))
                .await
                .unwrap(),
            LifecycleObservation::Pending(_)
        ));
    }
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
}

#[tokio::test]
async fn stop_recovery_rejects_another_stopped_process_or_provider_resource() {
    let running = Running::start().await;
    let checkpoint = LifecycleCheckpoint::stop(
        Uuid::from_u128(100),
        Uuid::from_u128(102),
        binding(),
        &before(Phase::Ready),
    )
    .unwrap();
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
            .await
            .unwrap(),
        LifecycleObservation::Reached(_)
    ));
    running
        .fake
        .0
        .lock()
        .unwrap()
        .sandbox
        .as_mut()
        .unwrap()
        .status
        .as_mut()
        .unwrap()
        .main_process_instance_id = "other-main".into();
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::BindingMismatch)
    ));
    {
        let mut state = running.fake.0.lock().unwrap();
        let sandbox = state.sandbox.as_mut().unwrap();
        sandbox.status.as_mut().unwrap().main_process_instance_id = "main-1".into();
        sandbox.metadata.as_mut().unwrap().id = "other-sandbox".into();
    }
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::BindingMismatch)
    ));
}

#[tokio::test]
async fn missing_create_and_failed_observer_preserve_uncertainty() {
    let running = Running::start().await;
    let checkpoint =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::from_u128(103), binding()).unwrap();
    running.fake.0.lock().unwrap().sandbox = None;
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::LifecycleUnknown)
    ));
    running.fake.0.lock().unwrap().denied = true;
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::Unavailable)
    ));
    assert_eq!(running.fake.0.lock().unwrap().creates, 0);
}

#[tokio::test]
async fn provider_mismatch_and_exhausted_budget_never_issue_a_request() {
    let running = Running::start().await;
    let checkpoint =
        LifecycleCheckpoint::create(Uuid::from_u128(200), Uuid::from_u128(103), binding()).unwrap();
    let before = running.fake.0.lock().unwrap().gets;
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&checkpoint, Duration::from_secs(1))
            .await,
        Err(RuntimeFailure::BindingMismatch)
    ));
    assert!(matches!(
        running
            .runtime
            .reconcile_lifecycle(&start_checkpoint(), Duration::ZERO)
            .await,
        Err(RuntimeFailure::LifecycleUnknown)
    ));
    assert_eq!(running.fake.0.lock().unwrap().gets, before);
}
