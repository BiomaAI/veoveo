//! The wake bus: everything that can start an episode flows through here.
//!
//! Producers (task watchers, the gateway notification delegate, the heartbeat
//! timer, the operator endpoint, the elicitation handler) send typed wakes
//! into one bounded channel. The consumer batches them: drain a coalescing
//! window, collapse duplicates by `(kind, dedup_key)`, drop wakes the ledger
//! proves stale, and debounce low-priority noise between episodes. Every wake
//! is persisted with its disposition, so the ledger explains why each episode
//! ran.

use std::time::{Duration, Instant};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::ledger::{KernelLedger, WakeState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeKind {
    TaskSettled,
    ResourceUpdated,
    Timer,
    Heartbeat,
    Operator,
    ElicitationPending,
    ElicitationAnswered,
}

impl WakeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            WakeKind::TaskSettled => "task_settled",
            WakeKind::ResourceUpdated => "resource_updated",
            WakeKind::Timer => "timer",
            WakeKind::Heartbeat => "heartbeat",
            WakeKind::Operator => "operator",
            WakeKind::ElicitationPending => "elicitation_pending",
            WakeKind::ElicitationAnswered => "elicitation_answered",
        }
    }

    /// Coalescible wakes may be dropped under backpressure and collapse with
    /// duplicates inside a batch window.
    pub const fn coalescible(self) -> bool {
        matches!(
            self,
            WakeKind::ResourceUpdated | WakeKind::Timer | WakeKind::Heartbeat
        )
    }

    /// Priority wakes bypass the inter-episode debounce.
    pub const fn priority(self) -> bool {
        matches!(self, WakeKind::Operator | WakeKind::ElicitationAnswered)
    }
}

#[derive(Debug, Clone)]
pub struct WakeEvent {
    pub wake_id: Uuid,
    pub kind: WakeKind,
    pub dedup_key: String,
    pub payload: serde_json::Value,
}

impl WakeEvent {
    fn new(kind: WakeKind, dedup_key: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            wake_id: Uuid::new_v4(),
            kind,
            dedup_key: dedup_key.into(),
            payload,
        }
    }

    pub fn task_settled(task_id: &str) -> Self {
        Self::new(
            WakeKind::TaskSettled,
            format!("task:{task_id}"),
            serde_json::json!({ "task_id": task_id }),
        )
    }

    pub fn resource_updated(uri: &str) -> Self {
        Self::new(
            WakeKind::ResourceUpdated,
            format!("resource:{uri}"),
            serde_json::json!({ "uri": uri }),
        )
    }

    pub fn timer(name: &str) -> Self {
        Self::new(
            WakeKind::Timer,
            format!("timer:{name}"),
            serde_json::json!({ "name": name }),
        )
    }

    pub fn heartbeat() -> Self {
        Self::new(WakeKind::Heartbeat, "heartbeat", serde_json::Value::Null)
    }

    pub fn operator(text: &str) -> Self {
        let wake_id = Uuid::new_v4();
        Self {
            wake_id,
            kind: WakeKind::Operator,
            dedup_key: format!("operator:{wake_id}"),
            payload: serde_json::json!({ "text": text }),
        }
    }

    pub fn elicitation_pending(elicitation_id: Uuid) -> Self {
        Self::new(
            WakeKind::ElicitationPending,
            format!("elicitation:{elicitation_id}"),
            serde_json::json!({ "elicitation_id": elicitation_id }),
        )
    }

    pub fn elicitation_answered(elicitation_id: Uuid) -> Self {
        Self::new(
            WakeKind::ElicitationAnswered,
            format!("elicitation:{elicitation_id}:answered"),
            serde_json::json!({ "elicitation_id": elicitation_id }),
        )
    }
}

/// Cloneable producer handle.
#[derive(Clone)]
pub struct WakeBus {
    tx: mpsc::Sender<WakeEvent>,
}

impl WakeBus {
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<WakeEvent>) {
        let (tx, rx) = mpsc::channel(capacity);
        (Self { tx }, rx)
    }

    /// Deliver a wake. Coalescible kinds are dropped under backpressure —
    /// they recur naturally; everything else waits for channel space.
    pub async fn send(&self, event: WakeEvent) {
        if event.kind.coalescible() {
            if let Err(err) = self.tx.try_send(event) {
                tracing::warn!(%err, "coalescible wake dropped under backpressure");
            }
        } else if self.tx.send(event).await.is_err() {
            tracing::warn!("wake bus receiver is gone");
        }
    }
}

/// One scheduler round: the wakes an episode should answer.
#[derive(Debug)]
pub struct WakeBatch {
    pub wakes: Vec<WakeEvent>,
}

pub struct WakeReceiver {
    rx: mpsc::Receiver<WakeEvent>,
    ledger: KernelLedger,
    coalesce_window: Duration,
    min_wake_interval: Duration,
    last_episode_finished: Option<Instant>,
}

impl WakeReceiver {
    pub fn new(
        rx: mpsc::Receiver<WakeEvent>,
        ledger: KernelLedger,
        coalesce_window: Duration,
        min_wake_interval: Duration,
    ) -> Self {
        Self {
            rx,
            ledger,
            coalesce_window,
            min_wake_interval,
            last_episode_finished: None,
        }
    }

    pub fn note_episode_finished(&mut self) {
        self.last_episode_finished = Some(Instant::now());
    }

    /// Receive the next batch, or `None` when every producer is gone.
    pub async fn next_batch(&mut self) -> Option<WakeBatch> {
        let mut pending: Vec<WakeEvent> = Vec::new();
        loop {
            if pending.is_empty() {
                pending.push(self.rx.recv().await?);
            }
            // Drain the coalescing window so a burst becomes one episode.
            let deadline = tokio::time::Instant::now() + self.coalesce_window;
            while let Ok(Some(event)) = tokio::time::timeout_at(deadline, self.rx.recv()).await {
                pending.push(event);
            }

            let batch = self.settle(std::mem::take(&mut pending));
            if batch.is_empty() {
                continue;
            }

            // Debounce: low-priority wakes wait out the inter-episode gap,
            // continuing to absorb new arrivals while they do.
            if let Some(finished) = self.last_episode_finished
                && !batch.iter().any(|event| event.kind.priority())
            {
                let elapsed = finished.elapsed();
                if elapsed < self.min_wake_interval {
                    let wait = self.min_wake_interval - elapsed;
                    pending = batch;
                    let deadline = tokio::time::Instant::now() + wait;
                    while let Ok(Some(event)) =
                        tokio::time::timeout_at(deadline, self.rx.recv()).await
                    {
                        pending.push(event);
                    }
                    // Re-settle: the extra arrivals may duplicate or outrank.
                    pending = self.settle_pre_deduped(pending);
                    continue;
                }
            }
            return Some(WakeBatch { wakes: batch });
        }
    }

    /// Dedup a raw drain, persist dispositions, and drop stale wakes.
    fn settle(&self, raw: Vec<WakeEvent>) -> Vec<WakeEvent> {
        let mut kept: Vec<WakeEvent> = Vec::new();
        for event in raw {
            let duplicate = kept
                .iter()
                .any(|existing| existing.dedup_key == event.dedup_key);
            let disposition = if duplicate {
                WakeState::Coalesced
            } else if self.is_stale(&event) {
                WakeState::Dropped
            } else {
                WakeState::Queued
            };
            if let Err(err) = self.ledger.record_wake(
                event.wake_id,
                event.kind.as_str(),
                &event.dedup_key,
                &event.payload,
                disposition,
            ) {
                tracing::error!(%err, "recording wake failed");
            }
            if disposition == WakeState::Queued {
                kept.push(event);
            }
        }
        kept
    }

    /// Re-settle a batch that already has ledger rows plus fresh arrivals.
    fn settle_pre_deduped(&self, raw: Vec<WakeEvent>) -> Vec<WakeEvent> {
        let mut kept: Vec<WakeEvent> = Vec::new();
        for event in raw {
            let duplicate = kept
                .iter()
                .any(|existing| existing.dedup_key == event.dedup_key);
            if duplicate {
                let _ = self
                    .ledger
                    .mark_wake(event.wake_id, WakeState::Coalesced, None);
            } else {
                kept.push(event);
            }
        }
        kept
    }

    /// A task wake whose task was already consumed proves a double signal.
    fn is_stale(&self, event: &WakeEvent) -> bool {
        if event.kind != WakeKind::TaskSettled {
            return false;
        }
        let Some(task_id) = event.payload.get("task_id").and_then(|id| id.as_str()) else {
            return true;
        };
        match self.ledger.task_consumed(task_id) {
            Ok(consumed) => consumed,
            Err(_) => false,
        }
    }
}

pub fn mark_batch_handled(
    ledger: &KernelLedger,
    batch: &WakeBatch,
    episode_id: Uuid,
) -> Result<()> {
    for event in &batch.wakes {
        ledger.mark_wake(event.wake_id, WakeState::Handled, Some(episode_id))?;
    }
    Ok(())
}
