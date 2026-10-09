use crate::docker_daemon::{DockerDaemon, Profile, bounded, checked};
use rcgen::{
    BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;
use uuid::Uuid;
use veoveo_computers_runtime::{
    AllocationConfig, Binding, DevelopmentTemplate, HomeAllocator, PersistentHome,
};

const HOST: &str = "unix:///var/run/docker.sock";
const TEST: &str = "native_service_shared_mount_and_restart";
pub struct Fixture {
    pub dir: PathBuf,
    pub provider: veoveo_computers_runtime::ProviderInstanceId,
    pub endpoint: String,
    pub initial: Binding,
    pub replacement: Binding,
    daemon: Option<DockerDaemon>,
    socket_dir: PathBuf,
    service_name: String,
    image: String,
    finished: bool,
    cleanup_test: &'static str,
    template_capacities: std::collections::BTreeMap<String, u64>,
}
fn host() -> Command {
    let mut command = Command::new("docker");
    command.args(["--host", HOST]);
    command
}
fn certificates(dir: &Path) {
    fs::create_dir(dir).unwrap();
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    let mut params = CertificateParams::new(vec![]).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let key = KeyPair::generate().unwrap();
    fs::write(dir.join("ca.pem"), params.self_signed(&key).unwrap().pem()).unwrap();
    fs::set_permissions(dir.join("ca.pem"), fs::Permissions::from_mode(0o644)).unwrap();
    let issuer = Issuer::new(params, key);
    for (name, purpose, sans) in [
        (
            "server",
            ExtendedKeyUsagePurpose::ServerAuth,
            vec!["127.0.0.1".into()],
        ),
        ("client", ExtendedKeyUsagePurpose::ClientAuth, vec![]),
    ] {
        let mut params = CertificateParams::new(sans).unwrap();
        params.extended_key_usages = vec![purpose];
        let key = KeyPair::generate().unwrap();
        fs::write(
            dir.join(format!("{name}.pem")),
            params.signed_by(&key, &issuer).unwrap().pem(),
        )
        .unwrap();
        fs::set_permissions(
            dir.join(format!("{name}.pem")),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        let path = dir.join(format!("{name}-key.pem"));
        fs::write(&path, key.serialize_pem()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}
impl Fixture {
    pub async fn start() -> Self {
        Self::start_with_templates(vec![], TEST).await
    }
    pub async fn start_with_templates(
        selected: Vec<DevelopmentTemplate>,
        cleanup_test: &'static str,
    ) -> Self {
        let id = Uuid::now_v7();
        let root = PathBuf::from(
            std::env::var_os("VEOVEO_COMPUTERS_NATIVE_OUTPUT").expect("owned diagnostics root"),
        );
        fs::create_dir_all(&root).unwrap();
        let dir = root.join(format!("storage-service-{}", id.simple()));
        fs::create_dir(&dir).unwrap();
        fs::create_dir(dir.join("probe")).unwrap();
        let socket_dir = std::env::temp_dir().join(format!("vv-storage-{}", id.simple()));
        fs::create_dir(&socket_dir).unwrap();
        fs::set_permissions(&socket_dir, fs::Permissions::from_mode(0o700)).unwrap();
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE").expect("pinned Computer image");
        assert!(image.contains("@sha256:"));
        certificates(&dir.join("tls"));
        certificates(&dir.join("guest"));
        let profile = if !selected.is_empty() {
            Profile::NativeProvider {
                test_name: cleanup_test,
            }
        } else {
            Profile::SharedStorage
        };
        let fingerprint = selected
            .first()
            .map(DevelopmentTemplate::fingerprint)
            .unwrap_or_else(|| "f".repeat(64));
        let template_capacities: std::collections::BTreeMap<String, u64> = if selected.is_empty() {
            std::collections::BTreeMap::from([
                ("f".repeat(64), 536870912),
                ("e".repeat(64), 536870912),
            ])
        } else {
            selected
                .iter()
                .map(|t| {
                    (
                        t.fingerprint(),
                        u64::from(t.persistent_home().unwrap().capacity_mib()) * 1024 * 1024,
                    )
                })
                .collect()
        };
        let templates: Vec<_> = template_capacities.iter().map(|(fingerprint, capacity)| serde_json::json!({"fingerprint":fingerprint,"capacityBytes":capacity})).collect();
        let images = selected
            .iter()
            .map(|t| t.image().to_owned())
            .collect::<Vec<_>>();
        let daemon = DockerDaemon::start(&dir, &socket_dir, &image, profile, &images).await;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        drop(listener);
        let config = serde_json::json!({
            "providerId": "00000000-0000-7000-8000-000000000064", "namespace": "storage-fixture",
            "root": dir.join("retained"), "reserveBytes": 536870912,
            "templates": templates,
            "dockerSocket": daemon.socket, "pluginName": "veoveo-retained",
            "pluginSocket": socket_dir.join("volume.sock"), "listen": endpoint,
            "tls": {"workerCa": dir.join("tls/ca.pem"), "certificate": dir.join("tls/server.pem"), "privateKey": dir.join("tls/server-key.pem")},
        });
        fs::write(
            dir.join("config.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let fixture = Self {
            dir,
            provider: "00000000-0000-7000-8000-000000000064".parse().unwrap(),
            endpoint,
            initial: Binding::new(id, fingerprint.clone()).unwrap(),
            replacement: Binding::replacement(id, Uuid::now_v7(), fingerprint).unwrap(),
            daemon: Some(daemon),
            socket_dir,
            service_name: format!("veoveo-storage-service-{}", id.simple()),
            image,
            finished: false,
            cleanup_test,
            template_capacities,
        };
        // Stock user mounts use Engine copy-on-mount behavior. An absent target
        // avoids initialization before the allocator can register its writer.
        // This probe is inside the owned daemon, whose cleanup owns every child.
        checked(fixture.docker().args([
            "run",
            "--rm",
            "--pull=never",
            "--name=fixture-image-absence",
            "--network=none",
            "--read-only",
            "--user=10001:10001",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--memory=64m",
            "--pids-limit=32",
            "--entrypoint=/bin/sh",
            &fixture.daemon.as_ref().unwrap().image_id,
            "-c",
            "test ! -e /probe && test ! -L /probe",
        ]))
        .await;
        fixture.start_service().await;
        fixture
            .daemon
            .as_ref()
            .unwrap()
            .register(
                "veoveo-retained",
                &format!("unix://{}/volume.sock", fixture.socket_dir.display()),
                &fixture.dir.join("probe"),
            )
            .await;
        println!("Native allocator diagnostics: {}", fixture.dir.display());
        fixture
    }
    pub async fn worker(
        &self,
        provider: veoveo_computers_runtime::ProviderInstanceId,
    ) -> HomeAllocator {
        self.worker_template(provider, self.initial.template_fingerprint())
            .await
    }
    pub async fn worker_template(
        &self,
        provider: veoveo_computers_runtime::ProviderInstanceId,
        fingerprint: &str,
    ) -> HomeAllocator {
        HomeAllocator::new(
            AllocationConfig::new(
                self.endpoint.clone(),
                self.dir.join("tls/ca.pem"),
                self.dir.join("tls/client.pem"),
                self.dir.join("tls/client-key.pem"),
            )
            .unwrap(),
            provider,
            fingerprint.into(),
            *self
                .template_capacities
                .get(fingerprint)
                .expect("admitted fixture template"),
        )
        .await
        .unwrap()
    }
    pub fn docker(&self) -> Command {
        self.daemon.as_ref().unwrap().command()
    }
    pub fn docker_socket(&self) -> PathBuf {
        self.daemon.as_ref().unwrap().socket.clone()
    }
    /// Capture only the allocator's closed, selected Prepare observations before Drop.
    pub async fn prepare_home(
        &self,
        worker: &HomeAllocator,
        binding: &Binding,
    ) -> Result<(), veoveo_computers_runtime::RuntimeFailure> {
        let result = worker.prepare(binding).await;
        if result.is_err() {
            let mut command = host();
            command
                .args(["logs", "--tail", "64", &self.service_name])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            let mut capture = match command.spawn() {
                Ok(mut child) => {
                    capture_prepare_reader(
                        &mut child,
                        self.provider,
                        binding.computer_id(),
                        Duration::from_secs(5),
                    )
                    .await
                }
                Err(_) => PrepareCapture {
                    observations: Vec::new(),
                    read_status: PrepareReadStatus::SpawnUnavailable,
                    cleanup_status: PrepareCleanupStatus::NotSpawned,
                },
            };
            // Empty capture cannot establish a failure stage or settle allocation effects.
            let mut encoded = serde_json::to_vec(&capture).ok();
            if encoded.as_ref().is_some_and(|bytes| bytes.len() > 8192) {
                capture.observations.clear();
                capture.read_status = PrepareReadStatus::Truncated;
                encoded = serde_json::to_vec(&capture).ok();
            }
            if let Some(bytes) = encoded {
                let _ = fs::write(self.dir.join("prepare-failure-diagnostics.json"), &bytes);
                eprintln!(
                    "Selected allocator Prepare capture: {}",
                    String::from_utf8_lossy(&bytes)
                );
            }
        }
        result
    }
    pub fn allocation_config(&self) -> AllocationConfig {
        AllocationConfig::new(
            self.endpoint.clone(),
            self.dir.join("tls/ca.pem"),
            self.dir.join("tls/client.pem"),
            self.dir.join("tls/client-key.pem"),
        )
        .unwrap()
    }
    pub fn container(&self, suffix: &str) -> String {
        format!("fixture-{suffix}")
    }
    fn service_command(&self) -> Command {
        let mut command = host();
        command.args([
            "run",
            "--detach",
            "--pull",
            "never",
            "--name",
            &self.service_name,
            "--privileged",
            "--user",
            "0:0",
            "--network",
            "host",
            "--memory",
            "1g",
            "--cpus",
            "2",
            "--pids-limit",
            "256",
            "--mount",
            "type=bind,source=/dev,target=/dev",
        ]);
        for (source, target, options) in [
            (
                self.dir.clone(),
                self.dir.clone(),
                ",bind-propagation=rshared",
            ),
            (self.socket_dir.clone(), self.socket_dir.clone(), ""),
            (
                self.daemon
                    .as_ref()
                    .unwrap()
                    .socket
                    .parent()
                    .unwrap()
                    .to_owned(),
                self.daemon
                    .as_ref()
                    .unwrap()
                    .socket
                    .parent()
                    .unwrap()
                    .to_owned(),
                "",
            ),
            (
                option_env!("CARGO_BIN_EXE_veoveo-computer-storage")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(
                            std::env::var_os("VEOVEO_COMPUTERS_NATIVE_ALLOCATOR")
                                .expect("qualified allocator binary"),
                        )
                    }),
                PathBuf::from("/storage"),
                ",readonly",
            ),
        ] {
            command.arg("--mount").arg(format!(
                "type=bind,source={},target={}{}",
                source.display(),
                target.display(),
                options
            ));
        }
        command
            .args([
                "--entrypoint",
                "/bin/sh",
                &self.image,
                "-c",
                "chown 0:0 \"$1\" \"$2\" && exec /storage --config \"$3\"",
                "fixture",
            ])
            .arg(&self.socket_dir)
            .arg(self.dir.join("tls/server-key.pem"))
            .arg(self.dir.join("config.json"));
        command
    }
    pub async fn start_service(&self) {
        checked(&mut self.service_command()).await;
        let ready = tokio::time::Instant::now() + Duration::from_secs(15);
        let worker = self.worker(self.provider).await;
        loop {
            if worker.ready().await.is_ok() {
                break;
            }
            let running = checked(host().args([
                "inspect",
                "--format",
                "{{.State.Running}}",
                &self.service_name,
            ]))
            .await;
            assert_eq!(
                running,
                "true",
                "allocator exited; inspect {}/allocator.log",
                self.dir.display()
            );
            assert!(
                tokio::time::Instant::now() < ready,
                "allocator readiness; inspect {}",
                self.dir.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    pub async fn stop_service(&self) {
        let logs = bounded(host().args(["logs", &self.service_name])).await;
        fs::write(self.dir.join("allocator.log"), logs.stderr).unwrap();
        checked(host().args(["rm", "--force", &self.service_name])).await;
    }
    pub async fn cold_restart(&self) {
        self.stop_service().await;
        self.daemon.as_ref().unwrap().restart().await;
        self.start_service().await;
        self.daemon.as_ref().unwrap().restore_socket_access().await;
    }
    pub async fn create(&self, suffix: &str, binding: &Binding) {
        let mount = format!(
            "type=volume,source={},target=/probe,volume-subpath=home",
            PersistentHome::volume_name(binding.computer_id()).unwrap()
        );
        let mut command = self.docker();
        command.args([
            "create",
            "--name",
            &self.container(suffix),
            "--network",
            "none",
            "--user",
            "10001:10001",
            "--read-only",
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "--pids-limit",
            "32",
            "--memory",
            "64m",
            "--cpus",
            "1",
            "--mount",
            &mount,
        ]);
        let mut labels = binding.labels();
        labels.extend([
            ("openshell.ai/managed-by".into(), "openshell".into()),
            (
                "openshell.ai/sandbox-namespace".into(),
                "storage-fixture".into(),
            ),
            ("openshell.ai/sandbox-name".into(), binding.name()),
            (
                "openshell.ai/sandbox-id".into(),
                format!("resource-{suffix}"),
            ),
        ]);
        for (key, value) in labels {
            command.arg("--label").arg(format!("{key}={value}"));
        }
        command.args([
            &self.daemon.as_ref().unwrap().image_id,
            "/bin/sleep",
            "infinity",
        ]);
        checked(&mut command).await;
    }
    pub async fn start_container(&self, suffix: &str, allowed: bool) {
        let result = bounded(self.docker().args(["start", &self.container(suffix)])).await;
        assert_eq!(
            result.status.success(),
            allowed,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if !allowed {
            assert!(
                String::from_utf8_lossy(&result.stderr)
                    .contains("current registered Computer writer")
            );
        }
    }
    pub async fn remove(&self, suffix: &str) {
        checked(
            self.docker()
                .args(["rm", "--force", &self.container(suffix)]),
        )
        .await;
    }
    pub async fn write(&self, value: &str) {
        checked(self.docker().args([
            "exec",
            &self.container("a"),
            "/bin/sh",
            "-c",
            "printf '%s' \"$1\" > /probe/value",
            "fixture",
            value,
        ]))
        .await;
    }
    pub async fn copy_and_assert(&self, expected: &str) {
        checked(
            self.docker()
                .args(["cp", &format!("{}:/probe/value", self.container("a"))])
                .arg(self.dir.join("copied")),
        )
        .await;
        assert_eq!(
            fs::read_to_string(self.dir.join("copied")).unwrap(),
            expected
        );
    }
    pub async fn assert_content(&self, suffix: &str, expected: &str) {
        assert_eq!(
            checked(
                self.docker()
                    .args(["exec", &self.container(suffix), "cat", "/probe/value"])
            )
            .await,
            expected
        );
    }
    pub async fn hold_home(&self) {
        let mount = self
            .dir
            .join("retained/homes")
            .join(self.initial.computer_id().simple().to_string())
            .join("mount");
        checked(
            host()
                .args([
                    "run",
                    "--detach",
                    "--pull",
                    "never",
                    "--name",
                    &format!("{}-held", self.service_name),
                    "--network",
                    "none",
                    "--user",
                    "10001:10001",
                    "--read-only",
                    "--cap-drop",
                    "ALL",
                    "--security-opt",
                    "no-new-privileges",
                    "--pids-limit",
                    "32",
                    "--memory",
                    "64m",
                    "--cpus",
                    "1",
                    "--mount",
                ])
                .arg(format!(
                    "type=bind,source={},target=/held,bind-propagation=rprivate",
                    mount.display()
                ))
                .args(["--entrypoint", "/bin/sleep", &self.image, "infinity"]),
        )
        .await;
    }
    pub async fn write_held(&self, value: &str) {
        checked(host().args([
            "exec",
            &format!("{}-held", self.service_name),
            "/bin/sh",
            "-c",
            "printf '%s' \"$1\" > /held/home/value",
            "fixture",
            value,
        ]))
        .await;
        assert_eq!(
            checked(host().args([
                "exec",
                &format!("{}-held", self.service_name),
                "cat",
                "/held/home/value"
            ]))
            .await,
            value
        );
    }
    pub async fn release_home(&self) {
        checked(host().args(["rm", "--force", &format!("{}-held", self.service_name)])).await;
    }
    pub async fn drop_handoff_reply(&self, operation: Uuid) {
        self.drop_storage_reply(serde_json::json!({
            "schema": "veoveo.ai/computer-storage/v1", "operation": "handoff", "providerId": self.provider,
            "computerId": self.initial.computer_id(), "operationId": operation, "sourceInstanceId": self.initial.computer_id(),
            "sourceTemplateFingerprint": self.initial.template_fingerprint(), "sourceResourceId": "resource-a",
            "targetInstanceId": self.replacement.replacement_instance_id().unwrap(), "targetTemplateFingerprint": self.replacement.template_fingerprint(),
        })).await;
    }
    pub async fn drop_abandon_reply(&self, operation: Uuid, source: &Binding, target: &Binding) {
        self.drop_storage_reply(serde_json::json!({
            "schema": "veoveo.ai/computer-storage/v1", "operation": "abandon", "providerId": self.provider,
            "computerId": source.computer_id(), "operationId": operation,
            "sourceInstanceId": source.replacement_instance_id().unwrap_or(source.computer_id()),
            "sourceTemplateFingerprint": source.template_fingerprint(),
            "targetInstanceId": target.replacement_instance_id().unwrap(), "targetTemplateFingerprint": target.template_fingerprint(),
        })).await;
    }
    async fn drop_storage_reply(&self, request: serde_json::Value) {
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, pem::PemObject};
        use tokio::io::AsyncWriteExt;
        let ca = fs::read(self.dir.join("tls/ca.pem")).unwrap();
        let cert = fs::read(self.dir.join("tls/client.pem")).unwrap();
        let key = fs::read(self.dir.join("tls/client-key.pem")).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        for cert in CertificateDer::pem_slice_iter(&ca) {
            roots.add(cert.unwrap()).unwrap();
        }
        let tls = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_client_auth_cert(
            CertificateDer::pem_slice_iter(&cert)
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap(),
            PrivateKeyDer::from_pem_slice(&key).unwrap(),
        )
        .unwrap();
        let raw = serde_json::to_vec(&request).unwrap();
        assert!(raw.len() <= 1024);
        tokio::time::timeout(Duration::from_secs(5), async {
            let tcp = tokio::net::TcpStream::connect(&self.endpoint)
                .await
                .unwrap();
            let mut socket = tokio_rustls::TlsConnector::from(std::sync::Arc::new(tls))
                .connect(ServerName::try_from("127.0.0.1").unwrap(), tcp)
                .await
                .unwrap();
            socket.write_u32(raw.len() as u32).await.unwrap();
            socket.write_all(&raw).await.unwrap();
            socket.flush().await.unwrap();
            // No response read: the next identical request must resolve the
            // durable transition, including a concurrently completing first one.
        })
        .await
        .unwrap();
    }
    fn cleanup_command(&self) -> Command {
        let mut command = host();
        command.args([
            "run",
            "--rm",
            "--pull",
            "never",
            "--privileged",
            "--network",
            "none",
            "--user",
            "0:0",
            "--mount",
            "type=bind,source=/dev,target=/dev",
        ]);
        for (source, target, options) in [
            (
                self.dir.clone(),
                self.dir.clone(),
                ",bind-propagation=rshared",
            ),
            (self.socket_dir.clone(), self.socket_dir.clone(), ""),
            (
                std::env::current_exe().unwrap(),
                PathBuf::from("/storage-test"),
                ",readonly",
            ),
        ] {
            command.arg("--mount").arg(format!(
                "type=bind,source={},target={}{}",
                source.display(),
                target.display(),
                options
            ));
        }
        command
            .arg("--env")
            .arg(format!(
                "VEOVEO_STORAGE_SERVICE_CLEANUP={}",
                self.dir.display()
            ))
            .arg("--env")
            .arg(format!(
                "VEOVEO_STORAGE_SERVICE_SOCKET={}",
                self.socket_dir.display()
            ))
            .args([
                "--entrypoint",
                "/storage-test",
                &self.image,
                "--exact",
                self.cleanup_test,
                "--ignored",
                "--nocapture",
            ]);
        command
    }
    pub async fn finish(mut self, remaining: Option<&str>) {
        if let Some(remaining) = remaining {
            self.remove(remaining).await;
        }
        self.daemon.take().unwrap().finish().await;
        self.stop_service().await;
        checked(&mut self.cleanup_command()).await;
        assert!(
            !self.dir.join("retained").exists(),
            "retained fixture cleanup did not run"
        );
        fs::remove_dir(&self.socket_dir).unwrap();
        self.finished = true;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = std::process::Command::new("timeout")
            .args([
                "10",
                "docker",
                "--host",
                HOST,
                "rm",
                "--force",
                &format!("{}-held", self.service_name),
            ])
            .output();
        if let Ok(logs) = std::process::Command::new("timeout")
            .args(["5", "docker", "--host", HOST, "logs", &self.service_name])
            .output()
        {
            let _ = fs::write(self.dir.join("allocator.log"), logs.stderr);
        }
        drop(self.daemon.take());
        let _ = std::process::Command::new("timeout")
            .args([
                "10",
                "docker",
                "--host",
                HOST,
                "rm",
                "--force",
                &self.service_name,
            ])
            .output();
        let logs = self.dir.join("allocator.log");
        if !logs.exists() {
            let _ = fs::write(logs, "fixture interrupted; allocator and daemon removed");
        }
        let command = self.cleanup_command();
        let _ = std::process::Command::new("timeout")
            .arg("30")
            .arg(command.as_std().get_program())
            .args(command.as_std().get_args())
            .output();
        let _ = fs::remove_dir(&self.socket_dir);
    }
}

pub fn cleanup() {
    let root = PathBuf::from(std::env::var_os("VEOVEO_STORAGE_SERVICE_CLEANUP").unwrap());
    let retained = root.join("retained");
    if let Ok(entries) = fs::read_dir(retained.join("homes")) {
        for entry in entries {
            let entry = entry.unwrap();
            let mount = entry.path().join("mount");
            let result = std::process::Command::new("findmnt")
                .arg("--mountpoint")
                .arg(&mount)
                .output()
                .unwrap();
            if result.status.success() {
                nix::mount::umount(&mount).unwrap();
            } else {
                assert_eq!(result.status.code(), Some(1));
            }
            let backing = entry.path().join("home.ext4");
            let devices = std::process::Command::new("losetup")
                .args(["--list", "--noheadings", "--output", "NAME", "--associated"])
                .arg(&backing)
                .output()
                .unwrap();
            assert!(devices.status.success());
            for device in String::from_utf8(devices.stdout).unwrap().lines() {
                assert!(
                    device
                        .strip_prefix("/dev/loop")
                        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                );
                assert!(
                    std::process::Command::new("losetup")
                        .args(["--detach", device])
                        .status()
                        .unwrap()
                        .success()
                );
            }
            let remaining = std::process::Command::new("losetup")
                .args(["--list", "--noheadings", "--output", "NAME", "--associated"])
                .arg(&backing)
                .output()
                .unwrap();
            assert!(remaining.status.success() && remaining.stdout.is_empty());
        }
    }
    if retained.exists() {
        fs::remove_dir_all(retained).unwrap();
    }
    for directory in ["tls", "guest"] {
        if root.join(directory).exists() {
            fs::remove_dir_all(root.join(directory)).unwrap();
        }
    }
    let socket = PathBuf::from(std::env::var_os("VEOVEO_STORAGE_SERVICE_SOCKET").unwrap());
    if socket.join("volume.sock").exists() {
        fs::remove_file(socket.join("volume.sock")).unwrap();
    }
    use std::os::unix::fs::MetadataExt;
    let owner = fs::metadata(&root).unwrap();
    nix::unistd::chown(
        &socket,
        Some(nix::unistd::Uid::from_raw(owner.uid())),
        Some(nix::unistd::Gid::from_raw(owner.gid())),
    )
    .unwrap();
}

fn selected_prepare_observations(
    bytes: &[u8],
    provider: veoveo_computers_runtime::ProviderInstanceId,
    computer: Uuid,
) -> Vec<veoveo_computer_storage::PrepareFailureObservation> {
    use veoveo_computer_storage::{PREPARE_FAILURE_PREFIX, PrepareFailureObservation};
    let mut admitted = Vec::new();
    let mut size = 2; // JSON array delimiters; count each comma below.
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.len() > 1024 || admitted.len() == 16 {
            continue;
        }
        let Ok(text) = std::str::from_utf8(line) else {
            continue;
        };
        let Some(json) = text.strip_prefix(PREPARE_FAILURE_PREFIX) else {
            continue;
        };
        let Ok(observation) = serde_json::from_str::<PrepareFailureObservation>(json) else {
            continue;
        };
        if observation.provider_id != provider || observation.computer_id != computer {
            continue;
        }
        let Ok(encoded) = serde_json::to_vec(&observation) else {
            continue;
        };
        let added = encoded.len() + usize::from(!admitted.is_empty());
        if size + added > 8192 - 256 {
            break;
        }
        size += added;
        admitted.push(observation);
    }
    admitted
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
enum PrepareReadStatus {
    Captured,
    #[vocabulary(rename = "spawn_unavailable")]
    SpawnUnavailable,
    #[vocabulary(rename = "missing_pipe")]
    MissingPipe,
    #[vocabulary(rename = "read_unavailable")]
    ReadUnavailable,
    Deadline,
    Truncated,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
enum PrepareCleanupStatus {
    #[vocabulary(rename = "not_spawned")]
    NotSpawned,
    Reaped,
    Unresolved,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PrepareCapture {
    observations: Vec<veoveo_computer_storage::PrepareFailureObservation>,
    read_status: PrepareReadStatus,
    cleanup_status: PrepareCleanupStatus,
}
async fn capture_prepare_reader(
    child: &mut tokio::process::Child,
    provider: veoveo_computers_runtime::ProviderInstanceId,
    computer: Uuid,
    budget: Duration,
) -> PrepareCapture {
    use tokio::io::AsyncReadExt;
    let read = async {
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return (Vec::new(), PrepareReadStatus::MissingPipe);
        };
        let mut stdout = stdout.take(8193);
        let mut stderr = stderr.take(8193);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let (a, b) = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
        if a.is_err() || b.is_err() {
            return (Vec::new(), PrepareReadStatus::ReadUnavailable);
        }
        let truncated = out.len() > 8192 || err.len() > 8192;
        let observations =
            selected_prepare_observations(&err[..err.len().min(8192)], provider, computer);
        if truncated {
            return (observations, PrepareReadStatus::Truncated);
        }
        match child.wait().await {
            Ok(status) if status.success() => (observations, PrepareReadStatus::Captured),
            _ => (observations, PrepareReadStatus::ReadUnavailable),
        }
    };
    let (observations, read_status) = match tokio::time::timeout(budget, read).await {
        Ok(result) => result,
        Err(_) => (Vec::new(), PrepareReadStatus::Deadline),
    };
    // Only this read-only CLI PID is owned; reaping never settles allocation or a container.
    let cleanup_status = if matches!(child.try_wait(), Ok(Some(_))) {
        PrepareCleanupStatus::Reaped
    } else {
        let _ = child.start_kill();
        match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
            Ok(Ok(_)) => PrepareCleanupStatus::Reaped,
            _ => PrepareCleanupStatus::Unresolved,
        }
    };
    PrepareCapture {
        observations,
        read_status,
        cleanup_status,
    }
}

#[cfg(test)]
mod prepare_diagnostic_tests {
    use super::*;
    use veoveo_computer_storage::{
        PREPARE_FAILURE_PREFIX, PrepareFailureObservation, PrepareStage, StorageError,
    };
    #[test]
    fn selected_prepare_failure_reports_keep_actual_stage_and_exclude_untrusted_messages() {
        let provider = veoveo_computers_runtime::ProviderInstanceId::new();
        let computer = Uuid::now_v7();
        let mut bytes = b"private-path certificate SYNTHETIC_SECRET\nretained-storage: authenticated request failed: identity mismatch\n".to_vec();
        for stage in [
            PrepareStage::CapacityAdmission,
            PrepareStage::EngineAdmission,
            PrepareStage::FilesystemPreparation,
            PrepareStage::VolumeAdmission,
        ] {
            let observation = PrepareFailureObservation {
                provider_id: provider,
                computer_id: computer,
                stage,
                cause: StorageError::IdentityMismatch,
            };
            bytes.extend_from_slice(
                format!(
                    "{PREPARE_FAILURE_PREFIX}{}\n",
                    serde_json::to_string(&observation).unwrap()
                )
                .as_bytes(),
            );
        }
        let foreign = PrepareFailureObservation {
            provider_id: provider,
            computer_id: Uuid::now_v7(),
            stage: PrepareStage::VolumeAdmission,
            cause: StorageError::WriterDenied,
        };
        bytes.extend_from_slice(
            format!(
                "{PREPARE_FAILURE_PREFIX}{}\n",
                serde_json::to_string(&foreign).unwrap()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(format!("{PREPARE_FAILURE_PREFIX}{{\"providerId\":\"{provider}\",\"computerId\":\"{computer}\",\"stage\":\"volume_admission\",\"cause\":\"identity_mismatch\",\"message\":\"SYNTHETIC_SECRET\"}}\n").as_bytes());
        let reports = selected_prepare_observations(&bytes, provider, computer);
        assert_eq!(reports.len(), 4);
        assert_eq!(reports[0].stage, PrepareStage::CapacityAdmission);
        assert_eq!(reports[3].stage, PrepareStage::VolumeAdmission);
        let json = serde_json::to_string(&reports).unwrap();
        assert!(json.contains("computerId"));
        assert!(json.contains("identity_mismatch"));
        assert!(!json.contains("SYNTHETIC_SECRET"));
        assert!(!json.contains("private-path"));
        assert!(selected_prepare_observations(&bytes, provider, Uuid::now_v7()).is_empty());
    }
    #[tokio::test]
    async fn diagnostic_reader_bounds_bytes_and_reaps_oversize_missing_pipe_and_deadline() {
        use std::process::Stdio;
        let provider = veoveo_computers_runtime::ProviderInstanceId::new();
        let computer = Uuid::now_v7();
        let mut oversized = Command::new("/bin/sh");
        oversized
            .args(["-c", "printf '%10000s' '' >&2"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = oversized.spawn().unwrap();
        let capture =
            capture_prepare_reader(&mut child, provider, computer, Duration::from_secs(2)).await;
        assert_eq!(capture.read_status, PrepareReadStatus::Truncated);
        assert_eq!(capture.cleanup_status, PrepareCleanupStatus::Reaped);
        assert!(serde_json::to_vec(&capture).unwrap().len() <= 8192);
        for (pipes, expected) in [
            (false, PrepareReadStatus::MissingPipe),
            (true, PrepareReadStatus::Deadline),
        ] {
            let mut command = Command::new("/bin/sleep");
            command.arg("10").kill_on_drop(true);
            if pipes {
                command.stdout(Stdio::piped()).stderr(Stdio::piped());
            }
            let mut child = command.spawn().unwrap();
            let capture =
                capture_prepare_reader(&mut child, provider, computer, Duration::from_millis(20))
                    .await;
            assert_eq!(capture.read_status, expected);
            assert_eq!(capture.cleanup_status, PrepareCleanupStatus::Reaped);
            assert!(child.try_wait().unwrap().is_some());
        }
    }
}
