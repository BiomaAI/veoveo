//! A disposable daemon owns plugin registration caches as well as its containers.
use std::{
    path::{Path, PathBuf},
    process::Output,
    time::Duration,
};
use tokio::process::Command;
use uuid::Uuid;

// Latest stable Engine release verified against Moby and the official image on
// 2026-09-09. This fixture does not upgrade the installation's Docker daemon.
const IMAGE: &str = "docker.io/library/docker:29.8.0-dind@sha256:77759fdec1efef224ba7110ef7b5b3c6af6164ffaef5441d3beba059bde8b857";

pub struct DockerDaemon {
    pub socket: PathBuf,
    pub image_id: String,
    name: String,
    root: PathBuf,
    diagnostics: PathBuf,
    finished: bool,
}
impl DockerDaemon {
    pub async fn start(
        diagnostics: &Path,
        plugin_socket: &Path,
        computer_image: &str,
        shared_storage: bool,
    ) -> Self {
        let name = format!("veoveo-docker-probe-{}", Uuid::now_v7().simple());
        let root = std::env::temp_dir().join(&name);
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        std::fs::create_dir(root.join("run")).unwrap();
        let socket = root.join("run/docker.sock");
        let image_id = checked(Command::new("docker").args([
            "image",
            "inspect",
            computer_image,
            "--format",
            "{{.Id}}",
        ]))
        .await;
        assert!(image_id.starts_with("sha256:") && image_id.len() == 71);
        let archive = diagnostics
            .parent()
            .unwrap()
            .join(format!("{}.tar", image_id.replace(':', "-")));
        if !archive.exists() {
            let temporary = archive.with_extension(format!("{}.tmp", Uuid::now_v7()));
            checked(
                Command::new("docker")
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
        };
        let mut command = Command::new("docker");
        command.args([
            "run",
            "--detach",
            "--pull",
            "never",
            "--name",
            &daemon.name,
            "--privileged",
            "--network",
            "none",
            "--memory",
            "1g",
            "--cpus",
            "2",
            "--pids-limit",
            "512",
            "--tmpfs",
            "/tmp:rw,exec,mode=1777",
        ]);
        for (source, target) in [
            (daemon.root.join("data"), PathBuf::from("/var/lib/docker")),
            (daemon.root.join("run"), daemon.root.join("run")),
            (diagnostics.to_owned(), diagnostics.to_owned()),
            (plugin_socket.to_owned(), plugin_socket.to_owned()),
        ] {
            let propagation = if shared_storage && source == diagnostics {
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
                "--iptables=false",
                "--ip6tables=false",
                "--ip-forward=false",
                "--ip-masq=false",
                "--storage-driver=overlay2",
            ]);
        checked(&mut command).await;
        let uid = checked(Command::new("id").arg("-u")).await;
        let gid = checked(Command::new("id").arg("-g")).await;
        let ready = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let result = bounded(
                Command::new("docker")
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
            Command::new("docker")
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
        checked(
            daemon
                .command()
                .args(["image", "load", "--input"])
                .arg(archive),
        )
        .await;
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
    pub async fn register(&self, name: &str, address: &str, home: &Path) {
        checked(Command::new("docker").args(["exec", &self.name, "/bin/sh", "-c", "mkdir -p /etc/docker/plugins && (set -C; printf '%s' \"$1\" > \"$2\") && chown 10001:10001 \"$3\"", "fixture", address, &format!("/etc/docker/plugins/{name}.spec")]).arg(home)).await;
    }
    pub async fn finish(mut self) {
        let logs = bounded(Command::new("docker").args(["logs", &self.name])).await;
        std::fs::write(self.diagnostics.join("docker-daemon.log"), logs.stderr).unwrap();
        checked(Command::new("docker").args(["rm", "--force", "--volumes", &self.name])).await;
        assert!(
            self.remove_files(),
            "isolated daemon files were not removed"
        );
        self.finished = true;
    }
    fn remove_files(&self) -> bool {
        let removed = std::process::Command::new("timeout")
            .args([
                "15",
                "docker",
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
                .args(["5", "docker", "logs", &self.name])
                .output()
            {
                let _ = std::fs::write(self.diagnostics.join("docker-daemon.log"), logs.stderr);
            }
            let _ = std::process::Command::new("timeout")
                .args(["10", "docker", "rm", "--force", "--volumes", &self.name])
                .output();
            self.remove_files();
        }
    }
}
