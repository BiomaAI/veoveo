use super::*;
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;
use veoveo_mcp_contract::*;

#[derive(Clone, Copy)]
enum Reply {
    Allowed,
    Denied(u16),
    WrongId,
    WrongUri,
    WrongLength,
}

struct ArtifactEndpoint {
    address: SocketAddr,
    reply: Arc<Mutex<Reply>>,
    requests: Arc<AtomicUsize>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl ArtifactEndpoint {
    fn start(artifact_id: ArtifactId) -> Self {
        Self::with_authority(
            artifact_id,
            format!("GET /artifacts/{artifact_id}/meta HTTP/1.1"),
            "fixture-alice".into(),
        )
    }

    fn with_authority(artifact_id: ArtifactId, request_line: String, secret: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let reply = Arc::new(Mutex::new(Reply::Allowed));
        let requests = Arc::new(AtomicUsize::new(0));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker = {
            let reply = reply.clone();
            let requests = requests.clone();
            let stopped = stopped.clone();
            std::thread::spawn(move || {
                loop {
                    let (mut stream, _) = listener.accept().unwrap();
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut headers = Vec::new();
                    while !headers.ends_with(b"\r\n\r\n") {
                        assert!(headers.len() < 8192, "unbounded fixture request");
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        headers.push(byte[0]);
                    }
                    let headers = String::from_utf8(headers).unwrap();
                    assert_eq!(
                        headers.lines().next().unwrap(),
                        request_line,
                        "a warm cache must authorize metadata without downloading bytes"
                    );
                    requests.fetch_add(1, Ordering::SeqCst);
                    let authorized = headers.lines().any(|line| {
                        line.eq_ignore_ascii_case(&format!("authorization: Bearer {secret}"))
                    });
                    let reply = if authorized {
                        *reply.lock().unwrap()
                    } else {
                        Reply::Denied(403)
                    };
                    let (status, body) = match reply {
                        Reply::Denied(status) => (status, String::new()),
                        reply => {
                            let id = if matches!(reply, Reply::WrongId) {
                                ArtifactId::new()
                            } else {
                                artifact_id
                            };
                            let uri = if matches!(reply, Reply::WrongUri) {
                                ArtifactId::new().plane_uri()
                            } else {
                                artifact_id.plane_uri()
                            };
                            let length = if matches!(reply, Reply::WrongLength) {
                                6
                            } else {
                                5
                            };
                            (
                                200,
                                format!(
                                    r#"{{"artifact_id":"{id}","artifact_uri":"{uri}","byte_len":{length},"created_at":"2026-09-08T00:00:00Z"}}"#
                                ),
                            )
                        }
                    };
                    write!(
                        stream,
                        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .unwrap();
                }
            })
        };
        Self {
            address,
            reply,
            requests,
            stopped,
            worker: Some(worker),
        }
    }

    fn stop(&mut self) -> std::thread::Result<()> {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            // Wake accept; no fixture request is processed after the stop flag.
            let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(2));
            worker.join()?;
        }
        Ok(())
    }
}

impl Drop for ArtifactEndpoint {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

struct FixtureIdentity(Arc<AtomicUsize>);

impl RrdIdentityValidator for FixtureIdentity {
    fn validate(&self, path: &Path, byte_len: u64, sha256: &str) -> Result<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() as u64 == byte_len && hex::encode(Sha256::digest(&bytes)) == sha256,
            "fixture byte identity mismatch"
        );
        Ok(())
    }
}

#[test]
fn warm_cache_requires_current_artifact_read_permission_and_exact_metadata() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let artifact_id = ArtifactId::new();
        let digest = hex::encode(Sha256::digest(b"valid"));
        let path = directory.path().join(format!("{artifact_id}-{digest}.rrd"));
        std::fs::write(&path, b"valid").unwrap();
        let mut endpoint = ArtifactEndpoint::start(artifact_id);
        let cache = LayerCache::new(
            directory.path().to_owned(),
            LayerCacheLimits {
                managed_bytes: 1024,
                minimum_free_bytes: 1,
            },
            HttpArtifactPlane::new(format!("http://{}", endpoint.address)),
        )
        .unwrap();
        let validations = Arc::new(AtomicUsize::new(0));
        let validation: Arc<dyn RrdIdentityValidator> =
            Arc::new(FixtureIdentity(validations.clone()));
        let alice = caller("alice");
        let bob = caller("bob");
        let read = |caller| {
            cache.materialize_with_validator(
                ArtifactReadAuthority::Caller(caller),
                artifact_id,
                5,
                &digest,
                validation.clone(),
            )
        };

        drop(read(&alice).await.unwrap());
        assert_eq!(validations.load(Ordering::SeqCst), 1);
        assert!(
            read(&bob).await.is_err(),
            "another caller inherited cached access"
        );
        for reply in [
            Reply::Denied(403), // revoked grant
            Reply::Denied(401), // expired credential
            Reply::Denied(404), // deleted occurrence
            Reply::WrongId,
            Reply::WrongUri,
            Reply::WrongLength,
        ] {
            *endpoint.reply.lock().unwrap() = reply;
            assert!(read(&alice).await.is_err());
            assert_eq!(
                validations.load(Ordering::SeqCst),
                1,
                "rejection must precede local byte access"
            );
            assert_eq!(cache.stats().unwrap().pinned_bytes, 0);
            assert_eq!(std::fs::read(&path).unwrap(), b"valid");
        }
        *endpoint.reply.lock().unwrap() = Reply::Allowed;
        drop(read(&alice).await.unwrap());
        assert_eq!(validations.load(Ordering::SeqCst), 2);
        assert_eq!(endpoint.requests.load(Ordering::SeqCst), 9);
        endpoint.stop().unwrap();
        assert!(
            read(&alice).await.is_err(),
            "offline cache bypassed Artifact authority"
        );
        assert_eq!(cache.stats().unwrap().pinned_bytes, 0);
        assert_eq!(validations.load(Ordering::SeqCst), 2);
    });
}

fn caller(name: &str) -> PlaneCaller {
    let now = Utc::now();
    let actor = Principal {
        id: PrincipalId::new(name).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        subject: TokenSubject::new(name).unwrap(),
        tenant: Some(TenantId::new("fixture").unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(now),
    };
    PlaneCaller {
        bearer_token: format!("fixture-{name}"),
        memberships: BTreeSet::new(),
        identity: GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new("operator").unwrap(),
            server: ServerSlug::new("stream").unwrap(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("mission").unwrap(),
                tenant: TenantId::new("fixture").unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(actor.id.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: actor.id.clone(),
                },
            },
            actor,
            jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + chrono::TimeDelta::minutes(5),
        },
    }
}

#[tokio::test]
async fn task_capability_reauthorizes_a_reopened_cache_and_rejects_revoked_or_wrong_task() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let id = ArtifactId::new();
    let digest = hex::encode(Sha256::digest(b"valid"));
    std::fs::write(
        directory.path().join(format!("{id}-{digest}.rrd")),
        b"valid",
    )
    .unwrap();
    let cap = IssuedArtifactReadCapability {
        capability_id: ArtifactReadCapabilityId::new(),
        task_id: ArtifactTaskId::new(),
        secret: ArtifactReadCapabilitySecret::new("task-read-fixture-secret-0123456789").unwrap(),
        expires_at: Utc::now() + chrono::TimeDelta::hours(1),
    };
    let endpoint = ArtifactEndpoint::with_authority(
        id,
        format!(
            "GET /artifact-read-capabilities/{}/artifacts/{id}/meta?task_id={} HTTP/1.1",
            cap.capability_id, cap.task_id
        ),
        cap.secret.expose_secret().to_owned(),
    );
    let cache = LayerCache::new(
        directory.path().to_owned(),
        LayerCacheLimits {
            managed_bytes: 1024,
            minimum_free_bytes: 1,
        },
        HttpArtifactPlane::new(format!("http://{}", endpoint.address)),
    )
    .unwrap();
    let validations = Arc::new(AtomicUsize::new(0));
    let validator: Arc<dyn RrdIdentityValidator> = Arc::new(FixtureIdentity(validations.clone()));
    let authority = ArtifactReadAuthority::Task {
        capability: &cap,
        task_id: cap.task_id,
    };
    drop(
        cache
            .materialize_with_validator(authority, id, 5, &digest, validator.clone())
            .await
            .unwrap(),
    );
    assert_eq!(validations.load(Ordering::SeqCst), 1);
    for status in [401, 403, 404] {
        *endpoint.reply.lock().unwrap() = Reply::Denied(status);
        assert!(
            cache
                .materialize_with_validator(authority, id, 5, &digest, validator.clone())
                .await
                .is_err()
        );
        assert_eq!(cache.stats().unwrap().pinned_bytes, 0);
        assert_eq!(validations.load(Ordering::SeqCst), 1);
    }
    let wrong = ArtifactReadAuthority::Task {
        capability: &cap,
        task_id: ArtifactTaskId::new(),
    };
    let requests = endpoint.requests.load(Ordering::SeqCst);
    assert!(
        cache
            .materialize_with_validator(wrong, id, 5, &digest, validator)
            .await
            .is_err()
    );
    assert_eq!(endpoint.requests.load(Ordering::SeqCst), requests);
}
