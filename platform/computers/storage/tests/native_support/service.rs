use crate::docker_daemon::{DockerDaemon, bounded, checked};
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
use veoveo_computers_runtime::{AllocationConfig, Binding, HomeAllocator, PersistentHome};

const HOST: &str = "unix:///var/run/docker.sock";
const TEST: &str = "native_service_shared_mount_and_restart";
pub struct Fixture {
    pub dir: PathBuf,
    pub provider: Uuid,
    pub endpoint: String,
    pub initial: Binding,
    pub replacement: Binding,
    daemon: Option<DockerDaemon>,
    socket_dir: PathBuf,
    service_name: String,
    image: String,
    finished: bool,
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
        let id = Uuid::now_v7();
        let root = PathBuf::from(
            std::env::var_os("VEOVEO_COMPUTERS_NATIVE_OUTPUT").expect("owned diagnostics root"),
        );
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
        let daemon = DockerDaemon::start(&dir, &socket_dir, &image, true).await;
        let engine = checked(daemon.command().args(["info", "--format", "{{.ID}}"])).await;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = listener.local_addr().unwrap().to_string();
        drop(listener);
        let config = serde_json::json!({
            "identity": {"providerId": Uuid::from_u128(100), "engineId": engine, "namespace": "storage-fixture"},
            "root": dir.join("retained"), "reserveBytes": 536870912,
            "templates": [{"fingerprint": "f".repeat(64), "capacityBytes": 536870912}],
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
            provider: Uuid::from_u128(100),
            endpoint,
            initial: Binding::new(id, "f".repeat(64)).unwrap(),
            replacement: Binding::replacement(id, Uuid::now_v7(), "f".repeat(64)).unwrap(),
            daemon: Some(daemon),
            socket_dir,
            service_name: format!("veoveo-storage-service-{}", id.simple()),
            image,
            finished: false,
        };
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
    pub async fn worker(&self, provider: Uuid) -> HomeAllocator {
        HomeAllocator::new(
            AllocationConfig::new(
                self.endpoint.clone(),
                self.dir.join("tls/ca.pem"),
                self.dir.join("tls/client.pem"),
                self.dir.join("tls/client-key.pem"),
            )
            .unwrap(),
            provider,
            "f".repeat(64),
            536870912,
        )
        .await
        .unwrap()
    }
    pub fn docker(&self) -> Command {
        self.daemon.as_ref().unwrap().command()
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
                PathBuf::from(env!("CARGO_BIN_EXE_veoveo-computer-storage")),
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
    pub async fn create(&self, suffix: &str, binding: &Binding) {
        let mount = format!(
            "type=volume,source={},target=/probe,volume-subpath=home,volume-nocopy",
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
                TEST,
                "--ignored",
                "--nocapture",
            ]);
        command
    }
    pub async fn finish(mut self) {
        self.remove("a").await;
        self.daemon.take().unwrap().finish().await;
        self.stop_service().await;
        checked(&mut self.cleanup_command()).await;
        fs::remove_dir(&self.socket_dir).unwrap();
        self.finished = true;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
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
