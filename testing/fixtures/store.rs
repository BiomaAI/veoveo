//! Shared Rust fixture: exact disposable store, no installation data or credentials.
use std::{process::Command, time::Duration};
use uuid::Uuid;
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
const IMAGE: &str =
    "surrealdb/surrealdb@sha256:51baed8709f57f67dcf04b30e3177db846803fa9342dae2be58c6fa5f8d59843";
fn fixture_password() -> String {
    format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple())
}
pub struct TestDb {
    container: String,
    pub a: PlatformStore,
    pub b: PlatformStore,
}
struct PendingContainer(String);
impl Drop for PendingContainer {
    fn drop(&mut self) {
        stop(&self.0);
    }
}
fn stop(name: &str) {
    let _ = Command::new("docker")
        .args(["rm", "--force", name])
        .output();
}
impl Drop for TestDb {
    fn drop(&mut self) {
        stop(&self.container);
    }
}

impl TestDb {
    pub async fn new() -> Self {
        let name = format!("veoveo-native-store-test-{}", Uuid::now_v7().simple());
        let password = fixture_password();
        let output = Command::new("docker")
            .args([
                "run",
                "--detach",
                "--rm",
                "--pull",
                "never",
                "--name",
                &name,
                "--runtime",
                "runc",
                "--memory",
                "2g",
                "--cpus",
                "2",
                "--pids-limit",
                "256",
                "--publish",
                "127.0.0.1::8000",
                "--env",
                "SURREAL_USER",
                "--env",
                "SURREAL_PASS",
                IMAGE,
                "start",
                "--log",
                "error",
                "memory",
            ])
            .env("SURREAL_USER", "fixture_admin")
            .env("SURREAL_PASS", &password)
            .output()
            .expect("docker required");
        assert!(
            output.status.success(),
            "isolated pinned SurrealDB fixture could not start"
        );
        let pending = PendingContainer(name.clone());
        let port = Command::new("docker")
            .args(["port", &name, "8000/tcp"])
            .output()
            .unwrap();
        assert!(port.status.success());
        let port = String::from_utf8(port.stdout).unwrap();
        let port = port
            .trim()
            .strip_prefix("127.0.0.1:")
            .expect("fixture is loopback only");
        let endpoint = format!("ws://127.0.0.1:{port}");
        let database = format!("fixture_{}", Uuid::now_v7().simple());
        let config = StoreConfig::builder(
            &endpoint,
            "veoveo_fixture",
            &database,
            StoreCredentials::root("fixture_admin", password),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap();
        let admin = tokio::time::timeout(Duration::from_secs(60), async {
            loop {
                match PlatformStore::connect(config.clone()).await {
                    Ok(store) => break store,
                    // Retrying deterministic schema failures cannot make this
                    // fresh fixture ready and previously hid the useful error
                    // behind a full minute of reconnects. Migration diagnostics
                    // contain only repository-owned SQL, never fixture secrets.
                    Err(
                        error @ (veoveo_platform_store::StoreError::MigrationExecution { .. }
                        | veoveo_platform_store::StoreError::Migration(_)
                        | veoveo_platform_store::StoreError::Config(_)),
                    ) => {
                        panic!("isolated store initialization failed: {error}");
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        })
        .await
        .expect("isolated migrations/readiness failed");
        let runtime_password = fixture_password();
        admin
            .replace_database_editor("fixture_runtime", &runtime_password.clone().into())
            .await
            .unwrap();
        let config = StoreConfig::builder(
            &endpoint,
            "veoveo_fixture",
            database,
            StoreCredentials::database("fixture_runtime", runtime_password),
        )
        .build()
        .unwrap();
        let a = PlatformStore::connect(config.clone()).await.unwrap();
        let b = PlatformStore::connect(config).await.unwrap();
        // Transfer cleanup ownership only after every fallible setup action succeeds.
        std::mem::forget(pending);
        Self {
            container: name,
            a,
            b,
        }
    }
}
