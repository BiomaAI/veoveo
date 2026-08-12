use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context as _, ensure};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{LiveSessionId, SubscriptionHub};

use crate::{adapter::Adapter, uris};

const RUNTIME_EVENT_SCHEMA: &str = "veoveo.io/uav-runtime-event/v2";
const MAXIMUM_EVENT_BYTES: usize = 1_024;

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
    AdapterReady,
    Ready,
}

pub(super) struct RuntimeEventListener {
    session_id: LiveSessionId,
    world_bootstrap_file: Option<PathBuf>,
    adapter: Arc<Adapter>,
}

impl RuntimeEventListener {
    pub(super) fn new(
        session_id: LiveSessionId,
        world_bootstrap_file: Option<PathBuf>,
        adapter: Arc<Adapter>,
    ) -> Self {
        Self {
            session_id,
            world_bootstrap_file,
            adapter,
        }
    }

    pub(super) async fn run(self, subscribers: Arc<SubscriptionHub>, shutdown: CancellationToken) {
        let mut reconnect_delay = Duration::from_millis(250);
        while !shutdown.is_cancelled() {
            let result = self
                .consume_connection(subscribers.clone(), shutdown.child_token())
                .await;
            if shutdown.is_cancelled() {
                break;
            }
            match result {
                Ok(()) => reconnect_delay = Duration::from_millis(250),
                Err(error) => {
                    tracing::warn!(%error, "simulator runtime event stream disconnected");
                }
            }
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = tokio::time::sleep(reconnect_delay) => {}
            }
            reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(5));
        }
    }

    async fn consume_connection(
        &self,
        subscribers: Arc<SubscriptionHub>,
        shutdown: CancellationToken,
    ) -> anyhow::Result<()> {
        let Adapter::Http(adapter) = self.adapter.as_ref() else {
            return Ok(());
        };
        let mut response = adapter.runtime_events().await?;
        let mut buffered = Vec::new();
        loop {
            let chunk = tokio::select! {
                () = shutdown.cancelled() => return Ok(()),
                chunk = response.chunk() => chunk.context("reading simulator runtime event stream")?,
            };
            let Some(chunk) = chunk else {
                anyhow::bail!("simulator runtime event stream ended");
            };
            buffered.extend_from_slice(&chunk);
            ensure!(
                buffered.len() <= MAXIMUM_EVENT_BYTES,
                "simulator runtime event exceeds {MAXIMUM_EVENT_BYTES} bytes"
            );
            while let Some(newline) = buffered.iter().position(|byte| *byte == b'\n') {
                let line = buffered.drain(..=newline).collect::<Vec<_>>();
                let line = &line[..line.len() - 1];
                if line.is_empty() {
                    continue;
                }
                let event = parse(line, &self.session_id)?;
                self.apply(event, subscribers.clone()).await;
            }
        }
    }

    async fn apply(&self, event: RuntimeEvent, subscribers: Arc<SubscriptionHub>) {
        match event.event {
            RuntimeEventKind::AdapterReady => {
                tracing::info!(
                    session_id = %event.session_id,
                    generation = event.generation,
                    "authoritative simulator adapter accepts world configuration"
                );
                if let Some(path) = self.world_bootstrap_file.as_deref()
                    && let Err(error) = super::world_bootstrap::apply(path, &self.adapter).await
                {
                    tracing::error!(
                        %error,
                        session_id = %event.session_id,
                        generation = event.generation,
                        "installation world binding reapplication failed"
                    );
                }
            }
            RuntimeEventKind::Ready => {
                tracing::info!(
                    session_id = %event.session_id,
                    generation = event.generation,
                    "authoritative simulator reported runtime readiness"
                );
                subscribers
                    .notify_resource_updated(uris::live_cameras(&event.session_id))
                    .await;
            }
        }
    }
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
            br#"{"schema":"veoveo.io/uav-runtime-event/v2","event":"ready","sessionId":"session-alpha","generation":2}"#,
            &expected,
        )
        .unwrap();
        assert_eq!(event.event, RuntimeEventKind::Ready);
        assert_eq!(
            parse(
                br#"{"schema":"veoveo.io/uav-runtime-event/v2","event":"adapter_ready","sessionId":"session-alpha","generation":2}"#,
                &expected,
            )
            .unwrap()
            .event,
            RuntimeEventKind::AdapterReady
        );
        assert_eq!(event.generation, 2);
        assert!(
            parse(
                br#"{"schema":"veoveo.io/uav-runtime-event/v2","event":"ready","sessionId":"session-beta","generation":2}"#,
                &expected,
            )
            .is_err()
        );
        assert!(
            parse(
                br#"{"schema":"veoveo.io/uav-runtime-event/v2","event":"ready","sessionId":"session-alpha","generation":2,"secret":"no"}"#,
                &expected,
            )
            .is_err()
        );
    }
}
