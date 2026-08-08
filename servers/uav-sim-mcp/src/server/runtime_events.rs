use std::{
    fs,
    os::unix::fs::FileTypeExt as _,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context as _, ensure};
use serde::Deserialize;
use tokio::net::UnixDatagram;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{LiveSessionId, SubscriptionHub};

use crate::uris;

const RUNTIME_EVENT_SCHEMA: &str = "veoveo.io/uav-runtime-event/v1";

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RuntimeEvent {
    schema: String,
    event: RuntimeEventKind,
    session_id: LiveSessionId,
    generation: u64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RuntimeEventKind {
    Ready,
}

pub(super) struct RuntimeEventListener {
    socket: UnixDatagram,
    path: PathBuf,
    session_id: LiveSessionId,
}

impl RuntimeEventListener {
    pub(super) fn bind(path: &Path, session_id: LiveSessionId) -> anyhow::Result<Self> {
        validate_path(path)?;
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_socket(),
                    "runtime event socket path exists and is not a Unix socket: {}",
                    path.display()
                );
                fs::remove_file(path).with_context(|| {
                    format!("removing stale runtime event socket {}", path.display())
                })?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("inspecting runtime event socket {}", path.display())
                });
            }
        }
        let socket = UnixDatagram::bind(path)
            .with_context(|| format!("binding runtime event socket {}", path.display()))?;
        Ok(Self {
            socket,
            path: path.to_owned(),
            session_id,
        })
    }

    pub(super) async fn run(self, subscribers: Arc<SubscriptionHub>, shutdown: CancellationToken) {
        let mut buffer = [0_u8; 1_024];
        loop {
            let received = tokio::select! {
                () = shutdown.cancelled() => break,
                received = self.socket.recv(&mut buffer) => received,
            };
            let length = match received {
                Ok(length) => length,
                Err(error) => {
                    tracing::error!(%error, "pod-local runtime event receive failed");
                    break;
                }
            };
            match parse(&buffer[..length], &self.session_id) {
                Ok(event) => {
                    tracing::info!(
                        session_id = %event.session_id,
                        generation = event.generation,
                        "authoritative simulator reported runtime readiness"
                    );
                    subscribers
                        .notify_resource_updated(uris::live_cameras(&event.session_id))
                        .await;
                }
                Err(error) => {
                    tracing::warn!(%error, "rejected malformed pod-local runtime event");
                }
            }
        }
        if let Err(error) = fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %self.path.display(), "failed to remove runtime event socket");
        }
    }
}

fn validate_path(path: &Path) -> anyhow::Result<()> {
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            && path.file_name().and_then(|name| name.to_str()) == Some("runtime-events.sock"),
        "runtime event socket must be an absolute normalized runtime-events.sock path"
    );
    ensure!(
        path.parent().is_some_and(Path::is_dir),
        "runtime event socket directory does not exist: {}",
        path.display()
    );
    Ok(())
}

fn parse(bytes: &[u8], expected_session: &LiveSessionId) -> anyhow::Result<RuntimeEvent> {
    let event: RuntimeEvent = serde_json::from_slice(bytes).context("decoding runtime event")?;
    ensure!(
        event.schema == RUNTIME_EVENT_SCHEMA,
        "unknown runtime event schema"
    );
    ensure!(
        event.session_id == *expected_session,
        "runtime event session mismatch"
    );
    ensure!(
        event.generation > 0,
        "runtime event generation must be positive"
    );
    Ok(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_ready_event_is_strict_and_session_bound() {
        let expected = LiveSessionId::new("session-alpha").unwrap();
        let event = parse(
            br#"{"schema":"veoveo.io/uav-runtime-event/v1","event":"ready","sessionId":"session-alpha","generation":2}"#,
            &expected,
        )
        .unwrap();
        assert_eq!(event.event, RuntimeEventKind::Ready);
        assert_eq!(event.generation, 2);
        assert!(
            parse(
                br#"{"schema":"veoveo.io/uav-runtime-event/v1","event":"ready","sessionId":"session-beta","generation":2}"#,
                &expected,
            )
            .is_err()
        );
        assert!(
            parse(
                br#"{"schema":"veoveo.io/uav-runtime-event/v1","event":"ready","sessionId":"session-alpha","generation":2,"secret":"no"}"#,
                &expected,
            )
            .is_err()
        );
    }
}
