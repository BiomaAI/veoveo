//! Qualify the Docker boundary before adopting a production allocator. This
//! fixture exposes a disposable directory, not a retained production allocation.
#[path = "native_support/volume_plugin.rs"]
mod plugin;
use plugin::VolumeFixture;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires local Docker, the pinned Docker 29.8.0 DinD image and Computer image; owns an isolated daemon, plugin and disposable containers"]
async fn docker_volume_boundary_excludes_competing_registered_writers() {
    let fixture = VolumeFixture::start().await;
    fixture.create("a", true).await;
    fixture.start_container("a", true).await;
    fixture.write("a", "first").await;
    // docker cp uses a nested mount. Its matching Unmount cannot free the home
    // while the first container is still alive.
    fixture.copy_from("a").await;
    fixture.create("b", true).await;
    fixture.start_container("b", false).await;
    fixture.write("a", "still-first").await;
    fixture.remove_container("b").await;
    fixture.stop_container("a").await;
    fixture.start_container("a", true).await;
    fixture.assert_content("a", "still-first").await;
    fixture.stop_container("a").await;
    // A stopped consumer still reserves the same volume. The writer boundary
    // cannot move to another resource until the old container is removed.
    fixture.create("b", true).await;
    fixture.start_container("b", false).await;
    fixture.remove_container("a").await;
    // Removal alone cannot admit a differently named instance.
    fixture.start_container("b", false).await;
    fixture.admit_replacement().await;
    fixture.start_container("b", true).await;
    fixture.assert_content("b", "still-first").await;
    fixture.write("b", "second").await;
    fixture.remove_container("b").await;
    // A delayed old Create cannot regain the home even as its sole consumer.
    fixture.create("a", true).await;
    fixture.start_container("a", false).await;
    fixture.remove_container("a").await;
    fixture.create("b", true).await;
    fixture.start_container("b", true).await;
    fixture.assert_content("b", "second").await;
    fixture.finish().await;
}
