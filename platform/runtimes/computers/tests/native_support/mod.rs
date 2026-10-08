use std::{
    fs,
    net::TcpListener,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Child, Command},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computers_runtime::{GatewayConfig, OpenShellRuntime};
mod controller;
mod docker_daemon;
mod guest_authority;
pub mod profile;
pub use profile::preflight;

fn provider_config(
    dir: &std::path::Path,
    host: &ComputeHost,
    port: u16,
    image: &str,
    supervisor_image: &str,
    sandbox: &std::path::Path,
    ssh_session_ttl_secs: u64,
    log_level: &str,
) -> String {
    let gateway_ip = host.gateway_ip;
    let namespace = &host.namespace;
    let socket = &host.socket;
    format!(
        r#"[openshell]
version = 2
[openshell.gateway]
bind_address = "{gateway_ip}:{port}"
compute_driver = "docker"
log_level = "{log_level}"
ssh_session_ttl_secs = {ssh_session_ttl_secs}
guest_tls_ca = {ca}
guest_tls_cert = {cert}
guest_tls_key = {key}
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
default_image = {image}
image_pull_policy = "never"
supervisor_image = {supervisor_image}
allow_driver_config = true
sandbox_label = "{namespace}"
grpc_endpoint = "https://{gateway_ip}:{port}"
supervisor_bin = {sandbox}
sandbox_pids_limit = 256
enable_bind_mounts = false
"#,
        image = serde_json::to_string(&image).unwrap(),
        socket = quoted(&socket),
        sandbox = quoted(&sandbox),
        supervisor_image = serde_json::to_string(&supervisor_image).unwrap(),
        ca = quoted(&dir.join("ca.pem")),
        cert = quoted(&dir.join("guest.pem")),
        key = quoted(&dir.join("guest-key.pem")),
        jwt_key = quoted(&dir.join("jwt-key.pem")),
        jwt_public = quoted(&dir.join("jwt-public.pem")),
        jwt_kid = quoted(&dir.join("jwt-kid")),
    )
}

#[cfg(test)]
mod generated_config_tests {
    use super::*;
    #[test]
    fn generated_native_provider_config_matches_packaged_loader_input() {
        let host = ComputeHost {
            socket: "/run/veoveo-native/docker.sock".into(),
            output: "/run/veoveo-native/output".into(),
            namespace: "private-native".into(),
            gateway_ip: "172.30.0.1".parse().unwrap(),
        };
        let generated = provider_config(
            std::path::Path::new("/run/veoveo-native/trust"),
            &host,
            18805,
            &format!("registry.internal:5000/computer@sha256:{}", "a".repeat(64)),
            &format!("registry.internal:5000/provider@sha256:{}", "c".repeat(64)),
            std::path::Path::new("/usr/local/bin/openshell-sandbox"),
            3600,
            "warn",
        );
        if let Some(directory) = std::env::var_os("VEOVEO_PROVIDER_CONFIG_EXPORT") {
            fs::write(PathBuf::from(directory).join("native.toml"), &generated).unwrap();
        } else {
            assert_eq!(
                generated,
                include_str!("../../provider-patches/generated/native.toml")
            );
        }
    }
}

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
    launch: controller::Launch,
}
struct Cleanup {
    child: Option<Child>,
    dir: PathBuf,
    namespace: String,
    network: String,
    socket: PathBuf,
    owned_daemon: Option<docker_daemon::DockerDaemon>,
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
            if controller::stop(child).is_err() {
                let _ = controller::stop_child_only(child);
                eprintln!(
                    "owned controller cleanup did not complete within its process-group budget"
                );
            }
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
        drop(self.owned_daemon.take());
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
    pub fn process_id(&self) -> Option<u32> {
        self.cleanup.child.as_ref().map(Child::id)
    }

    pub async fn start(test_name: &'static str) -> Self {
        Self::start_with_session_ttl(3600, test_name).await.0
    }
    pub async fn start_with_session_ttl(
        ssh_session_ttl_secs: u64,
        test_name: &'static str,
    ) -> (Self, String) {
        Self::start_isolated(ssh_session_ttl_secs, "warn", test_name).await
    }
    #[allow(dead_code)]
    pub async fn start_with_execution_logging(test_name: &'static str) -> Self {
        Self::start_isolated(3600, "info", test_name).await.0
    }
    async fn start_isolated(ttl: u64, log_level: &str, test_name: &'static str) -> (Self, String) {
        let gateway_ip = preflight().await;
        let output = required_path("VEOVEO_COMPUTERS_NATIVE_OUTPUT");
        let diagnostics = output.join(format!("private-daemon-{}", Uuid::now_v7().simple()));
        fs::create_dir_all(&diagnostics).unwrap();
        fs::set_permissions(&diagnostics, fs::Permissions::from_mode(0o700)).unwrap();
        let plugins = diagnostics.join("plugins");
        fs::create_dir(&plugins).unwrap();
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE").expect("pinned native image");
        let daemon = docker_daemon::DockerDaemon::start(
            &diagnostics,
            &plugins,
            &image,
            docker_daemon::Profile::NativeProvider { test_name },
            &[],
        )
        .await;
        let host = ComputeHost {
            socket: daemon.socket.clone(),
            output: diagnostics,
            namespace: format!("veoveo-native-{}", Uuid::now_v7().simple()),
            gateway_ip,
        };
        let (mut provider, endpoint) = Self::start_on(ttl, Some(host), log_level).await;
        provider.cleanup.owned_daemon = Some(daemon);
        (provider, endpoint)
    }
    #[allow(dead_code)] // Used by the owning controller recovery scenario.
    pub fn namespace(&self) -> &str {
        &self.cleanup.namespace
    }

    pub fn docker_socket(&self) -> PathBuf {
        self.cleanup.socket.clone()
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
        let admitted_gateway_ip = preflight().await;
        let host = host.expect("OpenShell 0.1.2 native provider requires an isolated ComputeHost; outer-host sidecar networking is unsupported");
        assert!(
            host.socket.is_absolute()
                && host.socket != PathBuf::from("/var/run/docker.sock")
                && host.socket.exists(),
            "isolated native daemon socket required"
        );
        assert!(
            host.gateway_ip == admitted_gateway_ip && host.gateway_ip.is_private(),
            "native gateway must use the admitted private bridge route"
        );
        let gateway = required_path("VEOVEO_COMPUTERS_NATIVE_GATEWAY");
        let sandbox = required_path("VEOVEO_COMPUTERS_NATIVE_SANDBOX");
        let driver = required_path("VEOVEO_COMPUTERS_NATIVE_DRIVER");
        let supervisor_image = std::env::var("VEOVEO_COMPUTERS_NATIVE_SUPERVISOR_IMAGE").unwrap();
        let output = host.output.clone();
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE")
            .expect("digest-pinned native image is required");
        assert!(image.contains("@sha256:"));
        let suffix = Uuid::now_v7().simple().to_string();
        let dir = output.join(&suffix);
        fs::create_dir_all(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let namespace = host.namespace.clone();
        let socket = host.socket.clone();
        let gateway_ip = host.gateway_ip;
        let network = namespace.clone();
        let mut cleanup = Cleanup {
            child: None,
            dir: dir.clone(),
            namespace: namespace.clone(),
            network: network.clone(),
            socket: socket.clone(),
            owned_daemon: None,
        };
        certificates(&dir, gateway_ip);
        let listener = TcpListener::bind((gateway_ip, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let config = provider_config(
            &dir,
            &host,
            port,
            &image,
            &supervisor_image,
            &sandbox,
            ssh_session_ttl_secs,
            log_level,
        );
        fs::write(dir.join("gateway.toml"), config).unwrap();
        let launch = controller::Launch::new(gateway, driver, dir.clone());
        drop(listener);
        cleanup.child = Some(launch.spawn().expect("start native provider"));
        let endpoint = format!("{gateway_ip}:{port}");
        let runtime = connect_controller(cleanup.child.as_mut().unwrap(), &dir, &endpoint).await;
        eprintln!("Native provider diagnostics: {}", dir.display());
        guest_authority::assert_denied(&dir, &endpoint).await;
        let provider = Self {
            dir,
            runtime,
            image,
            endpoint: endpoint.clone(),
            cleanup,
            launch,
        };
        (provider, format!("https://{endpoint}"))
    }

    /// Restart the real owned gateway and its in-process Docker driver, preserving the daemon,
    /// database, immutable driver state and original trust/launch inputs.
    #[allow(dead_code)] // Shared by native targets; only lifecycle qualifies controller restart.
    pub async fn restart_controller(&mut self) -> (u32, u32) {
        let child = self.cleanup.child.as_mut().expect("owned controller");
        let before = child.id();
        self.launch
            .admit_controller(child)
            .expect("admit exact qualified owned gateway group");
        controller::stop(child).expect("bounded owned controller group termination");
        self.cleanup.child.take();
        self.cleanup.child = Some(
            self.launch
                .spawn()
                .expect("restart exact retained controller inputs"),
        );
        let after = self.cleanup.child.as_ref().unwrap().id();
        assert_ne!(before, after, "real controller process must change");
        self.runtime = connect_controller(
            self.cleanup.child.as_mut().unwrap(),
            &self.dir,
            &self.endpoint,
        )
        .await;
        self.launch
            .admit_controller(self.cleanup.child.as_mut().unwrap())
            .expect("admit replacement qualified gateway group");
        (before, after)
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

async fn connect_controller(
    child: &mut Child,
    dir: &std::path::Path,
    endpoint: &str,
) -> OpenShellRuntime {
    let runtime = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Some(exit) = child.try_wait().unwrap() {
                panic!("provider exited {exit}; diagnostics at {}", dir.display());
            }
            let config = GatewayConfig::new(
                "00000000-0000-7000-8000-000000000064".parse().unwrap(),
                endpoint.to_owned(),
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
    runtime
}

fn certificates(dir: &std::path::Path, gateway_ip: std::net::Ipv4Addr) {
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
                gateway_ip.to_string(),
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

/// Existing relay child dispatch belongs to the selected native test executable.
pub async fn registry_child() -> bool {
    if std::env::var_os(docker_daemon::registry_relay::CHILD_ENV).is_some() {
        docker_daemon::registry_relay::child().await.unwrap();
        true
    } else {
        false
    }
}
