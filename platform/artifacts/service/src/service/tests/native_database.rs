//! Native database lifecycle shared by artifact durability acceptance.
use secrecy::SecretString;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use veoveo_platform_store as platform;

pub(super) struct Database {
    child: Child,
    endpoint: String,
    password: String,
    _logs: tempfile::TempDir,
}

impl Database {
    pub(super) fn start() -> Self {
        let binary = std::env::var("VEOVEO_SURREAL_BINARY")
            .expect("set the exact SurrealDB 3.2.4 executable");
        let version = Command::new(&binary).arg("version").output().unwrap();
        assert!(version.status.success());
        assert!(
            String::from_utf8(version.stdout)
                .unwrap()
                .starts_with("3.2.4")
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let password = uuid::Uuid::now_v7().to_string();
        let logs = tempfile::tempdir().unwrap();
        let log = std::fs::File::create(logs.path().join("surreal.log")).unwrap();
        let child = Command::new(binary)
            .args([
                "start",
                "--no-banner",
                "--bind",
                &address.to_string(),
                "memory",
            ])
            .env("SURREAL_USER", "fixture")
            .env("SURREAL_PASS", &password)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        Self {
            child,
            endpoint: format!("ws://{address}"),
            password,
            _logs: logs,
        }
    }

    pub(super) async fn connect(&mut self) -> platform::PlatformStore {
        let start = Instant::now();
        let health = self.endpoint.replacen("ws://", "http://", 1) + "/health";
        let http = reqwest::Client::new();
        loop {
            assert!(
                self.child.try_wait().unwrap().is_none(),
                "fixture database exited"
            );
            if http
                .get(&health)
                .timeout(Duration::from_secs(1))
                .send()
                .await
                .is_ok_and(|response| response.status().is_success())
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "fixture database failed readiness"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let config = platform::StoreConfig::builder(
            &self.endpoint,
            "artifact_fixture",
            "fixture",
            platform::StoreCredentials::root("fixture", SecretString::from(self.password.clone())),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap();
        platform::PlatformStore::connect(config)
            .await
            .expect("native schema migration or database authentication failed")
    }

    pub(super) fn finish(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        self.child.wait().unwrap();
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
