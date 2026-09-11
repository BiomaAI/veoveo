use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computers_runtime::{GatewayConfig, OpenShellRuntime};
mod guest_authority;

pub struct ComputeHost {
    pub socket: PathBuf,
    pub output: PathBuf,
    pub namespace: String,
    pub gateway_ip: std::net::Ipv4Addr,
}

/// This fixture owns every process, network and container it can remove.
pub struct Provider {
    pub dir: PathBuf,
    pub runtime: OpenShellRuntime,
    pub image: String,
    #[allow(dead_code)] // Only installation-policy scenarios use the raw fixture endpoint.
    pub endpoint: String,
    cleanup: Cleanup,
}
struct Cleanup {
    child: Option<Child>,
    dir: PathBuf,
    namespace: String,
    network: String,
    socket: PathBuf,
}
impl Cleanup {
    fn docker(&self) -> Command {
        let mut command = Command::new("docker");
        command
            .arg("--host")
            .arg(format!("unix://{}", self.socket.display()));
        command
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        let filter = format!("label=openshell.ai/sandbox-namespace={}", self.namespace);
        if let Ok(output) = self
            .docker()
            .args(["ps", "--all", "--quiet", "--filter", &filter])
            .output()
        {
            for id in String::from_utf8_lossy(&output.stdout).split_whitespace() {
                if (12..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_hexdigit()) {
                    let _ = self.docker().args(["rm", "--force", id]).output();
                }
            }
        }
        let _ = self
            .docker()
            .args(["network", "rm", &self.network])
            .output();
        // Preserve non-secret diagnostics at the requested fixture output location.
        // Keys are always removed, including when startup or an assertion fails.
        for name in [
            "client-key.pem",
            "guest-key.pem",
            "server-key.pem",
            "jwt-key.pem",
        ] {
            let _ = fs::remove_file(self.dir.join(name));
        }
        for name in ["gateway.sqlite", "gateway.sqlite-wal", "gateway.sqlite-shm"] {
            let _ = fs::remove_file(self.dir.join(name));
        }
        for name in ["state", "data", "config"] {
            let _ = fs::remove_dir_all(self.dir.join(name));
        }
    }
}

fn required_path(name: &str) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is required")));
    assert!(path.is_absolute(), "{name} must be absolute");
    path
}
fn quoted(path: &std::path::Path) -> String {
    serde_json::to_string(path.to_str().unwrap()).unwrap()
}

impl Provider {
    pub async fn start() -> Self {
        Self::start_with_session_ttl(3600).await.0
    }

    pub async fn start_with_session_ttl(ssh_session_ttl_secs: u64) -> (Self, String) {
        Self::start_on(ssh_session_ttl_secs, None, "warn").await
    }
    #[allow(dead_code)] // The execution scenario must observe command-preview logging.
    pub async fn start_with_execution_logging() -> Self {
        Self::start_on(3600, None, "info").await.0
    }
    #[allow(dead_code)] // Used by the shared storage/worker fixture.
    pub async fn start_on_compute_host(host: ComputeHost) -> Self {
        Self::start_on(3600, Some(host), "warn").await.0
    }
    async fn start_on(
        ssh_session_ttl_secs: u64,
        host: Option<ComputeHost>,
        log_level: &str,
    ) -> (Self, String) {
        let gateway = required_path("VEOVEO_COMPUTERS_NATIVE_GATEWAY");
        let supervisor = required_path("VEOVEO_COMPUTERS_NATIVE_SUPERVISOR");
        let output = host
            .as_ref()
            .map(|host| host.output.clone())
            .unwrap_or_else(|| required_path("VEOVEO_COMPUTERS_NATIVE_OUTPUT"));
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE")
            .expect("digest-pinned native image is required");
        assert!(image.contains("@sha256:"));
        let suffix = Uuid::now_v7().simple().to_string();
        let dir = output.join(&suffix);
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let namespace = host
            .as_ref()
            .map(|host| host.namespace.clone())
            .unwrap_or_else(|| format!("veoveo-native-{suffix}"));
        let socket = host
            .as_ref()
            .map(|host| host.socket.clone())
            .unwrap_or_else(|| PathBuf::from("/var/run/docker.sock"));
        let gateway_ip = host
            .as_ref()
            .map(|host| host.gateway_ip.to_string())
            .unwrap_or_default();
        let network = namespace.clone();
        let mut cleanup = Cleanup {
            child: None,
            dir: dir.clone(),
            namespace: namespace.clone(),
            network: network.clone(),
            socket: socket.clone(),
        };
        certificates(&dir);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = format!(
            r#"[openshell]
version = 1
[openshell.gateway]
bind_address = "127.0.0.1:{port}"
compute_drivers = ["docker"]
log_level = "{log_level}"
ssh_session_ttl_secs = {ssh_session_ttl_secs}
[openshell.gateway.mtls_auth]
enabled = true
user_common_names = ["veoveo-computers-worker"]
[openshell.gateway.gateway_jwt]
signing_key_path = {jwt_key}
public_key_path = {jwt_public}
kid_path = {jwt_kid}
gateway_id = "{namespace}"
ttl_secs = 3600
[openshell.drivers.docker]
socket_path = {socket}
host_gateway_ip = "{gateway_ip}"
default_image = {image}
image_pull_policy = "Never"
sandbox_namespace = "{namespace}"
network_name = "{network}"
grpc_endpoint = "https://host.openshell.internal:{port}"
supervisor_bin = {supervisor}
guest_tls_ca = {ca}
guest_tls_cert = {cert}
guest_tls_key = {key}
sandbox_pids_limit = 256
enable_bind_mounts = false
"#,
            image = serde_json::to_string(&image).unwrap(),
            socket = quoted(&socket),
            supervisor = quoted(&supervisor),
            ca = quoted(&dir.join("ca.pem")),
            cert = quoted(&dir.join("guest.pem")),
            key = quoted(&dir.join("guest-key.pem")),
            jwt_key = quoted(&dir.join("jwt-key.pem")),
            jwt_public = quoted(&dir.join("jwt-public.pem")),
            jwt_kid = quoted(&dir.join("jwt-kid")),
        );
        fs::write(dir.join("gateway.toml"), config).unwrap();
        let log = fs::File::create(dir.join("gateway.log")).unwrap();
        let mut command = Command::new(gateway);
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("XDG_STATE_HOME", dir.join("state"))
            .env("XDG_DATA_HOME", dir.join("data"))
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .arg("--config")
            .arg(dir.join("gateway.toml"))
            .arg("--db-url")
            .arg(format!(
                "sqlite://{}?mode=rwc",
                dir.join("gateway.sqlite").display()
            ))
            .arg("--tls-cert")
            .arg(dir.join("server.pem"))
            .arg("--tls-key")
            .arg(dir.join("server-key.pem"))
            .arg("--tls-client-ca")
            .arg(dir.join("ca.pem"))
            .args([
                "--enable-mtls-auth",
                "true",
                "--enable-loopback-service-http",
                "false",
            ])
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        drop(listener);
        cleanup.child = Some(command.spawn().expect("start native provider"));
        let endpoint = format!("localhost:{port}");
        let runtime = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if let Some(exit) = cleanup.child.as_mut().unwrap().try_wait().unwrap() {
                    panic!("provider exited {exit}; diagnostics at {}", dir.display());
                }
                let config = GatewayConfig::new(
                    Uuid::from_u128(100),
                    endpoint.clone(),
                    "default".into(),
                    dir.join("ca.pem"),
                    dir.join("client.pem"),
                    dir.join("client-key.pem"),
                )
                .unwrap();
                match OpenShellRuntime::connect(config).await {
                    Ok(runtime) => break runtime,
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "provider readiness timed out; diagnostics at {}",
                dir.display()
            )
        });
        eprintln!("Native provider diagnostics: {}", dir.display());
        guest_authority::assert_denied(&dir, &endpoint).await;
        let provider = Self {
            dir,
            runtime,
            image,
            endpoint: endpoint.clone(),
            cleanup,
        };
        (provider, format!("https://{endpoint}"))
    }

    pub fn assert_running(&mut self) {
        assert!(
            self.cleanup
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none()
        );
    }
}

fn certificates(dir: &std::path::Path) {
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose,
    };
    let jwt_key = KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
    let private_path = dir.join("jwt-key.pem");
    fs::write(&private_path, jwt_key.serialize_pem()).unwrap();
    fs::set_permissions(private_path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(dir.join("jwt-public.pem"), jwt_key.public_key_pem()).unwrap();
    fs::write(dir.join("jwt-kid"), Uuid::now_v7().to_string()).unwrap();
    let mut ca_params = CertificateParams::new(vec![]).unwrap();
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let ca_key = KeyPair::generate().unwrap();
    fs::write(
        dir.join("ca.pem"),
        ca_params.self_signed(&ca_key).unwrap().pem(),
    )
    .unwrap();
    let issuer = Issuer::new(ca_params, ca_key);
    for (name, names, usage) in [
        (
            "server",
            vec![
                "localhost".into(),
                "127.0.0.1".into(),
                "host.openshell.internal".into(),
            ],
            ExtendedKeyUsagePurpose::ServerAuth,
        ),
        ("client", vec![], ExtendedKeyUsagePurpose::ClientAuth),
        ("guest", vec![], ExtendedKeyUsagePurpose::ClientAuth),
    ] {
        let mut params = CertificateParams::new(names).unwrap();
        params.extended_key_usages = vec![usage];
        params.distinguished_name = rcgen::DistinguishedName::new();
        params.distinguished_name.push(
            rcgen::DnType::CommonName,
            match name {
                "client" => "veoveo-computers-worker",
                "guest" => "veoveo-computer-supervisor",
                _ => "veoveo-computers-provider",
            },
        );
        let key = KeyPair::generate().unwrap();
        fs::write(
            dir.join(format!("{name}.pem")),
            params.signed_by(&key, &issuer).unwrap().pem(),
        )
        .unwrap();
        let path = dir.join(format!("{name}-key.pem"));
        fs::write(&path, key.serialize_pem()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
