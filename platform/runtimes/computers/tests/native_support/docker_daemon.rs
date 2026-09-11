//! A disposable daemon owns plugin registration caches as well as its containers.
use std::{
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};
use tokio::process::Command;
use uuid::Uuid;
#[path = "registry_relay.rs"]
pub mod registry_relay;

// Latest stable Engine release verified against Moby and the official image on
// 2026-09-09. This fixture does not upgrade the installation's Docker daemon.
const IMAGE: &str = "docker.io/library/docker:29.8.0-dind@sha256:77759fdec1efef224ba7110ef7b5b3c6af6164ffaef5441d3beba059bde8b857";
const HOST: &str = "unix:///var/run/docker.sock";
#[derive(Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)] // Each native harness selects its own fixture profile.
pub enum Profile {
    Volume,
    SharedStorage,
    NativeProvider { test_name: &'static str },
}
pub fn host() -> Command {
    let mut command = Command::new("docker");
    command.args(["--host", HOST]);
    command
}

pub struct DockerDaemon {
    pub socket: PathBuf,
    pub image_id: String,
    name: String,
    root: PathBuf,
    diagnostics: PathBuf,
    finished: bool,
    profile: Profile,
    host_bridge: Option<(String, String)>,
    registries: std::collections::BTreeSet<registry_relay::Registry>,
}
fn host_bridge() -> Option<(String, String)> {
    Some((
        std::fs::read_to_string("/sys/class/net/docker0/ifindex").ok()?,
        std::fs::read_to_string("/sys/class/net/docker0/address").ok()?,
    ))
}
impl DockerDaemon {
    pub async fn start(
        diagnostics: &Path,
        plugin_socket: &Path,
        computer_image: &str,
        profile: Profile,
        admitted_images: &[String],
    ) -> Self {
        let name = format!("veoveo-docker-probe-{}", Uuid::now_v7().simple());
        let root = std::env::temp_dir().join(&name);
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        std::fs::create_dir(root.join("run")).unwrap();
        let socket = root.join("run/docker.sock");
        let image_id =
            checked(host().args(["image", "inspect", computer_image, "--format", "{{.Id}}"])).await;
        assert!(image_id.starts_with("sha256:") && image_id.len() == 71);
        let registries = if matches!(profile, Profile::NativeProvider { .. }) {
            std::iter::once(computer_image)
                .chain(admitted_images.iter().map(String::as_str))
                .map(registry_relay::Registry::for_image)
                .collect()
        } else {
            std::collections::BTreeSet::new()
        };
        let archive = diagnostics
            .parent()
            .unwrap()
            .join(format!("{}.tar", image_id.replace(':', "-")));
        if !matches!(profile, Profile::NativeProvider { .. }) && !archive.exists() {
            let temporary = archive.with_extension(format!("{}.tmp", Uuid::now_v7()));
            checked(
                host()
                    .args(["image", "save", "--output"])
                    .arg(&temporary)
                    .arg(computer_image),
            )
            .await;
            std::fs::rename(temporary, &archive).unwrap();
        }
        let daemon = Self {
            socket,
            image_id,
            name,
            root,
            diagnostics: diagnostics.into(),
            finished: false,
            profile,
            host_bridge: host_bridge(),
            registries,
        };
        let mut command = host();
        command.args([
            "create",
            "--pull",
            "never",
            "--name",
            &daemon.name,
            "--privileged",
            "--network",
            if matches!(profile, Profile::NativeProvider { .. }) {
                "bridge"
            } else {
                "none"
            },
            "--memory",
            "1g",
            "--cpus",
            "2",
            "--pids-limit",
            "512",
            "--tmpfs",
            "/tmp:rw,exec,mode=1777",
        ]);
        for registry in &daemon.registries {
            command
                .arg("--add-host")
                .arg(format!("{}:127.0.0.1", registry.host));
        }
        for (source, target) in [
            (daemon.root.join("data"), PathBuf::from("/var/lib/docker")),
            (daemon.root.join("run"), daemon.root.join("run")),
            (diagnostics.to_owned(), diagnostics.to_owned()),
            (plugin_socket.to_owned(), plugin_socket.to_owned()),
        ] {
            let propagation = if profile != Profile::Volume && source == diagnostics {
                ",bind-propagation=rslave"
            } else {
                ""
            };
            command.arg("--mount").arg(format!(
                "type=bind,source={},target={}{}",
                source.display(),
                target.display(),
                propagation,
            ));
        }
        if matches!(profile, Profile::NativeProvider { .. }) {
            let supervisor = PathBuf::from(
                std::env::var_os("VEOVEO_COMPUTERS_NATIVE_SUPERVISOR")
                    .expect("qualified supervisor binary"),
            );
            assert!(supervisor.is_absolute());
            command.arg("--mount").arg(format!(
                "type=bind,source={},target={},readonly",
                supervisor.display(),
                supervisor.display()
            ));
        }
        command
            .args([
                "--entrypoint",
                "/usr/local/bin/dind",
                IMAGE,
                "docker-init",
                "--",
                "dockerd",
                "--host",
            ])
            .arg(format!("unix://{}", daemon.socket.display()))
            .args([
                "--data-root=/var/lib/docker",
                "--exec-root=/run/probe-exec",
                "--bridge=none",
                "--storage-driver=overlay2",
            ]);
        if matches!(profile, Profile::NativeProvider { .. }) {
            for registry in &daemon.registries {
                command.arg(format!("--insecure-registry={}", registry.authority));
            }
            command.args([
                "--iptables=true",
                "--ip6tables=false",
                "--ip-forward=true",
                "--ip-masq=true",
            ]);
        } else {
            command.args([
                "--iptables=false",
                "--ip6tables=false",
                "--ip-forward=false",
                "--ip-masq=false",
            ]);
        }
        // Cold host image/container preparation can outlast the ordinary command
        // budget. Keep that setup allowance explicit and never resend Create on
        // a lost response. A timeout names the owned intent for later cleanup.
        command.kill_on_drop(true);
        let started = std::time::Instant::now();
        let output = tokio::time::timeout(Duration::from_secs(60), command.output())
            .await
            .unwrap_or_else(|_| panic!("fixture Docker create deadline for {}", daemon.name))
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!(
            "Native private Docker create: {} ms",
            started.elapsed().as_millis()
        );
        // Validate the daemon's namespace choice before its entrypoint runs.
        // Even --bridge=none/--iptables=false cannot make host-network DinD
        // safe: its initialization can delete the host's default bridge.
        let mode = checked(host().args([
            "inspect",
            "--format",
            "{{.HostConfig.NetworkMode}}",
            &daemon.name,
        ]))
        .await;
        assert!(
            matches!(mode.as_str(), "bridge" | "none"),
            "a fixture daemon must own its network namespace"
        );
        checked(host().args(["start", &daemon.name])).await;
        let namespace =
            checked(host().args(["exec", &daemon.name, "readlink", "/proc/self/ns/net"])).await;
        assert_ne!(
            namespace,
            std::fs::read_link("/proc/self/ns/net")
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert_eq!(
            host_bridge(),
            daemon.host_bridge,
            "fixture changed host bridge identity"
        );
        let uid = checked(Command::new("id").arg("-u")).await;
        let gid = checked(Command::new("id").arg("-g")).await;
        let ready = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let result = bounded(
                host()
                    .args(["exec", &daemon.name, "test", "-S"])
                    .arg(&daemon.socket),
            )
            .await;
            if result.status.success() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < ready,
                "isolated daemon readiness"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        checked(
            host()
                .args(["exec", &daemon.name, "chown", &format!("{uid}:{gid}")])
                .arg(&daemon.socket),
        )
        .await;
        loop {
            if bounded(daemon.command().arg("info")).await.status.success() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < ready,
                "isolated Docker API readiness"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if let Profile::NativeProvider { test_name } = profile {
            // Pull only the already-published local candidate. A registry pull
            // preserves the manifest identity that a Docker save/load loses.
            registry_relay::pull(&daemon, computer_image, test_name).await;
            for image in admitted_images {
                if image != computer_image {
                    registry_relay::pull(&daemon, image, test_name).await;
                }
            }
            assert_eq!(
                checked(daemon.command().args([
                    "image",
                    "inspect",
                    computer_image,
                    "--format",
                    "{{.Id}}"
                ]))
                .await,
                daemon.image_id
            );
        } else {
            checked(
                daemon
                    .command()
                    .args(["image", "load", "--input"])
                    .arg(archive),
            )
            .await;
        }
        let loaded = checked(daemon.command().args([
            "image",
            "inspect",
            &daemon.image_id,
            "--format",
            "{{.Id}}",
        ]))
        .await;
        assert_eq!(loaded, daemon.image_id);
        daemon
    }
    pub fn command(&self) -> Command {
        let mut command = Command::new("docker");
        command
            .arg("--host")
            .arg(format!("unix://{}", self.socket.display()));
        command
    }
    #[allow(dead_code)] // Retained-storage qualification also replaces the daemon process.
    pub async fn restart(&self) {
        checked(host().args(["stop", "--time", "3", &self.name])).await;
        checked(host().args(["start", &self.name])).await;
        // Its volume restore needs the helper before the HTTP API can start.
        // The caller starts that helper before waiting for API readiness.
    }
    #[allow(dead_code)] // Restore host-user access after the private socket is recreated.
    pub async fn restore_socket_access(&self) {
        let endpoint = format!("unix://{}", self.socket.display());
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        loop {
            // Version checks API readiness without requiring a volume-plugin
            // inventory. The caller has restarted the storage helper.
            let result = bounded(host().args([
                "exec",
                &self.name,
                "docker",
                "--host",
                &endpoint,
                "version",
                "--format",
                "{{.Server.Version}}",
            ]))
            .await;
            if result.status.success() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "isolated daemon restart readiness"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let uid = checked(Command::new("id").arg("-u")).await;
        let gid = checked(Command::new("id").arg("-g")).await;
        checked(
            host()
                .args(["exec", &self.name, "chown", &format!("{uid}:{gid}")])
                .arg(&self.socket),
        )
        .await;
        assert_eq!(
            host_bridge(),
            self.host_bridge,
            "restart changed host bridge identity"
        );
    }
    pub async fn register(&self, name: &str, address: &str, home: &Path) {
        checked(host().args(["exec", &self.name, "/bin/sh", "-c", "mkdir -p /etc/docker/plugins && (set -C; printf '%s' \"$1\" > \"$2\") && chown 10001:10001 \"$3\"", "fixture", address, &format!("/etc/docker/plugins/{name}.spec")]).arg(home)).await;
    }
    pub async fn finish(mut self) {
        let logs = bounded(host().args(["logs", &self.name])).await;
        std::fs::write(self.diagnostics.join("docker-daemon.log"), logs.stderr).unwrap();
        assert!(
            self.remove_owned_networks(),
            "isolated provider network cleanup"
        );
        checked(host().args(["rm", "--force", "--volumes", &self.name])).await;
        assert!(
            self.remove_files(),
            "isolated daemon files were not removed"
        );
        self.finished = true;
        assert_eq!(
            host_bridge(),
            self.host_bridge,
            "fixture changed host bridge identity"
        );
    }
    fn remove_files(&self) -> bool {
        let removed = std::process::Command::new("timeout")
            .args([
                "15",
                "docker",
                "--host",
                HOST,
                "run",
                "--rm",
                "--network",
                "none",
                "--mount",
            ])
            .arg(format!(
                "type=bind,source={},target=/fixture",
                self.root.display()
            ))
            .args([
                "--entrypoint",
                "/bin/rm",
                IMAGE,
                "-rf",
                "/fixture/data",
                "/fixture/run",
            ])
            .output()
            .is_ok_and(|output| output.status.success());
        removed && std::fs::remove_dir(&self.root).is_ok()
    }
    fn remove_owned_networks(&self) -> bool {
        if !matches!(self.profile, Profile::NativeProvider { .. }) {
            return true;
        }
        let endpoint = format!("unix://{}", self.socket.display());
        let execute = |args: &[&str]| {
            std::process::Command::new("timeout")
                .args(["10", "docker", "--host", &endpoint])
                .args(args)
                .output()
        };
        let Ok(containers) = execute(&["ps", "--all", "--quiet", "--no-trunc"]) else {
            return false;
        };
        if !containers.status.success() {
            return false;
        }
        for id in String::from_utf8_lossy(&containers.stdout).split_whitespace() {
            if id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !execute(&["rm", "--force", id]).is_ok_and(|r| r.status.success())
            {
                return false;
            }
        }
        let Ok(networks) = execute(&[
            "network",
            "ls",
            "--filter",
            "type=custom",
            "--quiet",
            "--no-trunc",
        ]) else {
            return false;
        };
        if !networks.status.success() {
            return false;
        }
        for id in String::from_utf8_lossy(&networks.stdout).split_whitespace() {
            if id.len() != 64
                || !id.bytes().all(|b| b.is_ascii_hexdigit())
                || !execute(&["network", "rm", id]).is_ok_and(|r| r.status.success())
            {
                return false;
            }
        }
        true
    }
}
pub async fn bounded(command: &mut Command) -> Output {
    command.kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("fixture command deadline")
        .unwrap()
}
pub async fn checked(command: &mut Command) -> String {
    let output = bounded(command).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
impl Drop for DockerDaemon {
    fn drop(&mut self) {
        if !self.finished {
            if let Ok(logs) = std::process::Command::new("timeout")
                .args(["5", "docker", "--host", HOST, "logs", &self.name])
                .output()
            {
                let _ = std::fs::write(self.diagnostics.join("docker-daemon.log"), logs.stderr);
            }
            self.remove_owned_networks();
            let _ = std::process::Command::new("timeout")
                .args([
                    "10",
                    "docker",
                    "--host",
                    HOST,
                    "rm",
                    "--force",
                    "--volumes",
                    &self.name,
                ])
                .output();
            self.remove_files();
        }
    }
}
