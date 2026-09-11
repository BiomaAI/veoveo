use super::*;

#[tokio::test]
async fn retirement_preserves_exact_instance_and_never_replays_a_lost_reply() {
    for reply in [0, 1, 2] {
        let running = Running::start().await;
        let fake = running.fake.clone();
        let selected = Binding::replacement(
            binding().computer_id(),
            Uuid::now_v7(),
            binding().template_fingerprint().into(),
        )
        .unwrap();
        {
            let mut state = fake.0.lock().unwrap();
            let mut source = sandbox(Phase::Stopped);
            source.metadata.as_mut().unwrap().name = selected.name();
            source.metadata.as_mut().unwrap().labels = selected.labels();
            state.sandbox = Some(source);
            state.expected_binding = Some(selected.clone());
            state.deletion_reply = reply;
        }

        let before = running.runtime.get(&selected).await.unwrap().unwrap();
        let result = running.runtime.retire(&selected, &before).await;
        match reply {
            0 => assert_eq!(result.unwrap(), RetirementAcknowledgement::DeletionAccepted),
            1 => assert_eq!(result.unwrap_err(), RuntimeFailure::LifecycleUnknown),
            2 => assert_eq!(
                result.unwrap(),
                RetirementAcknowledgement::ResourceAlreadyAbsent
            ),
            _ => unreachable!(),
        }
        assert!(running.runtime.get(&selected).await.unwrap().is_none());
        assert_eq!(
            running
                .runtime
                .retire(&selected, &before)
                .await
                .unwrap_err(),
            RuntimeFailure::NotFound
        );
        let state = fake.0.lock().unwrap();
        assert_eq!(state.deletes, 1);
        assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
    }
}

#[tokio::test]
async fn retirement_rejects_stale_resource_run_or_phase_before_delete() {
    for changed in ["resource", "process", "phase", "binding", "missing"] {
        let running = Running::start().await;
        let fake = running.fake.clone();

        let before = running.runtime.get(&binding()).await.unwrap().unwrap();
        {
            let mut state = fake.0.lock().unwrap();
            match changed {
                "resource" => {
                    state
                        .sandbox
                        .as_mut()
                        .unwrap()
                        .metadata
                        .as_mut()
                        .unwrap()
                        .id = "foreign-resource".into()
                }
                "process" => {
                    state
                        .sandbox
                        .as_mut()
                        .unwrap()
                        .status
                        .as_mut()
                        .unwrap()
                        .main_process_instance_id = "other-process".into()
                }
                "phase" => {
                    state
                        .sandbox
                        .as_mut()
                        .unwrap()
                        .status
                        .as_mut()
                        .unwrap()
                        .phase = Phase::Ready as i32
                }
                "binding" => {
                    state
                        .sandbox
                        .as_mut()
                        .unwrap()
                        .metadata
                        .as_mut()
                        .unwrap()
                        .labels
                        .insert("veoveo-instance".into(), Uuid::now_v7().to_string());
                }
                "missing" => state.sandbox = None,
                _ => unreachable!(),
            }
        }
        assert!(
            running.runtime.retire(&binding(), &before).await.is_err(),
            "{changed}"
        );
        assert_eq!(fake.0.lock().unwrap().deletes, 0);
    }
    let running = Running::start().await;
    let fake = running.fake.clone();

    let mut before = running.runtime.get(&binding()).await.unwrap().unwrap();
    before.phase = Phase::Ready;
    assert!(running.runtime.retire(&binding(), &before).await.is_err());
    assert_eq!(fake.0.lock().unwrap().deletes, 0);
}
