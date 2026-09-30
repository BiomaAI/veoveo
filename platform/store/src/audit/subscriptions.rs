//! LIVE projections are filtered by the database before rows enter the application.
use super::codec::Row;
use crate::{PlatformStore, StoreError};
use futures::{StreamExt, stream::BoxStream};
use surrealdb::{
    Notification,
    types::{RecordId, SurrealValue, Value},
};
use veoveo_audit_contract::{AuditPartition, AuditReadScope, AuditRecord};

#[derive(SurrealValue)]
struct Identity {
    id: RecordId,
}
#[derive(Debug)]
pub enum AuditLiveChange {
    Record(Box<AuditRecord>),
    RetainedRangeChanged,
}
impl PlatformStore {
    /// An access receipt is scoped to its actor and profile as well as the target
    /// partition. Its ID cannot be used as a bearer credential.
    pub async fn audit_view_admitted(
        &self,
        scope: &AuditReadScope,
        partition: &AuditPartition,
        actor_partition: &AuditPartition,
        actor: &veoveo_types::PrincipalId,
        profile: &veoveo_types::GatewayProfileId,
        view: veoveo_audit_contract::AuditRecordId,
    ) -> Result<bool, StoreError> {
        use veoveo_audit_contract::{AuditDetail, AuditReadMethod, AuditTarget};
        if !scope.permits(partition) || !scope.permits(actor_partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        let mut response = self
            .db
            .query(include_str!("view.surql"))
            .bind(("id", super::record_id(actor_partition, view)))
            .bind(("actor_partition", actor_partition.storage_key()))
            .bind(("actor", actor.to_string()))
            .bind(("profile", profile.to_string()))
            .bind((
                "target",
                super::codec::scalar(&AuditTarget::AuditLog {
                    partition: partition.clone(),
                }),
            ))
            .bind((
                "detail",
                super::codec::scalar(&AuditDetail::Read {
                    method: AuditReadMethod::AuditView,
                }),
            ))
            .await?
            .check()?;
        let id: Option<RecordId> = response.take(0)?;
        Ok(id.is_some())
    }
    pub async fn audit_live(
        &self,
        scope: &AuditReadScope,
        partition: &AuditPartition,
    ) -> Result<BoxStream<'static, Result<AuditLiveChange, StoreError>>, StoreError> {
        if !scope.permits(partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        let mut response = self.db.query("LIVE SELECT id, partition, draft, recorded_at FROM audit_record WHERE partition = $partition;")
            .bind(("partition", partition.storage_key())).await?.check()?;
        let stream = response.stream::<Notification<Value>>(0)?;
        Ok(Box::pin(stream.map(|notification| {
            let notification = notification.map_err(StoreError::from)?;
            if notification.action == surrealdb::types::Action::Delete {
                return Ok(AuditLiveChange::RetainedRangeChanged);
            }
            let row = Row::from_value(notification.data).map_err(|_| StoreError::AuditIntegrity)?;
            Ok(AuditLiveChange::Record(Box::new(row.checked()?)))
        })))
    }
    pub async fn audit_sealer_wakes(
        &self,
    ) -> Result<BoxStream<'static, Result<(), StoreError>>, StoreError> {
        let mut response = self
            .db
            .query("LIVE SELECT id FROM audit_record;")
            .await?
            .check()?;
        let stream = response.stream::<Notification<Identity>>(0)?;
        Ok(Box::pin(stream.map(|notification| {
            let notification = notification.map_err(StoreError::from)?;
            let _ = notification.data.id;
            Ok(())
        })))
    }
    /// Trusted installation exporter wake source; readers use partition-scoped APIs.
    pub async fn audit_export_wakes(
        &self,
    ) -> Result<BoxStream<'static, Result<(), StoreError>>, StoreError> {
        let mut response = self
            .db
            .query("LIVE SELECT id FROM audit_block;")
            .await?
            .check()?;
        let stream = response.stream::<Notification<Identity>>(0)?;
        Ok(Box::pin(stream.map(|notification| {
            let notification = notification.map_err(StoreError::from)?;
            let _ = notification.data.id;
            Ok(())
        })))
    }
}

#[derive(SurrealValue)]
struct BlockChange {
    sequence: i64,
}

impl PlatformStore {
    /// Partition-owned block notifications provide durable recovery checkpoints.
    pub async fn audit_blocks_live(
        &self,
        scope: &AuditReadScope,
        partition: &AuditPartition,
    ) -> Result<BoxStream<'static, Result<(), StoreError>>, StoreError> {
        if !scope.permits(partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        let mut response = self
            .db
            .query("LIVE SELECT sequence FROM audit_block WHERE partition = $partition;")
            .bind(("partition", partition.storage_key()))
            .await?
            .check()?;
        let stream = response.stream::<Notification<BlockChange>>(0)?;
        Ok(Box::pin(stream.map(|notification| {
            let notification = notification.map_err(StoreError::from)?;
            let _ = notification.data.sequence;
            Ok(())
        })))
    }

    /// Subscribe before reading the marker to cover a concurrent sealing commit.
    /// The owner applies its request deadline; dropping this future closes the LIVE query.
    pub async fn audit_wait_sealed(
        &self,
        scope: &AuditReadScope,
        partition: &AuditPartition,
        record: veoveo_audit_contract::AuditRecordId,
    ) -> Result<(), StoreError> {
        if !scope.permits(partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        let id = RecordId::new("audit_record_seal", super::record_id(partition, record).key);
        let mut response = self
            .db
            .query("LIVE SELECT id FROM audit_record_seal WHERE id = $id;")
            .bind(("id", id.clone()))
            .await?
            .check()?;
        let mut live = response.stream::<Notification<Identity>>(0)?;
        let mut current = self
            .db
            .query("SELECT id FROM ONLY $id;")
            .bind(("id", id))
            .await?
            .check()?;
        if current.take::<Option<Identity>>(0)?.is_some() {
            return Ok(());
        }
        match live.next().await {
            Some(Ok(notification)) if notification.action == surrealdb::types::Action::Create => {
                Ok(())
            }
            Some(Err(error)) => Err(error.into()),
            _ => Err(StoreError::AuditIntegrity),
        }
    }
}
