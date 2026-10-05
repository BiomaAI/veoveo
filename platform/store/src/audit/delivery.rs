//! Installation exporter only. Persist intent before a network side effect, and
//! acknowledge delivery under the same lease used by retention and sealing.
use super::{
    AuditSealLease,
    blocks::{BlockRow, block_id, control_error, named_id},
    codec,
};
use crate::{PlatformStore, RecordId, StoreError};
use surrealdb::types::{Array, SurrealValue, Uuid as SurrealUuid};
use veoveo_audit_contract::*;

impl PlatformStore {
    pub async fn audit_export_candidate(
        &self,
        destination: &AuditDestinationId,
    ) -> Result<Option<AuditBlock>, StoreError> {
        let mut response = self
            .db
            .query(include_str!("../queries/audit/export_candidate.surql"))
            .bind(("destination", destination.as_str().to_owned()))
            .await?
            .check()
            .map_err(control_error)?;
        let index = response
            .num_statements()
            .checked_sub(1)
            .ok_or(StoreError::AuditIntegrity)?;
        let rows: Vec<BlockRow> = response.take(index)?;
        rows.into_iter().next().map(BlockRow::checked).transpose()
    }

    pub async fn prepare_audit_export(
        &self,
        lease: &AuditSealLease,
        destination: &AuditDestinationId,
        block: &AuditBlock,
        payload: &AuditExportPayload,
    ) -> Result<(), StoreError> {
        self.audit_export_transition(lease, destination, block, payload, false, None)
            .await
    }

    pub async fn complete_audit_export(
        &self,
        lease: &AuditSealLease,
        destination: &AuditDestinationId,
        block: &AuditBlock,
        payload: &AuditExportPayload,
    ) -> Result<(), StoreError> {
        self.audit_export_transition(lease, destination, block, payload, true, None)
            .await
    }

    pub async fn reject_audit_export(
        &self,
        lease: &AuditSealLease,
        destination: &AuditDestinationId,
        block: &AuditBlock,
        payload: &AuditExportPayload,
        rejection: AuditExportRejection,
    ) -> Result<(), StoreError> {
        self.audit_export_transition(lease, destination, block, payload, false, Some(rejection))
            .await
    }

    async fn audit_export_transition(
        &self,
        lease: &AuditSealLease,
        destination: &AuditDestinationId,
        block: &AuditBlock,
        payload: &AuditExportPayload,
        complete: bool,
        rejection: Option<AuditExportRejection>,
    ) -> Result<(), StoreError> {
        // Lease renewal and sealing touch the same fence while an export is in
        // flight. Retry only a database-confirmed abort, with the same identities
        // and hashes. A receipt retry never repeats the provider request.
        for attempt in 0..8 {
            match self
                .audit_export_transition_once(
                    lease,
                    destination,
                    block,
                    payload,
                    complete,
                    rejection,
                )
                .await
            {
                Err(StoreError::Database(error))
                    if attempt < 7
                        && matches!(
                            error.query_details(),
                            Some(surrealdb::types::QueryError::TransactionConflict)
                        ) =>
                {
                    tokio::time::sleep(std::time::Duration::from_millis(2u64.pow(attempt))).await;
                }
                result => return result,
            }
        }
        unreachable!("the final transaction attempt always returns")
    }

    async fn audit_export_transition_once(
        &self,
        lease: &AuditSealLease,
        destination: &AuditDestinationId,
        block: &AuditBlock,
        payload: &AuditExportPayload,
        complete: bool,
        rejection: Option<AuditExportRejection>,
    ) -> Result<(), StoreError> {
        let mut response = self
            .db
            .query(include_str!("../queries/audit/delivery.surql"))
            .bind(("owner", SurrealUuid::from(lease.owner)))
            .bind(("generation", lease.generation))
            .bind((
                "block_id",
                block_id(&block.head.partition, block.head.sequence),
            ))
            .bind(("block_hash", block.head_hash.to_string()))
            .bind(("expected_block", codec::FrozenDocument(block.clone())))
            .bind((
                "anchor_id",
                named_id("audit_retention_anchor", &block.head.partition),
            ))
            .bind(("destination", destination.as_str().to_owned()))
            .bind((
                "intent_id",
                RecordId::new(
                    "audit_export_delivery",
                    Array::from(vec![
                        (block.head.partition.storage_key()).into_value(),
                        (block.head.sequence.get() as i64).into_value(),
                        (destination.as_str()).into_value(),
                    ]),
                ),
            ))
            .bind((
                "cursor_id",
                RecordId::new(
                    "audit_export_cursor",
                    Array::from(vec![
                        (destination.as_str()).into_value(),
                        (block.head.partition.storage_key()).into_value(),
                    ]),
                ),
            ))
            .bind(("payload", codec::FrozenDocument(payload.clone())))
            .bind(("checkpoint", codec::FrozenDocument(block.checkpoint())))
            .bind(("complete", complete))
            .bind(("rejection", rejection.as_ref().map(codec::scalar)))
            .await?;
        if let Some(error) = crate::primary_transaction_error(response.take_errors()) {
            return Err(control_error(error));
        }
        Ok(())
    }
}
