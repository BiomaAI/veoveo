use super::*;
use serde_json::json;
use std::collections::BTreeMap;

fn selected() -> SelectedDrainGroup {
    let members: Vec<_> = [
        DrainProfile::server("control", Duration::from_secs(20)).unwrap(),
        DrainProfile::nvidia("worker", Duration::from_secs(20)).unwrap(),
    ]
    .into_iter()
    .map(|profile| SelectedDrainTarget {
        namespace: "fixture".into(),
        namespace_uid: Uuid::from_u128(1),
        deployment: "workload".into(),
        deployment_uid: Uuid::from_u128(2),
        deployment_version: "11".into(),
        generation: 1,
        pod: "old".into(),
        pod_uid: Uuid::from_u128(3),
        pod_version: "12".into(),
        container_id: format!("containerd://{}-old", profile.container),
        restart_count: 0,
        profile,
        grace: 30,
        annotations: BTreeMap::new(),
    })
    .collect();
    SelectedDrainGroup {
        progress: Arc::new(Mutex::new(DrainGroupProgress {
            selected: members.iter().map(SelectedDrainTarget::identity).collect(),
            replica_set_uid: Uuid::from_u128(4),
            replica_set_resource_version: "10".into(),
            state: DrainGroupState::Admitted,
            exits: vec![],
            watch_closed: false,
        })),
        members,
        images: vec!["sha256:control".into(), "sha256:worker".into()],
        replica_set: "old-rs".into(),
        replica_set_uid: Uuid::from_u128(4),
        replica_set_version: "10".into(),
        attempted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}
fn pod(version: &str, exits: [Option<i32>; 2]) -> drain::Pod {
    let statuses = ["control", "worker"].into_iter().zip(exits).map(|(name, exit)| json!({
            "name":name,"containerID":format!("containerd://{name}-old"),"imageID":format!("sha256:{name}"),"restartCount":0,"ready":exit.is_none(),
            "state": if let Some(code) = exit {json!({"terminated":{"exitCode":code,"finishedAt":"2026-10-07T00:00:10Z"}})} else {json!({"running":{"startedAt":"2026-10-06T00:00:00Z"}})}
        })).collect::<Vec<_>>();
    serde_json::from_value(json!({
        "metadata":{"name":"old","namespace":"fixture","uid":Uuid::from_u128(3),"resourceVersion":version,
            "ownerReferences":[{"name":"old-rs","kind":"ReplicaSet","uid":Uuid::from_u128(4),"controller":true}],
            "deletionTimestamp":if exits.iter().any(Option::is_some) {Some("2026-10-07T00:00:30Z")} else {None},
            "deletionGracePeriodSeconds":if exits.iter().any(Option::is_some) {Some(30)} else {None}},
        "spec":{"terminationGracePeriodSeconds":30,"containers":[{"name":"control","resources":{}},{"name":"worker","resources":{"requests":{"nvidia.com/gpu":"1"},"limits":{"nvidia.com/gpu":"1"}}}]},
        "status":{"conditions":[{"type":"Ready","status":"True"}],"containerStatuses": statuses}
    })).unwrap()
}
fn observation() -> GroupObservation {
    let mut observer = GroupObservation::new(selected());
    observer
        .initial(drain::WatchEvent::Added(pod("12", [None, None])))
        .unwrap();
    observer.dispatched("2026-10-07T00:00:00Z".parse().unwrap());
    observer
}
#[test]
fn coordinated_partial_exit_requires_every_exact_instance() {
    let mut observer = observation();
    assert!(
        !observer
            .observe(drain::WatchEvent::Modified(pod("13", [Some(0), None])))
            .unwrap()
    );
    assert_eq!(observer.selected.progress().exits.len(), 1);
    assert!(
        observer
            .observe(drain::WatchEvent::Modified(pod("14", [Some(0), Some(0)])))
            .unwrap()
    );
    let receipts = observer.receipts(2).unwrap();
    assert_eq!(receipts.len(), 2);
    for receipt in receipts {
        assert!(
            !serde_json::to_string(&receipt)
                .unwrap()
                .contains("containerd://")
        );
    }
}
#[test]
fn coordinated_refuses_failed_missing_stale_and_wrong_instance_observations() {
    for (exits, successful) in [
        ([Some(0), Some(1)], "control"),
        ([Some(1), Some(0)], "worker"),
    ] {
        let mut observer = observation();
        assert!(
            observer
                .observe(drain::WatchEvent::Modified(pod("13", exits)))
                .is_err()
        );
        let progress = observer.selected.progress();
        assert_eq!(progress.exits.len(), 1, "qualified partial fact was lost");
        assert_eq!(progress.exits[0].container, successful);
        assert!(matches!(progress.state, DrainGroupState::Unqualified));
    }
    assert!(
        observation()
            .observe(drain::WatchEvent::Deleted(pod("13", [Some(0), None])))
            .is_err()
    );
    let mut changed = pod("13", [Some(0), Some(0)]);
    changed.status.container_statuses[1].container_id = "containerd://other".into();
    assert!(
        observation()
            .observe(drain::WatchEvent::Modified(changed))
            .is_err()
    );
    let mut changed = pod("13", [Some(0), Some(0)]);
    changed.status.container_statuses[1].restart_count = 1;
    assert!(
        observation()
            .observe(drain::WatchEvent::Modified(changed))
            .is_err()
    );
    let mut changed = pod("13", [Some(0), Some(0)]);
    changed.metadata.owner_references[0].uid = Uuid::from_u128(99);
    assert!(
        observation()
            .observe(drain::WatchEvent::Modified(changed))
            .is_err()
    );
    let mut observer = observation();
    assert!(
        !observer
            .observe(drain::WatchEvent::Modified(pod("13", [None, None])))
            .unwrap()
    );
    assert!(
        observer
            .observe(drain::WatchEvent::Modified(pod("13", [Some(0), Some(0)])))
            .is_err()
    );
    assert!(
        observation()
            .observe(drain::WatchEvent::Added(pod("13", [Some(0), Some(0)])))
            .is_err()
    );
    let mut late = pod("13", [Some(0), Some(0)]);
    late.status.container_statuses[1]
        .state
        .terminated
        .as_mut()
        .unwrap()
        .finished_at = "2026-10-07T00:00:25Z".parse().unwrap();
    assert!(
        observation()
            .observe(drain::WatchEvent::Modified(late))
            .is_err()
    );
}
#[test]
fn coordinated_profiles_and_replacement_preserve_gpu_and_same_image_admission() {
    let selected = selected();
    let requested: Vec<_> = selected.members.iter().map(|m| m.profile.clone()).collect();
    profiles(&requested).unwrap();
    assert!(profiles(&requested[..1]).is_err());
    assert!(profiles(&[requested[0].clone(), requested[0].clone()]).is_err());
    assert!(profiles(&vec![requested[0].clone(); 9]).is_err());
    let mut no_gpu = pod("12", [None, None]);
    no_gpu.spec.containers[1].resources.requests.clear();
    assert!(
        GroupObservation::new(selected.clone())
            .initial(drain::WatchEvent::Added(no_gpu))
            .is_err()
    );
    let mut new = pod("20", [None, None]);
    new.metadata.name = "new".into();
    new.metadata.uid = Uuid::from_u128(5);
    new.metadata.owner_references[0].uid = Uuid::from_u128(6);
    new.metadata.owner_references[0].name = "new-rs".into();
    for status in &mut new.status.container_statuses {
        status.container_id.push_str("-new");
    }
    assert_eq!(replacement(&selected, &new).unwrap().len(), 2);
    new.status.container_statuses[1].image_id = "sha256:unadmitted".into();
    assert!(replacement(&selected, &new).is_err());
    new.status.container_statuses[1].image_id = selected.images[1].clone();
    new.status.container_statuses[1].ready = false;
    assert!(replacement(&selected, &new).is_err());
}

#[test]
fn coordinated_owned_watch_cleanup_survives_timeout_gap_and_owner_drop() {
    const KEY: &str = "VEOVEO_TEST_COORDINATED_WATCH_CLEANUP";
    let Ok(mode) = std::env::var(KEY) else {
        for mode in ["timeout", "gap", "owner"] {
            crate::process::tests::isolated_control(
                "installed::restart::group::tests::coordinated_owned_watch_cleanup_survives_timeout_gap_and_owner_drop",
                KEY,
                mode,
            );
        }
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
    let marker = root.join(".watch-pid");
    let selected = selected();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result: Result<()> = runtime.block_on(owner::run(async {
        let (watch, _registration) = OwnedWatch::register(selected.clone())?;
        // Hold the retained slot before launching, then publish the real handle
        // synchronously. A dropped operation leaves it with the owner action.
        let mut slot = watch.slot.lock().await;
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", if mode == "gap" {
            "printf '%s' $$ > \"$MARKER\"; printf '%s' '{\"type\":\"ERROR\",\"object\":{\"code\":410}}'; sleep 30"
        } else { "printf '%s' $$ > \"$MARKER\"; sleep 30" }])
            .env("MARKER", &marker).stdout(std::process::Stdio::piped());
        slot.watch = Some(drain::PodWatch::from_child(crate::spawn_async(command)?)?);
        drop(slot);
        let end = Instant::now() + Duration::from_secs(1);
        while !marker.exists() {
            ensure!(Instant::now() < end, "native watch readiness timed out");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        if mode == "owner" {
            owner::test_cancel(&owner::active().unwrap());
            std::future::pending::<()>().await;
        } else if mode == "gap" {
            observation().observe(watch.next().await?)?;
        } else {
            tokio::time::timeout(Duration::from_millis(30), watch.next()).await
                .context("native selected watch deadline expired")??;
        }
        anyhow::bail!("native fixture unexpectedly reached successful drain")
    }));
    assert!(result.is_err());
    let progress = selected.progress();
    assert!(
        progress.watch_closed,
        "actual retained watch was not awaited"
    );
    assert!(matches!(progress.state, DrainGroupState::Unqualified));
    let pid = std::fs::read_to_string(&marker).unwrap();
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "watch leader remained unreaped"
    );
    assert!(
        !root.join(pid).exists(),
        "watch ownership registration remained unsettled"
    );
}

#[test]
fn coordinated_current_receipt_fence_allows_new_version_but_rejects_process_drift() {
    let selected = selected();
    let mut current = pod("20", [None, None]);
    current.metadata.name = "new".into();
    current.metadata.uid = Uuid::from_u128(5);
    current.metadata.owner_references[0].uid = Uuid::from_u128(6);
    current.metadata.owner_references[0].name = "new-rs".into();
    for status in &mut current.status.container_statuses {
        status.container_id.push_str("-new");
    }
    let receipt = DrainGroupReceipt {
        exits: vec![],
        replacement_pod: current.metadata.name.clone(),
        replacement_pod_uid: current.metadata.uid,
        replacement_pod_resource_version: "20".into(),
        replacement_replica_set_uid: Uuid::from_u128(6),
        replacement_replica_set_resource_version: "19".into(),
        replacement_generation: 2,
        containers: replacement(&selected, &current).unwrap(),
    };
    let mut deployment: super::super::Deployment = serde_json::from_value(json!({
        "metadata":{"uid":Uuid::from_u128(2).to_string(),"generation":2,"resourceVersion":"21"},
        "spec":{"replicas":1,"selector":{"matchLabels":{}},"template":{"metadata":{}}},
        "status":{"observedGeneration":2,"updatedReplicas":1,"availableReplicas":1,"unavailableReplicas":0}
    })).unwrap();
    require_generation(&deployment, &selected.members[0], 2).unwrap();
    deployment.metadata.generation = 3;
    deployment.status.observed_generation = Some(3);
    assert!(
        require_generation(&deployment, &selected.members[0], 2).is_err(),
        "intervening rollout admitted"
    );
    current.metadata.resource_version = "opaque-newer".into();
    verify_replacement_receipt(&selected, &receipt, &current).unwrap();
    current.status.container_statuses[0].restart_count += 1;
    assert!(verify_replacement_receipt(&selected, &receipt, &current).is_err());
    current.status.container_statuses[0].restart_count -= 1;
    current.metadata.uid = Uuid::from_u128(7);
    assert!(verify_replacement_receipt(&selected, &receipt, &current).is_err());
    current.metadata.uid = receipt.replacement_pod_uid;
    current.metadata.owner_references[0].uid = Uuid::from_u128(8);
    assert!(verify_replacement_receipt(&selected, &receipt, &current).is_err());
}
