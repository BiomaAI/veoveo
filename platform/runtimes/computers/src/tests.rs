use super::*;
use crate::protocol::{datamodel::v1::ObjectMeta, sandbox::v1 as policy, v1 as api};
use futures::{Stream, stream};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tonic::{
    Request, Response, Status,
    transport::{Certificate, Identity, Server, ServerTlsConfig},
};
use uuid::Uuid;
mod allocation_tests;
mod instance_tests;
mod policy_tests;
mod recovery_tests;
mod terminal_tests;
type BoxStream<T> =
    std::pin::Pin<Box<dyn Stream<Item = std::result::Result<T, Status>> + Send + 'static>>;

fn policy() -> policy::SandboxPolicy {
    policy::SandboxPolicy {
        version: 1,
        filesystem: Some(policy::FilesystemPolicy {
            include_workdir: true,
            read_only: vec!["/usr".into()],
            read_write: vec!["/tmp".into()],
        }),
        landlock: Some(policy::LandlockPolicy {
            compatibility: "hard_requirement".into(),
        }),
        process: Some(policy::ProcessPolicy {
            run_as_user: "10001".into(),
            run_as_group: "10001".into(),
        }),
        ..Default::default()
    }
}
fn template(home: bool) -> DevelopmentTemplate {
    let mut policy = policy();
    if home {
        let fs = policy.filesystem.as_mut().unwrap();
        fs.include_workdir = false;
        fs.read_only.push("/sandbox".into());
        fs.read_write.push(PERSISTENT_HOME.into());
    }
    DevelopmentTemplate::new(
        format!("registry.example/dev@sha256:{}", "a".repeat(64)),
        16,
        65536,
        policy,
        if home {
            PERSISTENT_COMMAND.iter().map(|s| s.to_string()).collect()
        } else {
            vec!["/bin/bash".into(), "-l".into()]
        },
        home.then(|| PersistentHome::new(32768, 1024).unwrap()),
    )
    .unwrap()
}
fn binding() -> Binding {
    Binding::new(
        Uuid::parse_str("3c4c49ee-4f42-442d-a761-71a15466b992").unwrap(),
        template(false).fingerprint(),
    )
    .unwrap()
}
fn sandbox(phase: Phase) -> api::Sandbox {
    let binding = binding();
    api::Sandbox {
        metadata: Some(ObjectMeta {
            id: "sandbox-1".into(),
            name: binding.name(),
            workspace: "computers".into(),
            labels: binding.labels(),
            ..Default::default()
        }),
        spec: Some(template(false).spec(binding.computer_id()).unwrap()),
        status: Some(api::SandboxStatus {
            phase: phase as i32,
            main_process_instance_id: "main-1".into(),
            ..Default::default()
        }),
    }
}
#[test]
fn python_fingerprint_and_full_binding_goldens() {
    assert_eq!(
        template(false).fingerprint(),
        "e5fade003b74055ca5103c7277a0259879617098235ec72ba3afe561fc83f77d"
    );
    assert_eq!(
        template(true).fingerprint(),
        "7c3ad949f9ca4a4876541bdb3e322b99251ce7a7f1c61aac56c52366744aceb4"
    );
    assert_eq!(binding().name(), "cqnayesomb432fipnee");
    assert_eq!(
        binding().labels()["veoveo-template"],
        "4x5n4ab3oqcvzjiqhrzhpibftb4wc4eyenpmok5dv7swd7ed656q"
    );
    assert_eq!(
        PersistentHome::volume_name(binding().computer_id()).unwrap(),
        "veoveo-computer-3c4c49ee4f42442da76171a15466b992"
    );
    assert!(PersistentHome::volume_name(Uuid::nil()).is_err());
    assert!(Binding::new(Uuid::nil(), "a".repeat(63)).is_err());
}
#[test]
fn python_protobuf_shared_prefix_order_golden() {
    use crate::storage::object;
    use prost_types::{Value, value::Kind};
    let value = object([
        (
            "a",
            Value {
                kind: Some(Kind::NumberValue(1.0)),
            },
        ),
        (
            "b",
            Value {
                kind: Some(Kind::NumberValue(2.0)),
            },
        ),
        (
            "aa",
            Value {
                kind: Some(Kind::NumberValue(3.0)),
            },
        ),
    ]);
    assert_eq!(
        hex::encode(crate::canonical::encode(&value, ".google.protobuf.Struct")),
        "0a0f0a02616112091100000000000008400a0e0a0161120911000000000000f03f0a0e0a01621209110000000000000040"
    );
}
#[test]
fn descriptor_policy_json_camel_and_proto_names_preserve_fingerprint() {
    for camel in [false, true] {
        let value = if camel {
            serde_json::json!({"version":1,"filesystem":{"includeWorkdir":true,"readOnly":["/usr"],"readWrite":["/tmp"]},"landlock":{"compatibility":"hard_requirement"},"process":{"runAsUser":"10001","runAsGroup":"10001"}})
        } else {
            serde_json::json!({"version":1,"filesystem":{"include_workdir":true,"read_only":["/usr"],"read_write":["/tmp"]},"landlock":{"compatibility":"hard_requirement"},"process":{"run_as_user":"10001","run_as_group":"10001"}})
        };
        let parsed = parse_policy(&value).unwrap();
        let t = DevelopmentTemplate::new(
            format!("registry.example/dev@sha256:{}", "a".repeat(64)),
            16,
            65536,
            parsed,
            vec!["/bin/bash".into(), "-l".into()],
            None,
        )
        .unwrap();
        assert_eq!(t.fingerprint(), template(false).fingerprint());
    }
    assert!(parse_policy(&serde_json::json!({"unknownPolicyField":"SECRET"})).is_err());
    assert!(parse_policy(&serde_json::json!({"version":"invalid-SECRET"})).is_err());
}
#[test]
fn policy_admission_preserves_containment_and_storage_bounds() {
    for path in [
        "/",
        "/etc",
        "/etc/openshell-tls",
        "/run/openshell",
        "/tmp/../etc",
        "/usr/",
        "/etc/openshell-tls/ca.key",
    ] {
        let mut p = policy();
        p.filesystem.as_mut().unwrap().read_only.push(path.into());
        assert!(
            DevelopmentTemplate::new(
                format!("dev@sha256:{}", "a".repeat(64)),
                1,
                512,
                p,
                vec!["/bin/bash".into()],
                None
            )
            .is_err()
        );
    }
    for identity in ["", "0", "root", "-1", "01"] {
        let mut p = policy();
        p.process.as_mut().unwrap().run_as_user = identity.into();
        assert!(
            DevelopmentTemplate::new(
                format!("dev@sha256:{}", "a".repeat(64)),
                1,
                512,
                p,
                vec!["/bin/bash".into()],
                None
            )
            .is_err()
        );
    }
    for path in [
        "/etc/openshell-tls/ca-bundle.pem",
        "/etc/openshell-tls/openshell-ca.pem",
    ] {
        let mut p = policy();
        p.filesystem.as_mut().unwrap().read_only.push(path.into());
        assert!(
            DevelopmentTemplate::new(
                format!("dev@sha256:{}", "a".repeat(64)),
                1,
                512,
                p.clone(),
                vec!["/bin/bash".into()],
                None
            )
            .is_ok()
        );
        p.filesystem.as_mut().unwrap().read_write.push(path.into());
        assert!(
            DevelopmentTemplate::new(
                format!("dev@sha256:{}", "a".repeat(64)),
                1,
                512,
                p,
                vec!["/bin/bash".into()],
                None
            )
            .is_err()
        );
    }
    let p = template(true)
        .spec(Uuid::from_u128(1))
        .unwrap()
        .policy
        .unwrap();
    assert!(
        DevelopmentTemplate::new(
            format!("dev@sha256:{}", "a".repeat(64)),
            16,
            2048,
            p,
            PERSISTENT_COMMAND.iter().map(|s| s.to_string()).collect(),
            Some(PersistentHome::new(32768, 1024).unwrap())
        )
        .is_err()
    );
}
#[test]
fn terminal_and_execution_intents_are_finite() {
    for (c, r) in [(0, 0), (501, 20), (80, 201), (1, 1)] {
        assert!(TerminalSize::new(c, r).is_err());
    }
    for n in [0, 7201] {
        assert!(
            ExecIntent::new(vec!["/bin/true".into()], "/sandbox".into(), n, 1024, vec![]).is_err()
        );
    }
    assert!(
        ExecIntent::new(
            vec!["/bin/true".into()],
            "/sandbox/../etc".into(),
            1,
            1024,
            vec![]
        )
        .is_err()
    );
    assert!(
        ExecIntent::new(
            vec!["/bin/true".into()],
            "/sandbox".into(),
            1,
            1024,
            vec![0; MAX_CHUNK_BYTES + 1]
        )
        .is_err()
    );
    assert!(
        crate::terminal::forward_data(api::TcpForwardFrame {
            payload: Some(api::tcp_forward_frame::Payload::Data(vec![
                0;
                MAX_CHUNK_BYTES
                    + 1
            ]))
        })
        .is_err()
    );
    assert!(crate::terminal::forward_data(api::TcpForwardFrame { payload: None }).is_err());
    assert_eq!(
        crate::terminal::forward_data(api::TcpForwardFrame {
            payload: Some(api::tcp_forward_frame::Payload::Data(vec![0, 255, 13, 10]))
        })
        .unwrap(),
        [0, 255, 13, 10]
    );
}

struct FakeState {
    sandbox: Option<api::Sandbox>,
    expected_binding: Option<Binding>,
    created_spec: Option<api::SandboxSpec>,
    policy_fixture: Option<policy_tests::Fixture>,
    version: String,
    drivers: Vec<String>,
    denied: bool,
    watch: u8,
    exec: u8,
    gets: usize,
    get_delay: Duration,
    creates: usize,
    starts: usize,
    stops: usize,
    watches: usize,
    revokes: usize,
    input_bytes: Vec<u8>,
    ssh: terminal_tests::SshState,
    session_mode: u8,
    session_reply_gate: Option<terminal_tests::Gate>,
}
#[derive(Clone)]
struct Fake(Arc<Mutex<FakeState>>);
impl Fake {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(FakeState {
            sandbox: Some(sandbox(Phase::Stopped)),
            expected_binding: None,
            created_spec: None,
            policy_fixture: None,
            version: GATEWAY_VERSION.into(),
            drivers: vec!["docker".into()],
            denied: false,
            watch: 0,
            exec: 0,
            gets: 0,
            get_delay: Duration::ZERO,
            creates: 0,
            starts: 0,
            stops: 0,
            watches: 0,
            revokes: 0,
            input_bytes: vec![],
            ssh: Default::default(),
            session_mode: 0,
            session_reply_gate: None,
        })))
    }
    fn authorize(&self) -> std::result::Result<(), Status> {
        if self.0.lock().unwrap().denied {
            Err(Status::permission_denied("SECRET-PROVIDER-ERROR"))
        } else {
            Ok(())
        }
    }
}
fn boxed<T: Send + 'static>(events: Vec<std::result::Result<T, Status>>) -> BoxStream<T> {
    Box::pin(stream::iter(events))
}
fn exit(code: i32) -> api::ExecSandboxEvent {
    api::ExecSandboxEvent {
        payload: Some(api::exec_sandbox_event::Payload::Exit(
            api::ExecSandboxExit { exit_code: code },
        )),
    }
}
#[tonic::async_trait]
impl api::open_shell_server::OpenShell for Fake {
    async fn create_ssh_session(
        &self,
        request: Request<api::CreateSshSessionRequest>,
    ) -> std::result::Result<Response<api::CreateSshSessionResponse>, Status> {
        self.authorize()?;
        assert_eq!(request.into_inner().sandbox_id, "sandbox-1");
        let mode = self.0.lock().unwrap().session_mode;
        let response = api::CreateSshSessionResponse {
            sandbox_id: if mode == 1 { "wrong" } else { "sandbox-1" }.into(),
            token: "local-fixture-token".into(),
            host_key_fingerprint: if mode == 2 {
                "SHA256:WRONG".into()
            } else {
                terminal_tests::key()
                    .public_key()
                    .fingerprint(russh::keys::HashAlg::Sha256)
                    .to_string()
            },
            expires_at_ms: if mode == 3 {
                1
            } else {
                (SystemTime::now().duration_since(UNIX_EPOCH).unwrap() + Duration::from_secs(60))
                    .as_millis() as i64
            },
            gateway_host: "forbidden-redirect.invalid".into(),
            gateway_port: 1,
            ..Default::default()
        };
        // The token exists at the provider before its response reaches the client.
        let gate = self.0.lock().unwrap().session_reply_gate.clone();
        if let Some(gate) = gate {
            gate.pause().await;
        }
        Ok(Response::new(response))
    }
    async fn revoke_ssh_session(
        &self,
        request: Request<api::RevokeSshSessionRequest>,
    ) -> std::result::Result<Response<api::RevokeSshSessionResponse>, Status> {
        assert_eq!(request.into_inner().token, "local-fixture-token");
        self.0.lock().unwrap().revokes += 1;
        if self.0.lock().unwrap().session_mode == 4 {
            return Err(Status::unavailable("SECRET"));
        }
        Ok(Response::new(api::RevokeSshSessionResponse {
            revoked: true,
        }))
    }
    async fn forward_tcp(
        &self,
        request: Request<tonic::Streaming<api::TcpForwardFrame>>,
    ) -> std::result::Result<Response<BoxStream<api::TcpForwardFrame>>, Status> {
        terminal_tests::forward(self.clone(), request).await
    }
    async fn get_gateway_info(
        &self,
        _: Request<api::GetGatewayInfoRequest>,
    ) -> std::result::Result<Response<api::GetGatewayInfoResponse>, Status> {
        self.authorize()?;
        let state = self.0.lock().unwrap();
        Ok(Response::new(api::GetGatewayInfoResponse {
            gateway_version: state.version.clone(),
            compute_drivers: state
                .drivers
                .iter()
                .map(|name| api::ComputeDriverInfo {
                    name: name.clone(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }))
    }
    async fn get_sandbox(
        &self,
        request: Request<api::GetSandboxRequest>,
    ) -> std::result::Result<Response<api::SandboxResponse>, Status> {
        self.authorize()?;
        let delay = self.0.lock().unwrap().get_delay;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let request = request.into_inner();
        assert_eq!(request.workspace, "computers");
        let mut state = self.0.lock().unwrap();
        if let Some(fixture) = &mut state.policy_fixture {
            return fixture.get(request);
        }
        assert_eq!(
            request.name,
            state
                .expected_binding
                .clone()
                .unwrap_or_else(binding)
                .name()
        );
        state.gets += 1;
        Ok(Response::new(api::SandboxResponse {
            sandbox: Some(
                state
                    .sandbox
                    .clone()
                    .ok_or_else(|| Status::not_found("absent"))?,
            ),
        }))
    }
    async fn create_sandbox(
        &self,
        request: Request<api::CreateSandboxRequest>,
    ) -> std::result::Result<Response<api::SandboxResponse>, Status> {
        self.authorize()?;
        let request = request.into_inner();
        assert!(request.spec.as_ref().unwrap().environment.is_empty());
        let mut state = self.0.lock().unwrap();
        let expected = state.expected_binding.clone().unwrap_or_else(binding);
        assert_eq!(request.name, expected.name());
        assert_eq!(request.labels, expected.labels());
        state.creates += 1;
        if state.sandbox.is_some() {
            return Err(Status::already_exists("same"));
        }
        state.sandbox = Some(sandbox(Phase::Provisioning));
        let created = state.sandbox.as_mut().unwrap();
        created.metadata.as_mut().unwrap().name = expected.name();
        created.metadata.as_mut().unwrap().labels = expected.labels();
        created.spec = request.spec.clone();
        state.created_spec = request.spec;
        Ok(Response::new(api::SandboxResponse {
            sandbox: state.sandbox.clone(),
        }))
    }
    async fn start_sandbox(
        &self,
        request: Request<api::StartSandboxRequest>,
    ) -> std::result::Result<Response<api::SandboxResponse>, Status> {
        self.authorize()?;
        let request = request.into_inner();
        assert_eq!(request.workspace, "computers");
        let mut state = self.0.lock().unwrap();
        assert_eq!(
            request.name,
            state
                .expected_binding
                .clone()
                .unwrap_or_else(binding)
                .name()
        );
        state.starts += 1;
        state
            .sandbox
            .as_mut()
            .unwrap()
            .status
            .as_mut()
            .unwrap()
            .phase = Phase::Starting as i32;
        Ok(Response::new(api::SandboxResponse {
            sandbox: state.sandbox.clone(),
        }))
    }
    async fn stop_sandbox(
        &self,
        request: Request<api::StopSandboxRequest>,
    ) -> std::result::Result<Response<api::SandboxResponse>, Status> {
        self.authorize()?;
        let request = request.into_inner();
        assert_eq!(request.workspace, "computers");
        let mut state = self.0.lock().unwrap();
        assert_eq!(
            request.name,
            state
                .expected_binding
                .clone()
                .unwrap_or_else(binding)
                .name()
        );
        state.stops += 1;
        state
            .sandbox
            .as_mut()
            .unwrap()
            .status
            .as_mut()
            .unwrap()
            .phase = Phase::Stopping as i32;
        Ok(Response::new(api::SandboxResponse {
            sandbox: state.sandbox.clone(),
        }))
    }
    async fn watch_sandbox(
        &self,
        request: Request<api::WatchSandboxRequest>,
    ) -> std::result::Result<Response<BoxStream<api::SandboxStreamEvent>>, Status> {
        self.authorize()?;
        let request = request.into_inner();
        let mut state = self.0.lock().unwrap();
        if let Some(fixture) = &mut state.policy_fixture {
            return fixture.watch(request);
        }
        assert_eq!(request.id, "sandbox-1");
        assert!(request.follow_status);
        assert!(!request.follow_logs);
        state.watches += 1;
        let payload = match state.watch {
            1 => api::sandbox_stream_event::Payload::Warning(api::SandboxStreamWarning {
                message: "SECRET".into(),
            }),
            2 => {
                return Ok(Response::new(boxed(vec![Err(Status::unavailable(
                    "SECRET",
                ))])));
            }
            3 => return Ok(Response::new(boxed(vec![]))),
            9 => return Ok(Response::new(Box::pin(stream::pending()))),
            4 => {
                let mut s = state.sandbox.clone().unwrap();
                s.status.as_mut().unwrap().phase = Phase::Ready as i32;
                s.metadata.as_mut().unwrap().id = "replacement".into();
                api::sandbox_stream_event::Payload::Sandbox(s)
            }
            5 => {
                let mut s = state.sandbox.clone().unwrap();
                s.status.as_mut().unwrap().phase = Phase::Ready as i32;
                s.metadata
                    .as_mut()
                    .unwrap()
                    .labels
                    .insert("veoveo-instance".into(), Uuid::from_u128(11).to_string());
                api::sandbox_stream_event::Payload::Sandbox(s)
            }
            _ => {
                let target = if state
                    .sandbox
                    .as_ref()
                    .unwrap()
                    .status
                    .as_ref()
                    .unwrap()
                    .phase
                    == Phase::Stopping as i32
                {
                    Phase::Stopped
                } else {
                    Phase::Ready
                };
                let s = state.sandbox.as_mut().unwrap();
                let status = s.status.as_mut().unwrap();
                if target == Phase::Ready && status.phase == Phase::Starting as i32 {
                    status.main_process_instance_id = Uuid::now_v7().to_string();
                }
                status.phase = target as i32;
                api::sandbox_stream_event::Payload::Sandbox(s.clone())
            }
        };
        Ok(Response::new(boxed(vec![Ok(api::SandboxStreamEvent {
            payload: Some(payload),
        })])))
    }
    async fn get_sandbox_config(
        &self,
        request: Request<policy::GetSandboxConfigRequest>,
    ) -> std::result::Result<Response<policy::GetSandboxConfigResponse>, Status> {
        self.authorize()?;
        self.0
            .lock()
            .unwrap()
            .policy_fixture
            .as_mut()
            .ok_or_else(|| Status::unimplemented("fixture"))?
            .config(request.into_inner())
    }
    async fn get_sandbox_policy_status(
        &self,
        request: Request<api::GetSandboxPolicyStatusRequest>,
    ) -> std::result::Result<Response<api::GetSandboxPolicyStatusResponse>, Status> {
        self.authorize()?;
        self.0
            .lock()
            .unwrap()
            .policy_fixture
            .as_mut()
            .ok_or_else(|| Status::unimplemented("fixture"))?
            .loaded(request.into_inner())
    }
    async fn update_config(
        &self,
        request: Request<api::UpdateConfigRequest>,
    ) -> std::result::Result<Response<api::UpdateConfigResponse>, Status> {
        self.authorize()?;
        self.0
            .lock()
            .unwrap()
            .policy_fixture
            .as_mut()
            .ok_or_else(|| Status::unimplemented("fixture"))?
            .update(request.into_inner())
    }
    async fn exec_sandbox(
        &self,
        request: Request<api::ExecSandboxRequest>,
    ) -> std::result::Result<Response<BoxStream<api::ExecSandboxEvent>>, Status> {
        self.authorize()?;
        let r = request.into_inner();
        assert_eq!(r.sandbox_id, "sandbox-1");
        assert!(!r.tty);
        assert!(r.environment.is_empty());
        let mut state = self.0.lock().unwrap();
        let events = match state.exec {
            1 => vec![Ok(exit(124))],
            2 => vec![],
            3 => vec![Err(Status::unknown("SECRET"))],
            4 => vec![Ok(exit(0)), Ok(exit(0))],
            5 => vec![Ok(api::ExecSandboxEvent {
                payload: Some(api::exec_sandbox_event::Payload::Stdout(
                    api::ExecSandboxStdout {
                        data: vec![0; MAX_CHUNK_BYTES + 1],
                    },
                )),
            })],
            6 => {
                state
                    .sandbox
                    .as_mut()
                    .unwrap()
                    .status
                    .as_mut()
                    .unwrap()
                    .main_process_instance_id = "main-2".into();
                vec![Ok(exit(0))]
            }
            _ => vec![
                Ok(api::ExecSandboxEvent {
                    payload: Some(api::exec_sandbox_event::Payload::Stdout(
                        api::ExecSandboxStdout {
                            data: vec![0, 255, 10],
                        },
                    )),
                }),
                Ok(exit(7)),
            ],
        };
        Ok(Response::new(boxed(events)))
    }
    async fn exec_sandbox_interactive(
        &self,
        request: Request<tonic::Streaming<api::ExecSandboxInput>>,
    ) -> std::result::Result<Response<BoxStream<api::ExecSandboxEvent>>, Status> {
        self.authorize()?;
        let fake = self.clone();
        let mut input = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let start = input.message().await.unwrap().unwrap();
            assert!(matches!(
                start.payload,
                Some(api::exec_sandbox_input::Payload::Start(_))
            ));
            let expected = fake.0.lock().unwrap().exec as usize;
            while fake.0.lock().unwrap().input_bytes.len() < expected {
                let Some(event) = input.message().await.unwrap() else {
                    return;
                };
                if let Some(api::exec_sandbox_input::Payload::Stdin(bytes)) = event.payload {
                    assert!(bytes.len() <= MAX_CHUNK_BYTES);
                    fake.0.lock().unwrap().input_bytes.extend(bytes);
                } else {
                    panic!("unexpected input frame");
                }
            }
            // Request half must remain open while the native exit is emitted.
            let _ = tx.send(Ok(exit(0))).await;
        });
        Ok(Response::new(Box::pin(stream::unfold(
            rx,
            |mut rx| async move { rx.recv().await.map(|v| (v, rx)) },
        ))))
    }
}

struct TlsFiles {
    dir: PathBuf,
    ca: String,
    server_cert: String,
    server_key: String,
}
impl TlsFiles {
    fn new() -> Self {
        use rcgen::{
            BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
            KeyUsagePurpose,
        };
        let mut params = CertificateParams::new(vec![]).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let ca_key = KeyPair::generate().unwrap();
        let ca = params.self_signed(&ca_key).unwrap().pem();
        let issuer = Issuer::new(params, ca_key);
        let mut server =
            CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
        server.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let server_key = KeyPair::generate().unwrap();
        let server_cert = server.signed_by(&server_key, &issuer).unwrap().pem();
        let mut client = CertificateParams::new(vec![]).unwrap();
        client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        let client_key = KeyPair::generate().unwrap();
        let client_cert = client.signed_by(&client_key, &issuer).unwrap().pem();
        let dir = std::env::temp_dir().join(format!("veoveo-computers-test-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("ca.pem"), &ca).unwrap();
        std::fs::write(dir.join("cert.pem"), client_cert).unwrap();
        std::fs::write(dir.join("key.pem"), client_key.serialize_pem()).unwrap();
        Self {
            dir,
            ca,
            server_cert,
            server_key: server_key.serialize_pem(),
        }
    }
    fn config(&self, endpoint: String) -> GatewayConfig {
        GatewayConfig::new(
            Uuid::from_u128(100),
            endpoint,
            "computers".into(),
            self.dir.join("ca.pem"),
            self.dir.join("cert.pem"),
            self.dir.join("key.pem"),
        )
        .unwrap()
    }
}
impl Drop for TlsFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
struct Running {
    fake: Fake,
    runtime: OpenShellRuntime,
    tls: TlsFiles,
    endpoint: String,
    task: tokio::task::JoinHandle<()>,
}
impl Running {
    async fn start() -> Self {
        // Match the gateway/BFF entrypoints when a combined workspace test
        // unifies both Rustls provider features through other dependencies.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls = TlsFiles::new();
        let fake = Fake::new();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        let incoming = stream::unfold(listener, |listener| async move {
            let result = listener.accept().await.map(|(s, _)| s);
            Some((result, listener))
        });
        let server = Server::builder()
            .tls_config(
                ServerTlsConfig::new()
                    .identity(Identity::from_pem(&tls.server_cert, &tls.server_key))
                    .client_ca_root(Certificate::from_pem(&tls.ca)),
            )
            .unwrap()
            .add_service(api::open_shell_server::OpenShellServer::new(fake.clone()));
        let task = tokio::spawn(async move {
            server.serve_with_incoming(incoming).await.unwrap();
        });
        let runtime = OpenShellRuntime::connect(tls.config(endpoint.clone()))
            .await
            .unwrap();
        Self {
            fake,
            runtime,
            tls,
            endpoint,
            task,
        }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn generated_mtls_lifecycle_and_watch_never_poll_completion() {
    let running = Running::start().await;
    let runtime = &running.runtime;
    let b = binding();
    running.fake.0.lock().unwrap().sandbox = None;
    let create =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), b.clone()).unwrap();
    let created = runtime.create(&b, &template(false)).await.unwrap();
    assert!(created.phase == Phase::Provisioning);
    let calls = running.fake.0.lock().unwrap().gets;
    let ready = runtime
        .wait_for_lifecycle(&create, &created, Duration::from_secs(10))
        .await
        .unwrap();
    assert!(ready.phase == Phase::Ready);
    assert_eq!(running.fake.0.lock().unwrap().gets, calls);
    runtime.create(&b, &template(false)).await.unwrap();
    assert_eq!(running.fake.0.lock().unwrap().creates, 1);
    let stop =
        LifecycleCheckpoint::stop(Uuid::from_u128(100), Uuid::now_v7(), b.clone(), &ready).unwrap();
    let stopping = runtime.stop(&b).await.unwrap();
    let stopped = runtime
        .wait_for_lifecycle(&stop, &stopping, Duration::from_secs(10))
        .await
        .unwrap();
    assert!(stopped.phase == Phase::Stopped);
    let start =
        LifecycleCheckpoint::start(Uuid::from_u128(100), Uuid::now_v7(), b.clone(), &stopped)
            .unwrap();
    let starting = runtime.start(&b).await.unwrap();
    runtime
        .wait_for_lifecycle(&start, &starting, Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(running.fake.0.lock().unwrap().starts, 1);
    assert_eq!(running.fake.0.lock().unwrap().stops, 1);
}
#[tokio::test]
async fn gateway_admission_auth_and_identity_fail_closed() {
    let running = Running::start().await;
    {
        let mut s = running.fake.0.lock().unwrap();
        s.version = "0.0.117-dev.6+g32efe0b-extra".into();
    }
    assert!(matches!(
        running.runtime.ready().await,
        Err(RuntimeFailure::VersionMismatch)
    ));
    {
        let mut s = running.fake.0.lock().unwrap();
        s.version = GATEWAY_VERSION.into();
        s.drivers = vec!["docker".into(), "kubernetes".into()];
    }
    assert!(matches!(
        running.runtime.ready().await,
        Err(RuntimeFailure::VersionMismatch)
    ));
    running.fake.0.lock().unwrap().denied = true;
    let error = running.runtime.get(&binding()).await.err().unwrap();
    assert_eq!(error, RuntimeFailure::Unavailable);
    assert!(!format!("{error:?} {error}").contains("SECRET"));
    running.fake.0.lock().unwrap().denied = false;
    running
        .fake
        .0
        .lock()
        .unwrap()
        .sandbox
        .as_mut()
        .unwrap()
        .metadata
        .as_mut()
        .unwrap()
        .labels
        .insert("veoveo-computer".into(), Uuid::new_v4().to_string());
    assert!(matches!(
        running.runtime.get(&binding()).await,
        Err(RuntimeFailure::BindingMismatch)
    ));
    running.fake.0.lock().unwrap().drivers = vec!["docker".into()];
    running.runtime.ready().await.unwrap();
    // The actual local TLS server rejects a different installation CA/client.
    let stranger = TlsFiles::new();
    assert!(matches!(
        OpenShellRuntime::connect(stranger.config(running.endpoint.clone())).await,
        Err(RuntimeFailure::Unavailable)
    ));
    // It also rejects a client offering no certificate, even with trusted CA.
    let channel = tonic::transport::Endpoint::from_shared(format!("https://{}", running.endpoint))
        .unwrap()
        .tls_config(
            tonic::transport::ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(&running.tls.ca)),
        )
        .unwrap()
        .connect()
        .await;
    if let Ok(channel) = channel {
        assert!(matches!(
            OpenShellRuntime::from_channel(channel, "computers".into(), Uuid::from_u128(100))
                .ready()
                .await,
            Err(RuntimeFailure::Unavailable)
        ));
    }
}
#[tokio::test]
async fn watch_warning_transport_end_and_identity_replacement_are_failures() {
    let running = Running::start().await;
    let before = running.runtime.get(&binding()).await.unwrap().unwrap();
    let checkpoint =
        LifecycleCheckpoint::start(Uuid::from_u128(100), Uuid::now_v7(), binding(), &before)
            .unwrap();
    let current = running.runtime.start(&binding()).await.unwrap();
    let calls = running.fake.0.lock().unwrap().gets;
    for mode in 1..=4 {
        running.fake.0.lock().unwrap().watch = mode;
        assert!(
            running
                .runtime
                .wait_for_lifecycle(&checkpoint, &current, Duration::from_secs(10))
                .await
                .is_err()
        );
    }
    assert_eq!(running.fake.0.lock().unwrap().gets, calls);
    assert_eq!(running.fake.0.lock().unwrap().watches, 4);
}
#[tokio::test]
async fn native_exec_exit_bytes_and_unknown_outcomes() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let intent = ExecIntent::new(
        vec!["/usr/bin/true".into()],
        "/sandbox".into(),
        5,
        1024,
        vec![],
    )
    .unwrap();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let capture = bytes.clone();
    let result = running
        .runtime
        .execute(&binding(), &intent, move |chunk| {
            capture.lock().unwrap().extend(chunk.data);
            async { Ok(()) }
        })
        .await
        .unwrap();
    assert_eq!(
        result,
        ExecResult {
            exit_code: 7,
            stdout_bytes: 3,
            stderr_bytes: 0
        }
    );
    assert_eq!(*bytes.lock().unwrap(), [0, 255, 10]);
    for mode in 1..=6 {
        {
            let mut s = running.fake.0.lock().unwrap();
            s.exec = mode;
            s.sandbox = Some(sandbox(Phase::Ready));
        }
        assert!(matches!(
            running
                .runtime
                .execute(&binding(), &intent, |_| async { Ok(()) })
                .await,
            Err(RuntimeFailure::ExecutionUnknown)
        ));
    }
    {
        let mut s = running.fake.0.lock().unwrap();
        s.exec = 0;
        s.sandbox = Some(sandbox(Phase::Ready));
    }
    assert!(matches!(
        running
            .runtime
            .execute(&binding(), &intent, |_| async {
                Err(RuntimeFailure::InvalidConfiguration)
            })
            .await,
        Err(RuntimeFailure::ExecutionUnknown)
    ));
}
#[tokio::test]
async fn streamed_exec_exact_delivery_and_underflow_are_distinct() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    running.fake.0.lock().unwrap().exec = 3;
    let intent =
        ExecIntent::new(vec!["/bin/cat".into()], "/sandbox".into(), 3, 1024, vec![]).unwrap();
    let mut source = std::io::Cursor::new(vec![0, 255, 1]);
    let input = ExecInput::new(&mut source, 3).unwrap();
    assert_eq!(
        running
            .runtime
            .execute_with_input(&binding(), &intent, input, |_| async { Ok(()) })
            .await
            .unwrap()
            .exit_code,
        0
    );
    assert_eq!(running.fake.0.lock().unwrap().input_bytes, [0, 255, 1]);
    running.fake.0.lock().unwrap().input_bytes.clear();
    let mut source = std::io::Cursor::new(vec![0]);
    let input = ExecInput::new(&mut source, 3).unwrap();
    assert!(matches!(
        running
            .runtime
            .execute_with_input(&binding(), &intent, input, |_| async { Ok(()) })
            .await,
        Err(RuntimeFailure::ExecutionUnknown)
    ));
    running.fake.0.lock().unwrap().input_bytes.clear();
    let mut source = std::io::Cursor::new(vec![0, 1, 2, 3]);
    let input = ExecInput::new(&mut source, 3).unwrap();
    assert!(matches!(
        running
            .runtime
            .execute_with_input(&binding(), &intent, input, |_| async { Ok(()) })
            .await,
        Err(RuntimeFailure::ExecutionUnknown)
    ));
}
