//! Durable wake scheduling.
//!
//! Producers commit wakes to SurrealDB before issuing a best-effort local
//! hint. The receiver always claims from the durable queue, so a full channel,
//! restart, or disconnected LIVE stream cannot lose accepted work.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{TimeDelta, Utc};
use tokio::sync::mpsc;
use veoveo_agent_runtime::{AgentRuntime, ClaimedWake, NewWake};
use veoveo_platform_store::{OpenObject, WakeId, WakeKind};

pub fn resource_updated(uri: &str) -> NewWake {
    NewWake::now(
        WakeKind::ResourceChanged,
        Some(format!("resource:{uri}")),
        payload([("uri", serde_json::json!(uri))]),
    )
}

pub fn timer(name: &str) -> NewWake {
    NewWake::now(
        WakeKind::Timer,
        Some(format!("timer:{name}")),
        payload([("name", serde_json::json!(name))]),
    )
}

pub fn heartbeat() -> NewWake {
    NewWake::now(
        WakeKind::Timer,
        Some("heartbeat".to_owned()),
        payload([
            ("name", serde_json::json!("heartbeat")),
            ("timer_kind", serde_json::json!("heartbeat")),
        ]),
    )
}

pub fn operator_message(text: &str) -> NewWake {
    let wake = NewWake::now(
        WakeKind::OperatorMessage,
        None,
        payload([("text", serde_json::json!(text))]),
    );
    NewWake {
        dedupe_key: Some(format!("operator:{}", wake.wake_id)),
        ..wake
    }
}

fn payload<const N: usize>(entries: [(&str, serde_json::Value); N]) -> OpenObject {
    OpenObject::new(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

pub trait WakeKindExt {
    fn coalescible(self) -> bool;
    fn note_name(self) -> &'static str;
}

impl WakeKindExt for WakeKind {
    fn coalescible(self) -> bool {
        matches!(self, WakeKind::ResourceChanged | WakeKind::Timer)
    }

    fn note_name(self) -> &'static str {
        match self {
            WakeKind::TaskResult => "task_result",
            WakeKind::ResourceChanged => "resource_changed",
            WakeKind::Timer => "timer",
            WakeKind::OperatorMessage => "operator_message",
            WakeKind::Elicitation => "elicitation",
        }
    }
}

pub fn is_priority(wake: &ClaimedWake) -> bool {
    wake.kind == WakeKind::OperatorMessage
        || (wake.kind == WakeKind::Elicitation
            && wake
                .payload
                .as_map()
                .get("phase")
                .and_then(serde_json::Value::as_str)
                == Some("answered"))
}

#[derive(Clone)]
pub struct WakeBus {
    runtime: AgentRuntime,
    tx: mpsc::Sender<WakeId>,
}

impl WakeBus {
    pub fn channel(runtime: AgentRuntime, capacity: usize) -> (Self, mpsc::Receiver<WakeId>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { runtime, tx }, rx)
    }

    pub async fn send(&self, wake: NewWake) -> Result<WakeId> {
        let wake_id = self.runtime.enqueue_wake(wake).await?;
        self.hint(wake_id);
        Ok(wake_id)
    }

    /// Notify the receiver about a wake already committed by another runtime
    /// transaction, such as task-result settlement.
    pub fn hint(&self, wake_id: WakeId) {
        if let Err(error) = self.tx.try_send(wake_id) {
            tracing::debug!(%wake_id, %error, "wake hint dropped; durable scan will recover it");
        }
    }
}

#[derive(Debug)]
pub struct WakeBatch {
    pub wakes: Vec<ClaimedWake>,
}

impl WakeBatch {
    pub fn ids(&self) -> Vec<WakeId> {
        self.wakes.iter().map(|wake| wake.wake_id).collect()
    }
}

pub struct WakeReceiver {
    rx: mpsc::Receiver<WakeId>,
    runtime: AgentRuntime,
    coalesce_window: Duration,
    min_wake_interval: Duration,
    claim_lease: Duration,
    last_episode_finished: Option<Instant>,
}

impl WakeReceiver {
    pub fn new(
        rx: mpsc::Receiver<WakeId>,
        runtime: AgentRuntime,
        coalesce_window: Duration,
        min_wake_interval: Duration,
        claim_lease: Duration,
    ) -> Self {
        Self {
            rx,
            runtime,
            coalesce_window,
            min_wake_interval,
            claim_lease,
            last_episode_finished: None,
        }
    }

    pub fn note_episode_finished(&mut self) {
        self.last_episode_finished = Some(Instant::now());
    }

    pub async fn next_batch(&mut self) -> Result<WakeBatch> {
        loop {
            let mut claimed = self.runtime.claim_wakes(256, self.claim_lease).await?;
            if claimed.is_empty() {
                tokio::select! {
                    _ = self.rx.recv() => {}
                    () = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
                continue;
            }

            let deadline = tokio::time::Instant::now() + self.coalesce_window;
            while let Ok(Some(_)) = tokio::time::timeout_at(deadline, self.rx.recv()).await {}
            claimed.extend(self.runtime.claim_wakes(256, self.claim_lease).await?);
            let claimed = self.coalesce(claimed).await?;
            if claimed.is_empty() {
                continue;
            }

            if let Some(finished) = self.last_episode_finished
                && !claimed.iter().any(is_priority)
                && finished.elapsed() < self.min_wake_interval
            {
                let wait = self.min_wake_interval - finished.elapsed();
                for wake in claimed {
                    self.runtime
                        .retry_wake(
                            wake.wake_id,
                            Utc::now() + duration_delta(wait)?,
                            "debounced",
                        )
                        .await?;
                }
                tokio::time::sleep(wait).await;
                continue;
            }
            return Ok(WakeBatch { wakes: claimed });
        }
    }

    async fn coalesce(&self, wakes: Vec<ClaimedWake>) -> Result<Vec<ClaimedWake>> {
        let mut kept: Vec<ClaimedWake> = Vec::new();
        for wake in wakes {
            let winner = wake
                .dedupe_key
                .as_deref()
                .filter(|_| wake.kind.coalescible())
                .and_then(|key| {
                    kept.iter()
                        .find(|candidate| candidate.dedupe_key.as_deref() == Some(key))
                });
            if let Some(winner) = winner {
                self.runtime
                    .coalesce_wake(wake.wake_id, winner.wake_id)
                    .await?;
            } else {
                kept.push(wake);
            }
        }
        Ok(kept)
    }

    pub async fn retry_batch(&self, batch: &WakeBatch, error: &str) -> Result<()> {
        self.defer_batch(batch, Duration::from_secs(2), error).await
    }

    pub async fn defer_batch(
        &self,
        batch: &WakeBatch,
        delay: Duration,
        reason: &str,
    ) -> Result<()> {
        let available_at = Utc::now() + duration_delta(delay)?;
        for wake in &batch.wakes {
            self.runtime
                .retry_wake(wake.wake_id, available_at, reason)
                .await?;
        }
        Ok(())
    }
}

fn duration_delta(duration: Duration) -> Result<TimeDelta> {
    Ok(TimeDelta::from_std(duration)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_wake_ids_are_uuid_v7() {
        assert_eq!(
            operator_message("inspect")
                .wake_id
                .as_uuid()
                .get_version_num(),
            7
        );
        assert_eq!(heartbeat().wake_id.as_uuid().get_version_num(), 7);
    }

    #[test]
    fn only_noise_is_coalescible() {
        assert!(WakeKind::Timer.coalescible());
        assert!(WakeKind::ResourceChanged.coalescible());
        assert!(!WakeKind::TaskResult.coalescible());
        assert!(!WakeKind::OperatorMessage.coalescible());
    }
}
