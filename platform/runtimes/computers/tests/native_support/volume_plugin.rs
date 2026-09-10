//! Isolated Docker volume API probe. No installed volume or daemon is restarted.
#[path = "docker_daemon.rs"]
mod docker_daemon;
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, OriginalUri, State},
    routing::post,
};
use docker_daemon::{DockerDaemon, bounded, checked};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::net::UnixListener;
use uuid::Uuid;

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Request {
    #[serde(default)]
    name: String,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Reply {
    err: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mountpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<Volume>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volumes: Option<Vec<Volume>>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Volume {
    name: String,
    mountpoint: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Consumer {
    id: String,
    labels: BTreeMap<String, String>,
}
#[derive(Serialize)]
struct Activation {
    #[serde(rename = "Implements")]
    implements: [&'static str; 1],
}
#[derive(Serialize)]
struct Capabilities {
    #[serde(rename = "Capabilities")]
    capabilities: Scope,
}
#[derive(Serialize)]
struct Scope {
    #[serde(rename = "Scope")]
    scope: &'static str,
}

struct Plugin {
    volume: Volume,
    computer: String,
    docker: reqwest::Client,
}
impl Plugin {
    async fn mount_allowed(&self) -> bool {
        let filters =
            serde_json::to_string(&BTreeMap::from([("volume", [&self.volume.name])])).unwrap();
        let result = self
            .docker
            .get("http://localhost/v1.53/containers/json")
            .query(&[("all", "1"), ("filters", &filters)])
            .send()
            .await;
        let Ok(response) = result else { return false };
        let Ok(response) = response.error_for_status() else {
            return false;
        };
        let Ok(consumers) = response.json::<Vec<Consumer>>().await else {
            return false;
        };
        consumers.len() == 1
            && consumers[0].id.len() == 64
            && consumers[0].labels.get("veoveo-computer") == Some(&self.computer)
    }
}
async fn dispatch(
    State(plugin): State<Arc<Plugin>>,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Json<Reply> {
    let Ok(request) = serde_json::from_slice::<Request>(&body) else {
        return Json(Reply {
            err: "invalid Docker volume request".into(),
            ..Reply::default()
        });
    };
    let mut reply = Reply::default();
    if uri.path() == "/VolumeDriver.List" {
        reply.volumes = Some(vec![plugin.volume.clone()]);
    } else if request.name != plugin.volume.name {
        reply.err = "unknown isolated volume".into();
    } else {
        match uri.path() {
            "/VolumeDriver.Create" | "/VolumeDriver.Remove" | "/VolumeDriver.Unmount" => {}
            "/VolumeDriver.Get" => reply.volume = Some(plugin.volume.clone()),
            "/VolumeDriver.Path" => reply.mountpoint = Some(plugin.volume.mountpoint.clone()),
            "/VolumeDriver.Mount" => {
                if plugin.mount_allowed().await {
                    reply.mountpoint = Some(plugin.volume.mountpoint.clone());
                } else {
                    reply.err = "mount requires one registered Computer container".into();
                }
            }
            _ => reply.err = "unsupported volume method".into(),
        }
    }
    Json(reply)
}

pub struct VolumeFixture {
    image: String,
    daemon: Option<DockerDaemon>,
    name: String,
    computer: String,
    dir: PathBuf,
    socket_dir: PathBuf,
    server: tokio::task::JoinHandle<()>,
    finished: bool,
}
impl VolumeFixture {
    pub async fn start() -> Self {
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE").expect("pinned Computer image");
        assert!(image.contains("@sha256:"));
        let uuid = Uuid::now_v7();
        let name = format!("veoveo-volume-probe-{}", uuid.simple());
        let root = PathBuf::from(
            std::env::var("VEOVEO_COMPUTERS_NATIVE_OUTPUT").expect("owned diagnostic root"),
        );
        let dir = root.join(&name);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        let socket_dir = std::env::temp_dir().join(format!("vv-volume-{}", uuid.simple()));
        std::fs::create_dir(&socket_dir).unwrap();
        let daemon = DockerDaemon::start(&dir, &socket_dir, &image).await;
        let image = daemon.image_id.clone();
        let host_dir = dir.to_str().unwrap();
        let listener = UnixListener::bind(socket_dir.join("volume.sock")).unwrap();
        let plugin = Arc::new(Plugin {
            volume: Volume {
                name: name.clone(),
                mountpoint: host_dir.into(),
            },
            computer: uuid.to_string(),
            docker: reqwest::Client::builder()
                .unix_socket(daemon.socket.clone())
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
        });
        let mut router = Router::new()
            .route(
                "/Plugin.Activate",
                post(|| async {
                    Json(Activation {
                        implements: ["VolumeDriver"],
                    })
                }),
            )
            .route(
                "/VolumeDriver.Capabilities",
                post(|| async {
                    Json(Capabilities {
                        capabilities: Scope { scope: "local" },
                    })
                }),
            );
        for method in [
            "Create", "Remove", "Mount", "Unmount", "Path", "Get", "List",
        ] {
            router = router.route(&format!("/VolumeDriver.{method}"), post(dispatch));
        }
        let router = router.layer(DefaultBodyLimit::max(4096)).with_state(plugin);
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let fixture = Self {
            image,
            daemon: Some(daemon),
            name,
            computer: uuid.to_string(),
            dir,
            socket_dir,
            server,
            finished: false,
        };
        let address = format!("unix://{}/volume.sock", fixture.socket_dir.display());
        fixture
            .daemon
            .as_ref()
            .unwrap()
            .register(&fixture.name, &address, &fixture.dir.join("home"))
            .await;
        checked(fixture.docker().args([
            "volume",
            "create",
            "--driver",
            &fixture.name,
            &fixture.name,
        ]))
        .await;
        println!("Volume probe diagnostics: {}", fixture.dir.display());
        fixture
    }
    fn docker(&self) -> tokio::process::Command {
        self.daemon.as_ref().unwrap().command()
    }
    fn container(&self, suffix: &str) -> String {
        format!("{}-{suffix}", self.name)
    }
    pub async fn create(&self, suffix: &str, no_copy: bool) {
        let mut mount = format!(
            "type=volume,source={},target=/probe,volume-subpath=home",
            self.name
        );
        if no_copy {
            mount.push_str(",volume-nocopy");
        }
        checked(
            self.docker()
                .args([
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
                    "--label",
                ])
                .arg(format!("veoveo-computer={}", self.computer))
                .args(["--mount", &mount, &self.image, "/bin/sleep", "infinity"]),
        )
        .await;
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
                    .contains("mount requires one registered Computer container")
            );
        }
    }
    pub async fn stop_container(&self, suffix: &str) {
        checked(
            self.docker()
                .args(["stop", "--time", "1", &self.container(suffix)]),
        )
        .await;
    }
    pub async fn remove_container(&self, suffix: &str) {
        checked(
            self.docker()
                .args(["rm", "--force", &self.container(suffix)]),
        )
        .await;
    }
    pub async fn write(&self, suffix: &str, value: &str) {
        checked(self.docker().args([
            "exec",
            &self.container(suffix),
            "/bin/sh",
            "-c",
            "printf '%s' \"$1\" > /probe/value",
            "fixture",
            value,
        ]))
        .await;
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
    pub async fn copy_from(&self, suffix: &str) {
        checked(
            self.docker()
                .args(["cp", &format!("{}:/probe/value", self.container(suffix))])
                .arg(self.dir.join("copied")),
        )
        .await;
        assert_eq!(
            std::fs::read_to_string(self.dir.join("copied")).unwrap(),
            "first"
        );
    }
    pub async fn finish(mut self) {
        self.remove_container("b").await;
        checked(self.docker().args(["volume", "rm", &self.name])).await;
        self.server.abort();
        self.daemon.take().unwrap().finish().await;
        std::fs::remove_dir_all(&self.socket_dir).unwrap();
        self.finished = true;
    }
}
impl Drop for VolumeFixture {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        self.server.abort();
        drop(self.daemon.take());
        let _ = std::fs::remove_dir_all(&self.socket_dir);
    }
}
