//! Lifecycle fault injection uses an owned CLI stand-in, with no Docker daemon or secrets.
#[path = "../../../testing/fixtures/store/container.rs"]
mod container;

use container::{Container, Docker};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};

struct CliFixture(PathBuf);

impl CliFixture {
    fn new(create: &str, start: &str, port: &str, cleanup: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("veoveo-fixture-cli-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&root).unwrap();
        let script = format!(
            "#!/bin/sh\nroot=$(dirname \"$0\")\ncase \"$1\" in\n\
             create) printf '%s' \"$$\" > \"$root/pid\"; {create};;\n\
             start) {start};;\n\
             port) {port};;\n\
             rm) printf '%s\\n' \"$@\" > \"$root/cleanup\"; {cleanup};;\n\
             *) exit 99;;\nesac\n"
        );
        let program = root.join("docker");
        fs::write(&program, script).unwrap();
        fs::set_permissions(program, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }

    fn docker(&self) -> Docker {
        Docker {
            program: self.0.join("docker"),
            command_timeout: Duration::from_millis(300),
        }
    }

    fn assert_cleanup(&self) {
        let args = fs::read_to_string(self.0.join("cleanup")).unwrap();
        assert!(args.starts_with("rm\n--force\n--volumes\nveoveo-native-store-test-"));
        assert_eq!(args.lines().count(), 4);
    }

    async fn assert_child_reaped(&self) {
        let pid = fs::read_to_string(self.0.join("pid")).unwrap();
        let process = PathBuf::from(format!("/proc/{pid}"));
        tokio::time::timeout(Duration::from_secs(2), async {
            while process.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("cancelled fixture CLI was not killed and reaped");
    }
}

impl Drop for CliFixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[tokio::test]
async fn successful_setup_transfers_ownership_until_drop() {
    let fixture = CliFixture::new("exit 0", "exit 0", "printf '127.0.0.1:18542\\n'", "exit 0");
    let (container, endpoint) =
        Container::start(fixture.docker(), "memory", "private-fixture-password")
            .await
            .unwrap();
    assert_eq!(endpoint, "ws://127.0.0.1:18542");
    assert!(!fixture.0.join("cleanup").exists());
    drop(container);
    fixture.assert_cleanup();
}

#[tokio::test]
async fn failed_creation_cleans_up_and_redacts_child_output() {
    let fixture = CliFixture::new(
        "printf '%s' \"$SURREAL_PASS\"; printf '%s' \"$SURREAL_PASS\" >&2; exit 23",
        "exit 99",
        "exit 99",
        "exit 0",
    );
    let error = Container::start(fixture.docker(), "memory", "private-fixture-password")
        .await
        .err()
        .expect("creation must fail");
    assert_eq!(error.stage, "container creation");
    assert!(error.to_string().contains("23"));
    assert!(!format!("{error:?} {error}").contains("private-fixture-password"));
    fixture.assert_cleanup();
}

#[tokio::test]
async fn command_deadline_kills_the_cli_and_cleans_up() {
    let fixture = CliFixture::new("exec sleep 30", "exit 99", "exit 99", "exit 0");
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        Container::start(fixture.docker(), "memory", "private-fixture-password"),
    )
    .await
    .expect("fixture command deadline did not run")
    .err()
    .expect("creation must time out");
    assert_eq!(error.stage, "container creation");
    assert!(error.to_string().contains("deadline exceeded"));
    fixture.assert_cleanup();
    fixture.assert_child_reaped().await;
}

#[tokio::test]
async fn caller_cancellation_also_kills_the_cli_and_cleans_up() {
    let fixture = CliFixture::new("exec sleep 30", "exit 99", "exit 99", "exit 0");
    let mut docker = fixture.docker();
    docker.command_timeout = Duration::from_secs(30);
    let result = tokio::time::timeout(
        Duration::from_millis(300),
        Container::start(docker, "memory", "private-fixture-password"),
    )
    .await;
    assert!(result.is_err());
    fixture.assert_cleanup();
    fixture.assert_child_reaped().await;
}

#[tokio::test]
async fn malformed_or_non_loopback_ports_fail_with_owned_cleanup() {
    for value in [
        "0.0.0.0:18542",
        "127.0.0.1:0",
        "127.0.0.1:70000",
        "127.0.0.1:1\n127.0.0.1:2",
    ] {
        let fixture = CliFixture::new("exit 0", "exit 0", &format!("printf '{value}'"), "exit 0");
        let error = Container::start(fixture.docker(), "memory", "private-fixture-password")
            .await
            .err()
            .expect("invalid published endpoint must fail");
        assert_eq!(error.stage, "published port");
        assert!(!error.to_string().contains(value));
        fixture.assert_cleanup();
    }
}

#[tokio::test]
async fn cleanup_has_its_own_deadline() {
    let fixture = CliFixture::new(
        "exit 0",
        "exit 0",
        "printf '127.0.0.1:18542\\n'",
        "exec sleep 30",
    );
    let (container, _) = Container::start(fixture.docker(), "memory", "private-fixture-password")
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let cleanup = std::panic::catch_unwind(|| drop(container));
    assert!(
        cleanup.is_err(),
        "cleanup failure must fail the owning test"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    fixture.assert_cleanup();
}
