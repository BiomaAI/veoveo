//! Real allocator process, mTLS worker client and isolated Docker daemon.
#[path = "../../../runtimes/computers/tests/native_support/docker_daemon.rs"]
mod docker_daemon;
#[path = "native_support/service.rs"]
mod native_service_support;

use docker_daemon::checked;
use native_service_support::Fixture;
use veoveo_computers_runtime::{AllocationConfig, HomeAllocator};

#[tokio::test]
#[ignore = "requires pinned local Computer/Docker images and an ext4 shared host subtree"]
async fn native_service_shared_mount_and_restart() {
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
    fixture.start_container("a", true).await;
    fixture.copy_and_assert("during restart").await;

    // Removal of A is necessary but does not grant an unrecorded instance B.
    fixture.remove("a").await;
    fixture.create("b", &fixture.replacement).await;
    fixture.start_container("b", false).await;
    fixture.remove("b").await;
    fixture.create("a", &fixture.initial).await;
    fixture.start_container("a", true).await;
    fixture.copy_and_assert("during restart").await;
    fixture.finish().await;
}
