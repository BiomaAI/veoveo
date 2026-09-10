//! Temporary native-fixture relay for the existing localhost registry. The
//! second daemon keeps its own network namespace, including bridge/firewall state.
use super::{DockerDaemon, bounded, checked, host};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::{TcpListener, TcpStream, UnixListener, UnixStream},
    sync::Semaphore,
};

pub const CHILD_ENV: &str = "VEOVEO_STORAGE_REGISTRY_PROXY";
async fn pipe(a: impl AsyncRead + AsyncWrite + Unpin, b: impl AsyncRead + AsyncWrite + Unpin) {
    let mut a = a;
    let mut b = b;
    let _ = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::io::copy_bidirectional_with_sizes(&mut a, &mut b, 65536, 65536),
    )
    .await;
}
#[allow(dead_code)] // Only the provider fixture reexecutes this entrypoint.
pub async fn child() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:5001").await?;
    let slots = Arc::new(Semaphore::new(16));
    let mut sessions = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (tcp, _) = accepted?;
                let Ok(permit) = slots.clone().try_acquire_owned() else { continue; };
                sessions.spawn(async move {
                    let _permit = permit;
                    if let Ok(unix) = UnixStream::connect("/registry.sock").await { pipe(tcp, unix).await; }
                });
            }
            _ = sessions.join_next(), if !sessions.is_empty() => {}
        }
    }
}
struct Relay {
    socket: PathBuf,
    container: Option<String>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Relay {
    fn drop(&mut self) {
        if let Some(name) = &self.container {
            let _ = std::process::Command::new("timeout")
                .args(["5", "docker", "--host", super::HOST, "rm", "--force", name])
                .output();
        }
        self.server.abort();
        let _ = std::fs::remove_file(&self.socket);
    }
}
pub async fn pull(daemon: &DockerDaemon, image: &str, test_name: &'static str) {
    assert!(image.starts_with("localhost:5001/") && image.contains("@sha256:"));
    let socket = daemon.root.join("run/registry.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let server = tokio::spawn(async move {
        let slots = Arc::new(Semaphore::new(16));
        let mut sessions = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let Ok((unix, _)) = accepted else { break; };
                    let Ok(permit) = slots.clone().try_acquire_owned() else { continue; };
                    sessions.spawn(async move {
                        let _permit = permit;
                        if let Ok(Ok(tcp)) = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect("127.0.0.1:5001")).await { pipe(unix, tcp).await; }
                    });
                }
                _ = sessions.join_next(), if !sessions.is_empty() => {}
            }
        }
    });
    let name = format!("{}-registry", daemon.name);
    let mut relay = Relay {
        socket,
        container: Some(name.clone()),
        server,
    };
    let uid = checked(tokio::process::Command::new("id").arg("-u")).await;
    let gid = checked(tokio::process::Command::new("id").arg("-g")).await;
    checked(
        host()
            .args([
                "run",
                "--detach",
                "--pull",
                "never",
                "--name",
                &name,
                "--network",
                &format!("container:{}", daemon.name),
                "--user",
                &format!("{uid}:{gid}"),
                "--read-only",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--pids-limit",
                "64",
                "--memory",
                "256m",
                "--cpus",
                "1",
                "--mount",
            ])
            .arg(format!(
                "type=bind,source={},target=/registry.sock,readonly",
                relay.socket.display()
            ))
            .arg("--mount")
            .arg(format!(
                "type=bind,source={},target=/registry-relay,readonly",
                std::env::current_exe().unwrap().display()
            ))
            .args([
                "--env",
                &format!("{CHILD_ENV}=1"),
                "--entrypoint",
                "/registry-relay",
                image,
                "--exact",
                test_name,
                "--ignored",
                "--nocapture",
            ]),
    )
    .await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
    loop {
        let ready = bounded(host().args([
            "exec",
            &daemon.name,
            "timeout",
            "2",
            "wget",
            "-q",
            "-O",
            "/dev/null",
            "http://127.0.0.1:5001/v2/",
        ]))
        .await;
        if ready.status.success() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            let logs = bounded(host().args(["logs", &name])).await;
            std::fs::write(daemon.diagnostics.join("registry-relay.log"), logs.stderr).unwrap();
            panic!(
                "isolated registry relay readiness; diagnostics at {}",
                daemon.diagnostics.display()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    checked(daemon.command().args(["image", "pull", image])).await;
    checked(host().args(["rm", "--force", &name])).await;
    relay.container = None;
}
