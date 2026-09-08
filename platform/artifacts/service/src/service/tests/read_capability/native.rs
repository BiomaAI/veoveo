//! A real SurrealDB process, independent service instances, and atomic read quotas.
use super::*;
use secrecy::SecretString;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use veoveo_platform_store as platform;

struct Database {
    child: Child,
    endpoint: String,
    password: String,
    _logs: tempfile::TempDir,
}

impl Database {
    fn start() -> Self {
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

    async fn connect(&mut self) -> platform::PlatformStore {
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
            "delegation",
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

    fn finish(&mut self) {
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

async fn context(store: &platform::PlatformStore, actor: &PlaneCaller) -> platform::RecordId {
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

#[tokio::test]
#[ignore = "requires VEOVEO_SURREAL_BINARY; owns and removes an isolated in-memory SurrealDB 3.2.4 process"]
async fn artifact_read_delegation_survives_service_recreation_and_enforces_native_atomic_state() {
    let mut database = Database::start();
    let store = database.connect().await;
    let mut alice = caller("alice", "acme", &["recordings"]);
    alice
        .memberships
        .insert(veoveo_mcp_contract::GroupMembership {
            group: veoveo_mcp_contract::GroupId::new("readers").unwrap(),
            role: veoveo_mcp_contract::GroupRole::Read,
        });
    let context_id = context(&store, &alice).await;
    let blobs = InMemoryBlobStore::default();
    let service = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(store.clone()),
        blobs.clone(),
        "http://fixture",
        1024,
    );
    let first = service
        .put(&alice, PutArtifactRequest::default(), vec![1; 3])
        .await
        .unwrap()
        .artifact_id;
    let second = service
        .put(&alice, PutArtifactRequest::default(), vec![2; 3])
        .await
        .unwrap()
        .artifact_id;
    let cap = service
        .issue_read_capability(&alice, request(2, 5))
        .await
        .unwrap();
    drop(service);
    let other_store = database.connect().await;
    let one = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(store.clone()),
        blobs.clone(),
        "http://fixture",
        1024,
    );
    let two = ArtifactService::with_options(
        crate::SurrealArtifactRepository::new(other_store),
        blobs,
        "http://fixture",
        1024,
    );
    let (first_result, second_result) =
        tokio::join!(read(&one, &cap, first), read(&two, &cap, second));
    assert_eq!(
        usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
        1,
        "native quota admitted two competing reads or neither: {first_result:?}, {second_result:?}"
    );
    let winner = if first_result.is_ok() { first } else { second };
    read(&two, &cap, winner).await.unwrap();
    let record =
        platform::ArtifactReadCapabilityId::from_uuid(cap.capability_id.as_uuid()).record_id();
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record;")
        .bind(("record", record.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let persisted: platform::ArtifactReadCapabilityRecord =
        response.take::<Option<_>>(0).unwrap().unwrap();
    assert_eq!(persisted.used_total_bytes, 3);
    assert_eq!(persisted.admitted_artifacts.len(), 1);
    assert_eq!(persisted.memberships.len(), 1);
    assert_eq!(persisted.memberships[0].group_key, "readers");
    assert_eq!(persisted.labels, ["recordings"]);
    assert_ne!(persisted.token_hash, cap.secret.expose_secret());
    store
        .client()
        .query("UPDATE $record SET expires_at = $past;")
        .bind(("record", record))
        .bind(("past", Utc::now() - TimeDelta::seconds(1)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        matches!(
            read(&one, &cap, winner).await,
            Err(ArtifactPlaneError::Unauthenticated)
        ),
        "expired admission was replayed"
    );

    let cap = one
        .issue_read_capability(&alice, request(2, 100))
        .await
        .unwrap();
    read(&two, &cap, first).await.unwrap();
    two.revoke_read_capability(&alice, cap.capability_id)
        .await
        .unwrap();
    assert!(matches!(
        read(&one, &cap, first).await,
        Err(ArtifactPlaneError::Unauthenticated)
    ));
    let cap = one
        .issue_read_capability(&alice, request(2, 100))
        .await
        .unwrap();
    read(&two, &cap, first).await.unwrap();
    store
        .client()
        .query("UPDATE $record SET updated_at = $changed;")
        .bind(("record", context_id.clone()))
        .bind(("changed", Utc::now() + TimeDelta::seconds(1)))
        .await
        .unwrap()
        .check()
        .unwrap();
    read(&two, &cap, first)
        .await
        .expect("metadata-only context update invalidated delegation");
    store
        .client()
        .query("UPDATE $record SET memberships = [];")
        .bind(("record", context_id))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        matches!(
            read(&two, &cap, first).await,
            Err(ArtifactPlaneError::Unauthenticated)
        ),
        "changed Work Context policy retained delegation"
    );
    drop(one);
    drop(two);
    drop(store);
    database.finish();
    println!("Native read delegation verified across service instances; database process reaped.");
}
