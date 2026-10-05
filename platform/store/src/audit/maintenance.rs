//! Installation workers only: lease renewal, whole-block retention and expired guards.
use super::{
    AuditSealLease,
    blocks::{BlockRow, control_error},
};
use crate::{PlatformStore, StoreError};
use chrono::{DateTime, Utc};
use surrealdb::types::Uuid as SurrealUuid;
use veoveo_audit_contract::AuditBlock;

impl PlatformStore {
    /// `caught_up` is valid only while the owner has a connected LIVE subscription
    /// and has drained its persisted cursor. It prevents false gaps on an idle log.
    pub async fn renew_audit_seal_lease(
        &self,
        lease: &AuditSealLease,
        caught_up: bool,
    ) -> Result<AuditSealLease, StoreError> {
        let mut response = self
            .db
            .query(include_str!("../queries/audit/renew.surql"))
            .bind(("owner", SurrealUuid::from(lease.owner)))
            .bind(("generation", lease.generation))
            .bind(("caught_up", caught_up))
            .await?;
        if let Some(error) = crate::primary_transaction_error(response.take_errors()) {
            return Err(control_error(error));
        }
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::AuditIntegrity)?;
        response
            .take::<Option<AuditSealLease>>(index)?
            .ok_or(StoreError::AuditIntegrity)
    }
    pub async fn release_audit_seal_lease(&self, lease: &AuditSealLease) -> Result<(), StoreError> {
        self.db
            .query(include_str!(
                "../queries/audit/maintenance/release_audit_seal_lease.surql"
            ))
            .bind(("owner", SurrealUuid::from(lease.owner)))
            .bind(("generation", lease.generation))
            .await?
            .check()?;
        Ok(())
    }
    /// SQL selects only each partition's first eligible block before the limit.
    pub async fn audit_retention_candidates(
        &self,
        cutoff: DateTime<Utc>,
        destinations: &[veoveo_audit_contract::AuditDestinationId],
    ) -> Result<Vec<AuditBlock>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("../queries/audit/retention_candidates.surql"))
            .bind(("cutoff", cutoff))
            .bind((
                "destinations",
                destinations
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        response
            .take::<Vec<BlockRow>>(0)?
            .into_iter()
            .map(BlockRow::checked)
            .collect()
    }
    /// Guards are short-lived deduplication state; deleting one never deletes its record.
    pub async fn prune_audit_download_windows(&self) -> Result<usize, StoreError> {
        let mut response = self
            .db
            .query(include_str!(
                "../queries/audit/maintenance/prune_audit_download_windows.surql"
            ))
            .await?
            .check()?;
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::AuditIntegrity)?;
        let count: Option<u64> = response.take(index)?;
        usize::try_from(count.ok_or(StoreError::AuditIntegrity)?)
            .map_err(|_| StoreError::AuditIntegrity)
    }
}
