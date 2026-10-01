//! One wake source per service replica. Notifications invalidate; fresh domain reads authorize.
use futures::StreamExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_platform_store::{
    ChangefeedCursor, ChangefeedDelivery, ComputerChange, PlatformStore, PlatformTable, RecordId,
};

#[derive(Clone)]
pub(super) enum Change {
    Computer(veoveo_computers_contract::ComputerId),
    Family(RecordId),
    Policy,
}
pub(super) struct AccessEvents {
    changes: broadcast::Sender<Change>,
    connected: watch::Receiver<Option<Uuid>>,
}
pub(super) struct Listener {
    epoch: Uuid,
    changes: broadcast::Receiver<Change>,
    connected: watch::Receiver<Option<Uuid>>,
}
impl AccessEvents {
    pub fn start(platform: PlatformStore, stop: CancellationToken) -> Arc<Self> {
        let (changes, _) = broadcast::channel(128);
        let (connected, receiver) = watch::channel(None);
        let hub = Arc::new(Self {
            changes: changes.clone(),
            connected: receiver,
        });
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => return,
                    _ = observe(&platform, &changes, &connected) => {},
                }
                connected.send_replace(None);
                tokio::select! {
                    _ = stop.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                }
            }
        });
        hub
    }
    pub async fn listen(&self) -> Result<Listener, ()> {
        let changes = self.changes.subscribe();
        let mut connected = self.connected.clone();
        tokio::time::timeout(Duration::from_secs(5), async {
            while connected.borrow_and_update().is_none() {
                connected.changed().await.map_err(|_| ())?;
            }
            Ok::<_, ()>(())
        })
        .await
        .map_err(|_| ())??;
        let epoch = (*connected.borrow()).ok_or(())?;
        Ok(Listener {
            epoch,
            changes,
            connected,
        })
    }
}
impl Listener {
    pub fn check(&self) -> Result<(), ()> {
        if self.connected.has_changed().is_err() || *self.connected.borrow() != Some(self.epoch) {
            Err(())
        } else {
            Ok(())
        }
    }
    pub async fn next(
        &mut self,
        computer: veoveo_computers_contract::ComputerId,
        family: &RecordId,
    ) -> Result<(), ()> {
        loop {
            self.check()?;
            tokio::select! {
                biased;
                changed = self.connected.changed() => {
                    changed.map_err(|_| ())?;
                    // Even a recovered stream requires a fresh attachment baseline.
                    return Err(());
                }
                change = self.changes.recv() => match change {
                    Ok(Change::Computer(id)) if id == computer => return Ok(()),
                    Ok(Change::Family(id)) if &id == family => return Ok(()),
                    Ok(Change::Policy) | Err(broadcast::error::RecvError::Lagged(_)) => return Ok(()),
                    Err(broadcast::error::RecvError::Closed) => return Err(()),
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
) -> Result<(), ()> {
    let mut source = platform.observe_changes(
        vec![
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
        .map_err(|_| ())?;
    first.ok_or(())?.map_err(|_| ())?;
    connected.send_replace(Some(Uuid::now_v7()));
    while let Some(delivery) = source.next().await {
        let delivery = delivery.map_err(|_| ())?;
        let ChangefeedDelivery::Changes { entries, .. } = delivery else {
            // Every source generation requires fresh attachment admission.
            return Err(());
        };
        for entry in entries {
            match entry.table() {
                Some("gateway_refresh_family") => {
                    let _ = changes.send(Change::Family(entry.record_id().ok_or(())?.clone()));
                }
                Some("gateway_control_active") => {
                    let _ = changes.send(Change::Policy);
                }
                _ => {
                    if let Some(
                        ComputerChange::Computer(id)
                        | ComputerChange::Automation { computer: id, .. },
                    ) = ComputerChange::decode(&entry).map_err(|_| ())?
                    {
                        let _ = changes.send(Change::Computer(id));
                    }
                }
            }
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn lost_observer_epoch_cannot_hide_behind_fast_reconnect() {
        let (sender, connected) = watch::channel(Some(Uuid::now_v7()));
        let (changes, receiver) = broadcast::channel(1);
        let epoch = connected.borrow().unwrap();
        let mut listener = Listener {
            epoch,
            changes: receiver,
            connected,
        };
        sender.send_replace(None);
        sender.send_replace(Some(Uuid::now_v7()));
        assert!(listener.check().is_err());
        assert!(
            listener
                .next(
                    veoveo_computers_contract::ComputerId::new(),
                    &RecordId::new("fixture", "family")
                )
                .await
                .is_err()
        );
        drop(changes);
    }
    #[tokio::test]
    async fn bounded_wakes_filter_other_computers_and_lag_requires_a_baseline() {
        let (sender, connected) = watch::channel(Some(Uuid::now_v7()));
        let (changes, receiver) = broadcast::channel(1);
        let epoch = connected.borrow().unwrap();
        let mut listener = Listener {
            epoch,
            changes: receiver,
            connected,
        };
        let computer = veoveo_computers_contract::ComputerId::new();
        let family = RecordId::new("fixture", "family");
        changes
            .send(Change::Computer(
                veoveo_computers_contract::ComputerId::new(),
            ))
            .ok()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.next(computer, &family))
                .await
                .is_err()
        );
        changes.send(Change::Computer(computer)).ok().unwrap();
        listener.next(computer, &family).await.unwrap();
        for _ in 0..3 {
            changes
                .send(Change::Computer(
                    veoveo_computers_contract::ComputerId::new(),
                ))
                .ok()
                .unwrap();
        }
        listener.next(computer, &family).await.unwrap();
        drop(sender);
        assert!(listener.check().is_err());
    }
}
