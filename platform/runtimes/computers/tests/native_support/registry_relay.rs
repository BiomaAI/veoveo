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
const PORT_ENV: &str = "VEOVEO_STORAGE_REGISTRY_PORT";

/// The owned local registry supplies exact published bytes. An enrolled alias is
/// resolved only inside the disposable daemon, preserving the tested image URI.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct Registry {
    pub authority: String,
    pub host: String,
    pub port: u16,
}
impl Registry {
    pub fn for_image(image: &str) -> Self {
        let (authority, path) = image.split_once('/').expect("explicit image registry");
        let (_, digest) = path
            .split_once("@sha256:")
            .expect("digest-pinned fixture image");
        assert!(digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()));
        let url = reqwest::Url::parse(&format!("http://{authority}"))
            .expect("fixture registry authority");
        let host = url.host_str().expect("registry host").to_owned();
        let port = url.port().expect("explicit unprivileged registry port");
        assert!(port >= 1024 && !host.contains(':'));
        assert_eq!(authority, format!("{host}:{port}"));
        Self {
            authority: authority.into(),
            host,
            port,
        }
    }
}
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
    let port: u16 = std::env::var(PORT_ENV)
        .expect("enrolled relay port")
        .parse()
        .expect("port");
    assert!(port >= 1024);
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
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
    let registry = Registry::for_image(image);
    assert!(
        daemon.registries.contains(&registry),
        "registry must be enrolled before daemon startup"
    );
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
                "--env",
                &format!("{PORT_ENV}={}", registry.port),
                "--entrypoint",
                "/registry-relay",
                &daemon.image_id,
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
            &format!("http://127.0.0.1:{}/v2/", registry.port),
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
