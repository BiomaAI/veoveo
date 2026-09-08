//! Native database lifecycle shared by artifact durability acceptance.
use chrono::Utc;
use secrecy::SecretString;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use veoveo_mcp_contract::PlaneCaller;
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

    pub(super) async fn connect_unmigrated(&mut self) -> platform::PlatformStore {
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
        .migrate_on_connect(false)
        .build()
        .unwrap();
        platform::PlatformStore::connect(config)
            .await
            .expect("native database authentication failed")
    }

    pub(super) async fn connect(&mut self) -> platform::PlatformStore {
        let store = self.connect_unmigrated().await;
        store
            .migrate()
            .await
            .expect("native schema migration failed");
        store
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

pub(super) async fn context(
    store: &platform::PlatformStore,
    actor: &PlaneCaller,
) -> platform::RecordId {
    let identity = store
        .ensure_identity(
            actor.tenant().unwrap().as_str(),
            actor.identity.actor.id.as_str(),
            actor.identity.actor.issuer.as_str(),
            actor.identity.actor.subject.as_str(),
            platform::PrincipalKind::User,
        )
        .await
        .unwrap();
    let id = platform::deterministic_work_context_id(
        &identity.tenant_key,
        actor.identity.authority.work_context.as_str(),
    )
    .unwrap()
    .record_id();
    let context = platform::WorkContextRecord {
        id: id.clone(),
        tenant: identity.tenant_id.record_id(),
        context_key: actor.identity.authority.work_context.to_string(),
        title: "Read delegation fixture".into(),
        policy_revision: "r1".into(),
        output_policy: platform::WorkContextOutputPolicyRecord {
            owner_kind: platform::ArtifactGrantSubjectKind::Principal,
            owner_key: identity.principal_key.clone(),
            initial_grants: vec![],
            classification: None,
            data_labels: vec![],
        },
        memberships: vec![platform::WorkContextMembershipRuleRecord {
            level: platform::WorkContextMembershipLevel::Owner,
            principals: vec![identity.principal_key],
            groups: vec![],
            roles: vec![],
            oauth_clients: vec![],
        }],
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    store
        .client()
        .query("CREATE ONLY $record CONTENT $content;")
        .bind(("record", id.clone()))
        .bind(("content", context))
        .await
        .unwrap()
        .check()
        .unwrap();
    id
}
