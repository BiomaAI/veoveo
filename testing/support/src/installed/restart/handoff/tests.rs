//! Adversarial Kubernetes observations, without a cluster or orchestration mutation.
use super::*;
use model::{HandoffFixture, Inventory, Object};
use serde_json::{Value, json};
use uuid::Uuid;

fn uid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}
fn object(name: &str, id: u128, component: &str, spec: Value) -> Result<Object> {
    Ok(serde_json::from_value(json!({"metadata": {
        "name":name,"uid":uid(id),"resourceVersion":"1",
        "creationTimestamp":"2026-10-10T00:00:00Z",
        "labels":{"app.kubernetes.io/component":component,"app.kubernetes.io/instance":"fixture"}
    },"spec":spec}))?)
}
fn inventory(objects: Vec<Object>) -> Inventory {
    Inventory {
        resource_version: "1".into(),
        objects: objects.into_iter().map(|o| (o.metadata.uid, o)).collect(),
    }
}
fn pod(name: &str, id: u128, rs: u128, component: &str, spec: &Value) -> Result<Object> {
    let mut pod = object(name, id, component, spec.clone())?;
    pod.spec["nodeName"] = json!("node");
    pod.metadata.owner_references =
        serde_json::from_value(json!([{"kind":"ReplicaSet","uid":uid(rs),"controller":true}]))?;
    pod.status = json!({"podIP":format!("10.0.0.{id}"),"containerStatuses":[{
        "name":"server","containerID":format!("containerd://{id}"),"imageID":"sha256:running",
        "restartCount":0,"ready":true,"state":{"running":{}}
    }]});
    Ok(pod)
}
fn replica_set(name: &str, id: u128, deployment: u128, spec: &Value) -> Result<Object> {
    let mut rs = object(name, id, "unused", json!({"template":{"spec":spec}}))?;
    rs.metadata.owner_references = serde_json::from_value(
        json!([{"kind":"Deployment","uid":uid(deployment),"controller":true}]),
    )?;
    Ok(rs)
}
fn endpoint(pod: u128) -> Result<Object> {
    let mut slice = object("slice", 9, "unused", json!({}))?;
    slice
        .metadata
        .labels
        .insert("kubernetes.io/service-name".into(), "service".into());
    slice.metadata.owner_references =
        serde_json::from_value(json!([{"kind":"Service","uid":uid(2),"controller":true}]))?;
    slice.endpoints = serde_json::from_value(json!([{"addresses":[format!("10.0.0.{pod}")],
        "targetRef":{"kind":"Pod","uid":uid(pod)},"conditions":{"ready":true,"terminating":false}}]))?;
    slice.ports = serde_json::from_value(json!([{"name":"http","protocol":"TCP","port":8800}]))?;
    Ok(slice)
}
fn fixture() -> Result<(HandoffFixture, BTreeMap<watch::Kind, Inventory>)> {
    use watch::Kind::*;
    let image = format!("registry.test/server@sha256:{}", "a".repeat(64));
    let spec = json!({"containers":[{"name":"server","image":image,
        "volumeMounts":[{"name":"workspace","mountPath":"/work"}]}],
        "volumes":[{"name":"workspace","persistentVolumeClaim":{"claimName":"authority"}}]});
    let selector: BTreeMap<String, String> = [
        ("app.kubernetes.io/component".into(), "a".into()),
        ("app.kubernetes.io/instance".into(), "fixture".into()),
    ]
    .into();
    let service_spec = json!({"ports":[{"port":8800,"name":"http","protocol":"TCP"}]});
    let f = HandoffFixture {
        namespace_uid: uid(1),
        service: "service".into(),
        service_uid: uid(2),
        service_resource_version: "1".into(),
        service_spec_sha256: model::digest(&service_spec)?,
        service_selector: selector.clone(),
        service_port: 8800,
        deployment_a: "a".into(),
        deployment_a_uid: uid(3),
        deployment_b: "b".into(),
        component_a: "a".into(),
        component_b: "b".into(),
        container: "server".into(),
        pod_a: "a-pod".into(),
        pod_a_uid: uid(4),
        container_a_id: "containerd://4".into(),
        image,
        image_id: "sha256:running".into(),
        node: "node".into(),
        workload_spec_sha256: model::digest(&spec)?,
        pvc: Some(model::Pvc {
            name: "authority".into(),
            uid: uid(20),
        }),
    };
    let a = object(
        "a",
        3,
        "a",
        json!({"replicas":1,"template":{"metadata":{"labels":selector},"spec":spec}}),
    )?;
    let mut service = object("service", 2, "unused", service_spec)?;
    service.spec["selector"] = serde_json::to_value(&f.service_selector)?;
    Ok((
        f,
        [
            (Deployments, inventory(vec![a])),
            (
                ReplicaSets,
                inventory(vec![replica_set("a-rs", 5, 3, &spec)?]),
            ),
            (Pods, inventory(vec![pod("a-pod", 4, 5, "a", &spec)?])),
            (Services, inventory(vec![service])),
            (Slices, inventory(vec![endpoint(4)?])),
        ]
        .into(),
    ))
}
fn add_b(f: &HandoffFixture, all: &mut BTreeMap<watch::Kind, Inventory>) -> Result<()> {
    use watch::Kind::*;
    let mut spec = all[&Deployments].objects[&uid(3)].spec["template"]["spec"].clone();
    spec["containers"][0]["volumeMounts"][0]["readOnly"] = json!(true);
    let mut labels = f.service_selector.clone();
    labels.insert("app.kubernetes.io/component".into(), "b".into());
    let b = object(
        "b",
        6,
        "b",
        json!({"replicas":1,"template":{"metadata":{"labels":labels},"spec":spec}}),
    )?;
    all.get_mut(&Deployments).unwrap().objects.insert(uid(6), b);
    all.get_mut(&ReplicaSets)
        .unwrap()
        .objects
        .insert(uid(8), replica_set("b-rs", 8, 6, &spec)?);
    all.get_mut(&Pods)
        .unwrap()
        .objects
        .insert(uid(7), pod("b-pod", 7, 8, "b", &spec)?);
    Ok(())
}
fn route(all: &mut BTreeMap<watch::Kind, Inventory>, component: &str, pod: u128) -> Result<()> {
    all.get_mut(&watch::Kind::Services)
        .unwrap()
        .objects
        .get_mut(&uid(2))
        .unwrap()
        .spec["selector"]["app.kubernetes.io/component"] = json!(component);
    all.insert(watch::Kind::Slices, inventory(vec![endpoint(pod)?]));
    Ok(())
}
#[test]
fn handoff_rejects_foreign_identity_returned_original_and_wrong_endpoint() -> Result<()> {
    use watch::Kind::*;
    let (f, mut all) = fixture()?;
    let mut receipt = HandoffReceipt::default();
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::OriginalOnly
    )?);
    add_b(&f, &mut all)?;
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::BothReady
    )?);
    let original = all.get_mut(&Pods).unwrap().objects.remove(&uid(4)).unwrap();
    all.get_mut(&Deployments)
        .unwrap()
        .objects
        .get_mut(&uid(3))
        .unwrap()
        .spec["replicas"] = json!(0);
    route(&mut all, "b", 7)?;
    ensure!(
        !model::check(&f, &all, &mut receipt, HandoffPhase::ReplacementOnly)?,
        "absence alone must not manufacture observed process exit"
    );
    let mut exited = original.clone();
    exited.status["containerStatuses"][0]["state"] =
        json!({"terminated":{"exitCode":0,"finishedAt":"2026-10-10T00:01:00Z"}});
    let mut nonzero = exited.clone();
    nonzero.status["containerStatuses"][0]["state"]["terminated"]["exitCode"] = json!(137);
    let mut refused = HandoffReceipt::default();
    ensure!(model::observe_exit(&f, &nonzero, &mut refused).is_err());
    ensure!(
        !refused.original_exit_observed
            && refused.original_exit_code == Some(137)
            && refused.original_exit_finished_at.is_some()
    );
    ensure!(!model::check(
        &f,
        &all,
        &mut refused,
        HandoffPhase::ReplacementOnly
    )?);
    model::observe_exit(&f, &exited, &mut receipt)?;
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::ReplacementOnly
    )?);
    all.get_mut(&Pods).unwrap().objects.insert(uid(4), original);
    ensure!(!model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::ReplacementOnly
    )?);
    all.get_mut(&Pods).unwrap().objects.remove(&uid(4));
    route(&mut all, "b", 4)?;
    ensure!(!model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::ReplacementOnly
    )?);
    route(&mut all, "b", 7)?;
    all.get_mut(&Services)
        .unwrap()
        .objects
        .get_mut(&uid(2))
        .unwrap()
        .metadata
        .uid = uid(90);
    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::ReplacementOnly).is_err());
    Ok(())
}
#[test]
fn handoff_rejects_overlap_config_change_and_incomplete_restoration() -> Result<()> {
    use watch::Kind::*;
    let (f, mut all) = fixture()?;
    let mut receipt = HandoffReceipt::default();
    add_b(&f, &mut all)?;
    all.get_mut(&ReplicaSets)
        .unwrap()
        .objects
        .get_mut(&uid(8))
        .unwrap()
        .spec["template"]["spec"]["containers"][0]["args"] = json!(["stale-config"]);
    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::BothReady).is_err());
    all.get_mut(&ReplicaSets)
        .unwrap()
        .objects
        .get_mut(&uid(8))
        .unwrap()
        .spec["template"]["spec"]["containers"][0]
        .as_object_mut()
        .unwrap()
        .remove("args");
    all.get_mut(&Pods)
        .unwrap()
        .objects
        .get_mut(&uid(7))
        .unwrap()
        .spec["containers"][0]["env"] = json!([{"name":"DATABASE","value":"foreign"}]);
    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::BothReady).is_err());
    all.get_mut(&Pods)
        .unwrap()
        .objects
        .get_mut(&uid(7))
        .unwrap()
        .spec["containers"][0]
        .as_object_mut()
        .unwrap()
        .remove("env");

    let ready_b = all[&Pods].objects[&uid(7)].clone();
    let pending = all
        .get_mut(&Pods)
        .unwrap()
        .objects
        .get_mut(&uid(7))
        .unwrap();
    pending.spec["nodeName"] = Value::Null;
    pending.status = json!({});
    ensure!(
        !model::check(&f, &all, &mut receipt, HandoffPhase::BothReady)?,
        "ordinary pending replacement must wait rather than fail"
    );
    ensure!(
        model::check(&f, &all, &mut receipt, HandoffPhase::OriginalRouting)?,
        "healthy original routing must permit pending-B retirement"
    );
    ensure!(
        !model::check(&f, &all, &mut receipt, HandoffPhase::Restored)?,
        "pending B still requires observed removal"
    );
    all.get_mut(&Pods).unwrap().objects.insert(uid(7), ready_b);

    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::OriginalOnly).is_err());
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::BothReady
    )?);
    all.get_mut(&Deployments)
        .unwrap()
        .objects
        .get_mut(&uid(6))
        .unwrap()
        .spec["template"]["metadata"]["labels"]["app.kubernetes.io/component"] = json!("a");
    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::BothReady).is_err());
    all.get_mut(&Deployments)
        .unwrap()
        .objects
        .get_mut(&uid(6))
        .unwrap()
        .spec["template"]["metadata"]["labels"]["app.kubernetes.io/component"] = json!("b");
    ensure!(!model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::Restored
    )?);
    all.get_mut(&Pods)
        .unwrap()
        .objects
        .get_mut(&uid(7))
        .unwrap()
        .spec["containers"][0]["volumeMounts"][0]["readOnly"] = json!(false);
    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::BothReady).is_err());
    all.get_mut(&Pods)
        .unwrap()
        .objects
        .get_mut(&uid(7))
        .unwrap()
        .spec["containers"][0]["volumeMounts"][0]["readOnly"] = json!(true);
    route(&mut all, "b", 7)?;
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::OriginalRestoredWithReplacementRouting
    )?);
    route(&mut all, "a", 4)?;
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::BothReady
    )?);
    all.get_mut(&Deployments).unwrap().objects.remove(&uid(6));
    ensure!(!model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::Restored
    )?);
    all.get_mut(&Pods).unwrap().objects.remove(&uid(7));
    ensure!(model::check(&f, &all, &mut receipt, HandoffPhase::Restored)? && receipt.restored);
    Ok(())
}

#[test]
fn handoff_barrier_rejects_queued_original_return_foreign_replay_and_missing_marker() -> Result<()>
{
    use watch::Kind::*;
    let (f, mut all) = fixture()?;
    let mut receipt = HandoffReceipt::default();
    add_b(&f, &mut all)?;
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::BothReady
    )?);
    let original = all.get_mut(&Pods).unwrap().objects.remove(&uid(4)).unwrap();
    all.get_mut(&Deployments)
        .unwrap()
        .objects
        .get_mut(&uid(3))
        .unwrap()
        .spec["replicas"] = json!(0);
    route(&mut all, "b", 7)?;
    let mut exited = original.clone();
    exited.status["containerStatuses"][0]["state"] =
        json!({"terminated":{"exitCode":0,"finishedAt":"2026-10-10T00:01:00Z"}});
    model::observe_exit(&f, &exited, &mut receipt)?;
    ensure!(model::check(
        &f,
        &all,
        &mut receipt,
        HandoffPhase::ReplacementOnly
    )?);
    receipt.assertion_versions = model::versions(&f, &all, &receipt)?;
    let nonce = uid(100);
    receipt.barrier_nonce = Some(nonce);
    let mut marker = all[&Pods].objects[&uid(7)].clone();
    model::observe_marker(&mut receipt, &marker)?;
    ensure!(!receipt.barrier_observed, "missing marker cannot qualify");
    marker
        .metadata
        .annotations
        .insert("veoveo.ai/acceptance-fence".into(), uid(99).to_string());
    ensure!(model::observe_marker(&mut receipt, &marker).is_err() && !receipt.barrier_observed);
    marker
        .metadata
        .annotations
        .insert("veoveo.ai/acceptance-fence".into(), nonce.to_string());
    let mut foreign = marker.clone();
    foreign.metadata.uid = uid(91);
    ensure!(model::observe_marker(&mut receipt, &foreign).is_err());
    // The original native Pod watch delivers these in order. A return must stop
    // the barrier consumer before it can accept the following B marker.
    all.get_mut(&Pods).unwrap().objects.insert(uid(4), original);
    let mut consume = || -> Result<()> {
        ensure!(
            model::check(&f, &all, &mut receipt, HandoffPhase::ReplacementOnly)?,
            "queued original return"
        );
        model::observe_marker(&mut receipt, &marker)
    };
    ensure!(consume().is_err() && !receipt.barrier_observed);
    all.get_mut(&Pods).unwrap().objects.remove(&uid(4));
    model::observe_marker(&mut receipt, &marker)?;
    ensure!(receipt.barrier_observed);
    ensure!(model::versions(&f, &all, &receipt)? == receipt.assertion_versions);
    all.get_mut(&Services)
        .unwrap()
        .objects
        .get_mut(&uid(2))
        .unwrap()
        .metadata
        .resource_version = "2".into();
    ensure!(
        model::versions(&f, &all, &receipt)? != receipt.assertion_versions,
        "restored transient selector edits still change Service RV"
    );
    Ok(())
}

#[test]
fn cancelled_owner_admits_cleanup_inventory_with_original_grace() -> Result<()> {
    const KEY: &str = "VEOVEO_TEST_HANDOFF_CANCELLED_INVENTORY";
    if std::env::var_os(KEY).is_none() {
        crate::process::tests::isolated_control(
            "installed::restart::handoff::tests::cancelled_owner_admits_cleanup_inventory_with_original_grace",
            KEY,
            "cancelled",
        );
        return Ok(());
    }
    let root = tempfile::tempdir()?;
    let owner = crate::lifecycle::owner::test_scope(root.path().to_owned(), Duration::from_secs(3));
    crate::lifecycle::owner::test_activate(Some(&owner));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        crate::lifecycle::owner::test_cancel(&owner);
        let original_end = crate::lifecycle::owner::cleanup_deadline()?;
        let mut ordinary = tokio::process::Command::new("/bin/printf");
        ordinary.arg("ordinary must not run");
        ensure!(
            crate::output_async(ordinary, Duration::from_secs(1))
                .await
                .is_err(),
            "ordinary inventory unexpectedly admitted a cancelled owner"
        );
        let mut command = std::process::Command::new("/bin/printf");
        command
            .arg("%s")
            .arg(r#"{"metadata":{"resourceVersion":"selected-current"},"items":[]}"#);
        let mut read = watch::CleanupRead::start(command, original_end - Duration::from_secs(2))?;
        let inventory = read.finish().await?;
        ensure!(inventory.resource_version == "selected-current" && inventory.objects.is_empty());
        read.drain(original_end).await?;
        ensure!(
            crate::lifecycle::owner::cleanup_deadline()? == original_end,
            "cleanup inventory refreshed the owner grace"
        );
        ensure!(
            read.finish().await.is_err(),
            "cleanup inventory replayed its consumed result"
        );
        Ok::<_, anyhow::Error>(())
    });
    crate::lifecycle::owner::test_activate(None);
    result
}

#[test]
fn handoff_five_native_watches_wait_and_protect_on_small_stack() -> Result<()> {
    const KEY: &str = "VEOVEO_TEST_HANDOFF_FINITE_STACK";
    if std::env::var_os(KEY).is_none() {
        crate::process::tests::isolated_control(
            "installed::restart::handoff::tests::handoff_five_native_watches_wait_and_protect_on_small_stack",
            KEY,
            "finite",
        );
        return Ok(());
    }
    // Isolate an overflow from the native test runner. This bounds the shared
    // observation graph, without changing the installed runtime's stack setting.
    std::thread::Builder::new().stack_size(512 * 1024).spawn(|| -> Result<()> {
        let root = tempfile::tempdir()?;
        let owner = crate::lifecycle::owner::test_scope(root.path().to_owned(), Duration::from_secs(10));
        crate::lifecycle::owner::test_activate(Some(&owner));
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let result = runtime.block_on(Box::pin(async {
            use tokio::io::AsyncWriteExt;
            let (f, mut all) = fixture()?;
            add_b(&f, &mut all)?;
            let ready = all[&watch::Kind::Pods].objects[&uid(7)].clone();
            all.get_mut(&watch::Kind::Pods).unwrap().objects.get_mut(&uid(7)).unwrap().status["containerStatuses"][0]["ready"] = json!(false);
            let target = InstallationTarget::decode(&serde_json::to_vec(&json!({
                "schema":"veoveo.ai/installation-target/v1",
                "kubernetes":{"context":"fixture","namespace":"fixture"},
                "localBaseUrl":"http://127.0.0.1:18781","publicBaseUrl":"https://fixture.example.test",
                "controlPlane":"gateway.json","expectedDeployments":["a"],"minimumGpuShares":1,
                "operator":{"clientId":"fixture","profile":"fixture","workContext":"fixture","scopes":["fixture:use"]}
            }))?)?;
            let mut observer = HandoffObserver { target, fixture:f, inventories:all, watches:vec![], receipt:HandoffReceipt::default(), cleanup_read:None };
            let mut inputs = BTreeMap::new();
            for &kind in watch::Kind::ALL {
                let mut command = tokio::process::Command::new("/bin/cat");
                command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
                let mut child = crate::spawn_async(command)?;
                inputs.insert(kind, child.stdin.take().ok_or_else(|| anyhow::anyhow!("native watch stdin absent"))?);
                observer.watches.push(watch::Watch::native_control(kind, child)?);
            }
            let observed = async {
                for input in inputs.values_mut() {
                    input.write_all(br#"{"type":"BOOKMARK","object":{}}"#).await?;
                }
                for _ in watch::Kind::ALL { observer.next().await?; }
                ensure!(observer.receipt.watch_events == 5);
                let event = serde_json::to_vec(&json!({"type":"MODIFIED","object":{
                    "metadata":{"name":"b-pod","uid":uid(7),"resourceVersion":"ready",
                    "labels":{"app.kubernetes.io/component":"b","app.kubernetes.io/instance":"fixture"},
                    "ownerReferences":[{"kind":"ReplicaSet","uid":uid(8),"controller":true}]},
                    "spec":ready.spec,"status":ready.status
                }}))?;
                inputs.get_mut(&watch::Kind::Pods).unwrap().write_all(&event).await?;
                observer.wait(HandoffPhase::BothReady, tokio::time::Instant::now()+Duration::from_secs(2)).await?;
                ensure!(observer.receipt.watch_events == 6);
                // A real native stream invalidates the physical route while the
                // independent owner work is pending; protect must refuse it.
                inputs.get_mut(&watch::Kind::Services).unwrap().write_all(br#"{"type":"MODIFIED","object":{"metadata":{"name":"service","uid":"00000000-0000-0000-0000-000000000002","resourceVersion":"changed"},"spec":{}}}"#).await?;
                let refused = tokio::time::timeout(Duration::from_secs(2), observer.protect(HandoffPhase::BothReady, std::future::pending::<Result<()>>())).await?;
                ensure!(refused.is_err() && observer.receipt.watch_events == 7, "protect lost the streamed route invalidation");
                Ok::<_,anyhow::Error>(())
            }.await;
            let closed = observer.close_until(std::time::Instant::now()+Duration::from_secs(3)).await;
            observed?;
            closed?;
            ensure!(observer.receipt.watches_closed);
            Ok::<_,anyhow::Error>(())
        }));
        crate::lifecycle::owner::test_activate(None);
        result
    })?.join().map_err(|_| anyhow::anyhow!("native handoff stack control panicked"))?
}
