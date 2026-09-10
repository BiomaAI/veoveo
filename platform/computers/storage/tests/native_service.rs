//! Real allocator process, mTLS worker client and isolated Docker daemon.
#[path = "../../../runtimes/computers/tests/native_support/docker_daemon.rs"]
mod docker_daemon;
#[path = "native_support/service.rs"]
#[allow(dead_code)] // This shared fixture also serves the native worker scenario.
mod native_service_support;

use docker_daemon::checked;
use native_service_support::Fixture;
use veoveo_computers_runtime::{AllocationConfig, Binding, HomeAllocator};

#[tokio::test]
#[ignore = "requires pinned local Computer/Docker images and an ext4 shared host subtree"]
async fn native_service_shared_mount_and_restart() {
    if std::env::var_os(docker_daemon::registry_relay::CHILD_ENV).is_some() {
        docker_daemon::registry_relay::child().await.unwrap();
        return;
    }
    if std::env::var_os("VEOVEO_STORAGE_SERVICE_CLEANUP").is_some() {
        native_service_support::cleanup();
        return;
    }
    let fixture = Fixture::start().await;
    let worker = fixture.worker(fixture.provider).await;
    worker.ready().await.unwrap();
    fixture
        .worker(uuid::Uuid::now_v7())
        .await
        .ready()
        .await
        .unwrap_err();
    let guest = HomeAllocator::new(
        AllocationConfig::new(
            fixture.endpoint.clone(),
            fixture.dir.join("tls/ca.pem"),
            fixture.dir.join("guest/client.pem"),
            fixture.dir.join("guest/client-key.pem"),
        )
        .unwrap(),
        fixture.provider,
        "f".repeat(64),
        512 * 1024 * 1024,
    )
    .await
    .unwrap();
    guest.ready().await.unwrap_err();

    worker.prepare(&fixture.initial).await.unwrap();
    worker.prepare(&fixture.initial).await.unwrap();
    worker.restore(&fixture.replacement).await.unwrap_err();
    fixture.create("a", &fixture.initial).await;
    fixture.start_container("a", true).await;
    fixture.write("before restart").await;
    fixture.create("b", &fixture.replacement).await;
    fixture.start_container("b", false).await;
    fixture.remove("b").await;
    fixture.copy_and_assert("before restart").await;

    // The mounted filesystem propagates to the host and survives helper exit.
    // A live Computer remains usable while the helper process is absent.
    fixture.stop_service().await;
    fixture.write("during restart").await;
    fixture.start_service().await;
    worker.restore(&fixture.initial).await.unwrap();
    fixture.copy_and_assert("during restart").await;
    checked(
        fixture
            .docker()
            .args(["stop", "--time", "1", &fixture.container("a")]),
    )
    .await;
    // Replace both processes with retained daemon data and journal in place.
    // The helper must serve retained metadata while Docker restores its API.
    fixture.cold_restart().await;
    worker.ready().await.unwrap();
    worker.restore(&fixture.initial).await.unwrap();
    fixture.start_container("a", true).await;
    fixture.copy_and_assert("during restart").await;

    let operation = uuid::Uuid::now_v7();
    worker
        .handoff(
            operation,
            &fixture.initial,
            &fixture.replacement,
            "wrong-resource",
        )
        .await
        .unwrap_err();
    worker
        .handoff(
            operation,
            &fixture.initial,
            &fixture.replacement,
            "resource-a",
        )
        .await
        .unwrap_err();
    fixture.hold_home().await;
    // Removal of A is necessary but does not grant an unrecorded instance B,
    // nor a new Docker container copying A's old binding/resource labels.
    fixture.remove("a").await;
    fixture.create("b", &fixture.replacement).await;
    fixture.start_container("b", false).await;
    fixture.remove("b").await;
    fixture.create("a", &fixture.initial).await;
    fixture.start_container("a", false).await;
    fixture.remove("a").await;
    worker
        .handoff(
            operation,
            &fixture.initial,
            &fixture.replacement,
            "resource-a",
        )
        .await
        .unwrap_err();
    worker.restore(&fixture.replacement).await.unwrap_err();
    // The failed handoff did not mistake lazy loop detachment for exclusion.
    fixture.write_held("held writer remains physical").await;
    fixture.release_home().await;
    fixture.drop_handoff_reply(operation).await;
    worker
        .handoff(
            operation,
            &fixture.initial,
            &fixture.replacement,
            "resource-a",
        )
        .await
        .unwrap();
    worker
        .handoff(
            uuid::Uuid::now_v7(),
            &fixture.initial,
            &fixture.replacement,
            "resource-a",
        )
        .await
        .unwrap_err();
    worker.restore(&fixture.initial).await.unwrap_err();
    fixture.create("b", &fixture.replacement).await;
    fixture.start_container("b", true).await;
    fixture
        .assert_content("b", "held writer remains physical")
        .await;
    fixture.stop_service().await;
    fixture.start_service().await;
    worker
        .handoff(
            operation,
            &fixture.initial,
            &fixture.replacement,
            "resource-a",
        )
        .await
        .unwrap();
    fixture
        .assert_content("b", "held writer remains physical")
        .await;
    fixture.remove("b").await;
    fixture.create("a", &fixture.initial).await;
    fixture.start_container("a", false).await;
    fixture.remove("a").await;
    let next = Binding::replacement(
        fixture.initial.computer_id(),
        uuid::Uuid::now_v7(),
        "e".repeat(64),
    )
    .unwrap();
    let next_worker = fixture
        .worker_template(fixture.provider, next.template_fingerprint())
        .await;
    next_worker
        .handoff(
            uuid::Uuid::now_v7(),
            &fixture.replacement,
            &next,
            "resource-b",
        )
        .await
        .unwrap();
    worker
        .handoff(
            operation,
            &fixture.initial,
            &fixture.replacement,
            "resource-a",
        )
        .await
        .unwrap_err();
    worker.restore(&fixture.replacement).await.unwrap_err();
    fixture.create("c", &next).await;
    fixture.start_container("c", true).await;
    fixture
        .assert_content("c", "held writer remains physical")
        .await;
    // A previously used instance cannot become a new target. Its immutable
    // transition record survives later template changes.
    worker
        .handoff(
            uuid::Uuid::now_v7(),
            &next,
            &fixture.replacement,
            "resource-c",
        )
        .await
        .unwrap_err();
    fixture
        .assert_content("c", "held writer remains physical")
        .await;
    fixture.finish(Some("c")).await;
}
