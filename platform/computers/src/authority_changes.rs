//! One wake source per service replica. Notifications invalidate; fresh domain reads authorize.
use crate::{ComputerError, ComputersStore, Result};
use futures::StreamExt;
use std::time::Duration;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;
use veoveo_computers_contract::{ComputerId, ExecutionId, FileTransferId};
use veoveo_platform_store::{
    ChangefeedCursor, ChangefeedDelivery, ComputerChange, PlatformStore, PlatformTable,
};
use veoveo_types::{GatewayRefreshFamilyId, TaskId};

/// Selects invalidations only. Every wake requires a fresh domain authority read.
#[derive(Clone)]
pub enum AuthorityInterest {
    All,
    Attachment {
        computer: ComputerId,
        family: GatewayRefreshFamilyId,
    },
    Command {
        computer: ComputerId,
        task: TaskId,
        execution: ExecutionId,
    },
    File {
        computer: ComputerId,
        task: TaskId,
        transfer: FileTransferId,
    },
}
impl AuthorityInterest {
    fn matches(&self, change: &Change) -> bool {
        match (self, change) {
            (Self::All, _) | (_, Change::Policy) => true,
            (
                Self::Attachment { computer, .. }
                | Self::Command { computer, .. }
                | Self::File { computer, .. },
                Change::Computer(id),
            ) => computer == id,
            (Self::Attachment { family, .. }, Change::Family(id)) => family == id,
            (Self::Command { task, .. } | Self::File { task, .. }, Change::Task(id)) => task == id,
            (Self::Command { execution, .. }, Change::Command(id)) => execution == id,
            (Self::File { transfer, .. }, Change::File(id)) => transfer == id,
            _ => false,
        }
    }
}
impl ComputersStore {
    /// Subscribe before reading the authority baseline. Cloned stores share one
    /// source; dropping the last store stops it. A lost source invalidates every
    /// listener even when reconnection completes before it next runs.
    pub async fn authority_changes(&self) -> Result<AuthorityChanges> {
        self.authority_events
            .get_or_init(|| AccessEvents::start(self.platform.clone()))
            .listen()
            .await
    }
}

#[derive(Clone)]
enum Change {
    Computer(veoveo_computers_contract::ComputerId),
    Family(GatewayRefreshFamilyId),
    Policy,
    Task(TaskId),
    Command(ExecutionId),
    File(FileTransferId),
}
pub(crate) struct AccessEvents {
    changes: broadcast::Sender<Change>,
    _stop: watch::Sender<()>,
    connected: watch::Receiver<Option<Uuid>>,
}
pub struct AuthorityChanges {
    epoch: Uuid,
    changes: broadcast::Receiver<Change>,
    connected: watch::Receiver<Option<Uuid>>,
}
impl AccessEvents {
    fn start(platform: PlatformStore) -> Self {
        let (stop, mut stopped) = watch::channel(());
        let (changes, _) = broadcast::channel(128);
        let (connected, receiver) = watch::channel(None);
        let hub = Self {
            _stop: stop,
            changes: changes.clone(),
            connected: receiver,
        };
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = stopped.changed() => return,
                    _ = observe(&platform, &changes, &connected) => {},
                }
                connected.send_replace(None);
                tokio::select! {
                    _ = stopped.changed() => return,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                }
            }
        });
        hub
    }
    pub async fn listen(&self) -> Result<AuthorityChanges> {
        let changes = self.changes.subscribe();
        let mut connected = self.connected.clone();
        tokio::time::timeout(Duration::from_secs(5), async {
            while connected.borrow_and_update().is_none() {
                connected
                    .changed()
                    .await
                    .map_err(|_| ComputerError::Unavailable)?;
            }
            Ok::<_, ComputerError>(())
        })
        .await
        .map_err(|_| ComputerError::Unavailable)??;
        let epoch = (*connected.borrow()).ok_or(ComputerError::Unavailable)?;
        Ok(AuthorityChanges {
            epoch,
            changes,
            connected,
        })
    }
}
impl AuthorityChanges {
    pub fn check(&self) -> Result<()> {
        if self.connected.has_changed().is_err() || *self.connected.borrow() != Some(self.epoch) {
            Err(ComputerError::Unavailable)
        } else {
            Ok(())
        }
    }
    pub fn into_stream(
        self,
        interest: AuthorityInterest,
    ) -> futures::stream::BoxStream<'static, Result<()>> {
        futures::stream::unfold(Some(self), move |state| {
            let interest = interest.clone();
            async move {
                let mut listener = state?;
                let result = listener.next(&interest).await;
                let next = result.is_ok().then_some(listener);
                Some((result, next))
            }
        })
        .boxed()
    }
    pub async fn next(&mut self, interest: &AuthorityInterest) -> Result<()> {
        loop {
            self.check()?;
            tokio::select! {
                biased;
                changed = self.connected.changed() => {
                    changed.map_err(|_| ComputerError::Unavailable)?;
                    // Even a recovered stream requires a fresh attachment baseline.
                    return Err(ComputerError::Unavailable);
                }
                change = self.changes.recv() => match change {
                    Ok(change) if interest.matches(&change) => return Ok(()),
                    Err(broadcast::error::RecvError::Lagged(_)) => return Ok(()),
                    Err(broadcast::error::RecvError::Closed) => return Err(ComputerError::Unavailable),
                    _ => {},
                }
            }
        }
    }
}
async fn observe(
    platform: &PlatformStore,
    changes: &broadcast::Sender<Change>,
    connected: &watch::Sender<Option<Uuid>>,
) -> Result<()> {
    let mut source = platform.observe_changes(
        vec![
            PlatformTable::Enterprise,
            PlatformTable::Tenant,
            PlatformTable::Principal,
            PlatformTable::ComputerAutomationPolicy,
            PlatformTable::ComputerSessionGrantPolicy,
            PlatformTable::Task,
            PlatformTable::ComputerExecution,
            PlatformTable::ComputerFileTransfer,
            PlatformTable::Computer,
            PlatformTable::ComputerAutomationGrant,
            PlatformTable::ComputerSessionGrant,
            PlatformTable::ComputerCliGrant,
            PlatformTable::ComputerMaintenance,
            PlatformTable::GatewayRefreshFamily,
            PlatformTable::GatewayControlActive,
        ],
        ChangefeedCursor::initial(),
    );
    let first = tokio::time::timeout(Duration::from_secs(5), source.next())
        .await
        .map_err(|_| ComputerError::Unavailable)?;
    first
        .ok_or(ComputerError::Unavailable)?
        .map_err(|_| ComputerError::Unavailable)?;
    connected.send_replace(Some(Uuid::now_v7()));
    while let Some(delivery) = source.next().await {
        let delivery = delivery.map_err(|_| ComputerError::Unavailable)?;
        let ChangefeedDelivery::Changes { entries, .. } = delivery else {
            // Every source generation requires fresh attachment admission.
            return Err(ComputerError::Unavailable);
        };
        for entry in entries {
            match entry.table() {
                Some("gateway_refresh_family") => {
                    let id = match entry.record_id().map(|id| &id.key) {
                        Some(surrealdb::types::RecordIdKey::Uuid(id)) => **id,
                        _ => return Err(ComputerError::Unavailable),
                    };
                    let family = GatewayRefreshFamilyId::new(id.to_string())
                        .map_err(|_| ComputerError::Unavailable)?;
                    let _ = changes.send(Change::Family(family));
                }
                Some(
                    "gateway_control_active"
                    | "enterprise"
                    | "tenant"
                    | "principal"
                    | "computer_automation_policy"
                    | "computer_session_grant_policy",
                ) => {
                    let _ = changes.send(Change::Policy);
                }
                Some("computer_execution" | "computer_file_transfer") => {
                    let id = match entry.record_id().map(|id| &id.key) {
                        Some(surrealdb::types::RecordIdKey::Uuid(id)) => **id,
                        _ => return Err(ComputerError::Unavailable),
                    };
                    let change = if entry.table() == Some("computer_execution") {
                        Change::Command(
                            ExecutionId::try_from(id).map_err(|_| ComputerError::Unavailable)?,
                        )
                    } else {
                        Change::File(
                            FileTransferId::try_from(id).map_err(|_| ComputerError::Unavailable)?,
                        )
                    };
                    let _ = changes.send(change);
                }
                Some("task") => {
                    if let Some(ComputerChange::Task(id)) =
                        ComputerChange::decode(&entry).map_err(|_| ComputerError::Unavailable)?
                    {
                        let _ = changes.send(Change::Task(id));
                    }
                }
                _ => {
                    if let Some(
                        ComputerChange::Computer(id)
                        | ComputerChange::Automation { computer: id, .. },
                    ) = ComputerChange::decode(&entry).map_err(|_| ComputerError::Unavailable)?
                    {
                        let _ = changes.send(Change::Computer(id));
                    }
                }
            }
        }
    }
    Err(ComputerError::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn lost_observer_epoch_cannot_hide_behind_fast_reconnect() {
        let (sender, connected) = watch::channel(Some(Uuid::now_v7()));
        let (changes, receiver) = broadcast::channel(1);
        let epoch = connected.borrow().unwrap();
        let mut listener = AuthorityChanges {
            epoch,
            changes: receiver,
            connected,
        };
        sender.send_replace(None);
        sender.send_replace(Some(Uuid::now_v7()));
        assert!(listener.check().is_err());
        assert!(listener.next(&AuthorityInterest::All).await.is_err());
        drop(changes);
    }
    #[tokio::test]
    async fn bounded_wakes_filter_other_computers_and_lag_requires_a_baseline() {
        let (sender, connected) = watch::channel(Some(Uuid::now_v7()));
        let (changes, receiver) = broadcast::channel(1);
        let epoch = connected.borrow().unwrap();
        let mut listener = AuthorityChanges {
            epoch,
            changes: receiver,
            connected,
        };
        let computer = veoveo_computers_contract::ComputerId::new();
        let family = GatewayRefreshFamilyId::new(Uuid::now_v7().to_string()).unwrap();
        changes
            .send(Change::Computer(
                veoveo_computers_contract::ComputerId::new(),
            ))
            .ok()
            .unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(20),
                listener.next(&AuthorityInterest::Attachment {
                    computer,
                    family: family.clone()
                })
            )
            .await
            .is_err()
        );
        changes.send(Change::Computer(computer)).ok().unwrap();
        listener
            .next(&AuthorityInterest::Attachment {
                computer,
                family: family.clone(),
            })
            .await
            .unwrap();
        for _ in 0..3 {
            changes
                .send(Change::Computer(
                    veoveo_computers_contract::ComputerId::new(),
                ))
                .ok()
                .unwrap();
        }
        listener
            .next(&AuthorityInterest::Attachment {
                computer,
                family: family.clone(),
            })
            .await
            .unwrap();
        drop(sender);
        assert!(listener.check().is_err());
    }
}
