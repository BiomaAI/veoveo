//! Fenced sealing and partition-scoped block reads. Request writes never touch a sequence.
use super::{
    codec::{self, Row},
    record_id,
};
use crate::{
    ChangefeedCursor, ChangefeedEntry, PlatformStore, RecordId, StoreError, decode_changefeed_entry,
};
use chrono::{DateTime, Utc};
use surrealdb::types::{Array, Object, SurrealValue, Uuid as SurrealUuid, Value};
use veoveo_audit_contract::*;

#[derive(Debug, Clone, SurrealValue)]
pub struct AuditSealLease {
    pub owner: uuid::Uuid,
    pub generation: i64,
    pub cursor: i64,
    pub lease_until: DateTime<Utc>,
}
#[derive(Debug)]
pub struct AuditCommittedRecords {
    /// First versionstamp not included in this batch.
    pub next: AuditVersionstamp,
    pub records: Vec<(AuditVersionstamp, AuditRecord)>,
}
/// The retained interval frozen for an export. A later deletion must fail the
/// export rather than silently move its first block forward.
pub struct AuditExportRange {
    pub checkpoint: Option<AuditCheckpoint>,
    pub first: Option<AuditBlockSequence>,
}
#[derive(SurrealValue)]
struct ExportRangeRow {
    checkpoint: Option<Value>,
    retired: i64,
}
#[derive(SurrealValue)]
pub(super) struct BlockRow {
    id: RecordId,
    partition: String,
    sequence: i64,
    block: Value,
}
impl BlockRow {
    pub(super) fn checked(self) -> Result<AuditBlock, StoreError> {
        let block: AuditBlock = codec::decode(self.block)?;
        if self.id != block_id(&block.head.partition, block.head.sequence)
            || self.partition != block.head.partition.storage_key()
            || self.sequence as u64 != block.head.sequence.get()
        {
            return Err(StoreError::AuditIntegrity);
        }
        Ok(block)
    }
}
pub(super) fn block_id(partition: &AuditPartition, sequence: AuditBlockSequence) -> RecordId {
    RecordId::new(
        "audit_block",
        Array::from(vec![
            (partition.storage_key()).into_value(),
            (sequence.get() as i64).into_value(),
        ]),
    )
}
pub(super) fn named_id(table: &'static str, partition: &AuditPartition) -> RecordId {
    RecordId::new(table, partition.storage_key())
}
fn block_row(block: &AuditBlock) -> Value {
    let mut row = Object::new();
    row.insert(
        "partition",
        (block.head.partition.storage_key()).into_value(),
    );
    row.insert("sequence", (block.head.sequence.get() as i64).into_value());
    row.insert(
        "first_versionstamp",
        (block.head.first_versionstamp.get() as i64).into_value(),
    );
    row.insert(
        "last_versionstamp",
        (block.head.last_versionstamp.get() as i64).into_value(),
    );
    row.insert(
        "records",
        block
            .head
            .members
            .iter()
            .map(|m| record_id(&block.head.partition, m.id))
            .collect::<Vec<_>>()
            .into_value(),
    );
    row.insert("block", codec::scalar(block));
    row.insert("sealed_at", block.head.sealed_at.into_value());
    Value::Object(row)
}
fn block_write(block: &AuditBlock) -> Value {
    let mut item = Object::new();
    item.insert(
        "id",
        block_id(&block.head.partition, block.head.sequence).into_value(),
    );
    item.insert(
        "head_id",
        named_id("audit_partition_head", &block.head.partition).into_value(),
    );
    item.insert(
        "partition",
        (block.head.partition.storage_key()).into_value(),
    );
    item.insert("sequence", (block.head.sequence.get() as i64).into_value());
    item.insert("head_hash", block.head_hash.to_string().into_value());
    item.insert(
        "previous",
        block
            .head
            .previous
            .as_ref()
            .map(ToString::to_string)
            .into_value(),
    );
    item.insert("checkpoint", codec::scalar(&block.checkpoint()));
    item.insert("row", block_row(block));
    Value::Object(item)
}
impl PlatformStore {
    pub async fn audit_export_range(
        &self,
        scope: &AuditReadScope,
        partition: &AuditPartition,
    ) -> Result<AuditExportRange, StoreError> {
        if !scope.permits(partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        let mut response = self
            .db
            .query(include_str!("export_range.surql"))
            .bind(("head_id", named_id("audit_partition_head", partition)))
            .bind(("anchor_id", named_id("audit_retention_anchor", partition)))
            .await?
            .check()?;
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::AuditIntegrity)?;
        let row: Option<ExportRangeRow> = response.take(index)?;
        let row = row.ok_or(StoreError::AuditIntegrity)?;
        let checkpoint: Option<AuditCheckpoint> = row.checkpoint.map(codec::decode).transpose()?;
        let retired = u64::try_from(row.retired).map_err(|_| StoreError::AuditIntegrity)?;
        if checkpoint
            .as_ref()
            .is_some_and(|head| &head.partition != partition || retired > head.sequence.get())
            || (checkpoint.is_none() && retired != 0)
        {
            return Err(StoreError::AuditIntegrity);
        }
        let first = checkpoint
            .as_ref()
            .filter(|head| retired < head.sequence.get())
            .map(|_| AuditBlockSequence::new(retired + 1))
            .transpose()
            .map_err(|_| StoreError::AuditIntegrity)?;
        Ok(AuditExportRange { checkpoint, first })
    }
    pub async fn acquire_audit_seal_lease(
        &self,
        owner: uuid::Uuid,
    ) -> Result<AuditSealLease, StoreError> {
        let mut response = self
            .db
            .query(include_str!("lease.surql"))
            .bind(("owner", SurrealUuid::from(owner)))
            .await?;
        if let Some(error) = crate::primary_transaction_error(response.take_errors()) {
            return Err(control_error(error));
        }
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::AuditIntegrity)?;
        let lease: Option<AuditSealLease> = response.take(index)?;
        lease.ok_or(StoreError::AuditIntegrity)
    }
    /// Installation-owned sealer only. It follows the native database feed to avoid
    /// the table-filter-before-LIMIT hole in SurrealDB 3.3. No UI uses this method.
    pub async fn audit_committed_records(
        &self,
        lease: &AuditSealLease,
    ) -> Result<AuditCommittedRecords, StoreError> {
        let cursor =
            ChangefeedCursor::from_versionstamp(lease.cursor).ok_or(StoreError::AuditIntegrity)?;
        let batches = self.replay_changes(cursor, 32).await?;
        let mut next = lease.cursor;
        let mut records = Vec::new();
        for batch in batches {
            let mut batch_records = Vec::new();
            let versionstamp = AuditVersionstamp::new(batch.versionstamp as u64)
                .map_err(|_| StoreError::AuditIntegrity)?;
            for change in batch.changes {
                let entry = decode_changefeed_entry(&change)?;
                if entry.table() != Some("audit_record") {
                    continue;
                }
                if let ChangefeedEntry::Upsert(value) = entry {
                    let row = Row::from_value(value).map_err(|_| StoreError::AuditIntegrity)?;
                    batch_records.push((versionstamp, row.checked()?));
                }
            }
            if batch_records.len() > 4096 {
                return Err(StoreError::AuditBatchLimit);
            }
            if records.len() + batch_records.len() > 4096 {
                break;
            }
            records.extend(batch_records);
            next = batch
                .versionstamp
                .checked_add(1)
                .ok_or(StoreError::AuditIntegrity)?;
        }
        Ok(AuditCommittedRecords {
            next: AuditVersionstamp::new(next as u64).map_err(|_| StoreError::AuditIntegrity)?,
            records,
        })
    }
    pub async fn audit_partition_checkpoint(
        &self,
        partition: &AuditPartition,
    ) -> Result<Option<AuditCheckpoint>, StoreError> {
        let mut response = self
            .db
            .query("SELECT VALUE checkpoint FROM ONLY $id;")
            .bind(("id", named_id("audit_partition_head", partition)))
            .await?
            .check()?;
        let value: Option<Value> = response.take(0)?;
        let checkpoint: Option<AuditCheckpoint> = value.map(codec::decode).transpose()?;
        if checkpoint
            .as_ref()
            .is_some_and(|c| &c.partition != partition)
        {
            return Err(StoreError::AuditIntegrity);
        }
        Ok(checkpoint)
    }
    pub async fn commit_audit_blocks(
        &self,
        lease: &AuditSealLease,
        next: AuditVersionstamp,
        blocks: &[AuditBlock],
    ) -> Result<(), StoreError> {
        if next.get() < lease.cursor as u64 || blocks.len() > 4096 {
            return Err(StoreError::AuditIntegrity);
        }
        let mut response = self
            .db
            .query(include_str!("seal.surql"))
            .bind(("owner", SurrealUuid::from(lease.owner)))
            .bind(("generation", lease.generation))
            .bind(("expected_cursor", lease.cursor))
            .bind(("next_cursor", next.get() as i64))
            .bind(("blocks", blocks.iter().map(block_write).collect::<Vec<_>>()))
            .await?;
        if let Some(error) = crate::primary_transaction_error(response.take_errors()) {
            return Err(control_error(error));
        }
        Ok(())
    }
    /// Return blocks in committed partition order, with SQL admission before decoding.
    pub async fn audit_blocks(
        &self,
        scope: &AuditReadScope,
        partition: &AuditPartition,
        after: Option<AuditBlockSequence>,
        limit: u16,
    ) -> Result<Vec<AuditBlock>, StoreError> {
        if !scope.permits(partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        if limit == 0 || limit > 100 {
            return Err(StoreError::AuditBatchLimit);
        }
        let mut response = self.db.query("SELECT id, partition, sequence, block FROM audit_block:[$partition, 0]..=[$partition, 9223372036854775807] WHERE partition = $partition AND sequence > $after ORDER BY id ASC LIMIT $limit;")
            .bind(("partition", partition.storage_key())).bind(("after", after.map_or(0, |n| n.get() as i64)))
            .bind(("limit", u32::from(limit))).await?.check()?;
        let rows: Vec<BlockRow> = response.take(0)?;
        rows.into_iter().map(BlockRow::checked).collect()
    }
    pub async fn audit_block_records(
        &self,
        scope: &AuditReadScope,
        block: &AuditBlock,
    ) -> Result<Vec<AuditRecord>, StoreError> {
        if !scope.permits(&block.head.partition) {
            return Err(StoreError::AuditAccessDenied);
        }
        if block.head.members.len() > 4096 {
            return Err(StoreError::AuditBatchLimit);
        }
        let ids = block
            .head
            .members
            .iter()
            .map(|m| record_id(&block.head.partition, m.id))
            .collect::<Vec<_>>();
        let mut response = self
            .db
            .query(
                "SELECT id, partition, draft, recorded_at FROM $ids WHERE partition = $partition;",
            )
            .bind(("ids", ids))
            .bind(("partition", block.head.partition.storage_key()))
            .await?
            .check()?;
        let rows: Vec<Row> = response.take(0)?;
        let mut rows = rows
            .into_iter()
            .map(|row| {
                let record = row.checked()?;
                Ok((record.draft.id(), record))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, StoreError>>()?;
        block
            .head
            .members
            .iter()
            .map(|member| rows.remove(&member.id).ok_or(StoreError::AuditIntegrity))
            .collect()
    }
    /// A sealed block supplies a stable set of IDs; every content filter runs in
    /// SQL. Association with member positions restores native commit order only.
    pub async fn audit_filtered_block_records(
        &self,
        scope: &AuditReadScope,
        block: &AuditBlock,
        query: &AuditQuery,
    ) -> Result<Vec<AuditRecord>, StoreError> {
        query.validate()?;
        if !scope.permits(&block.head.partition) || query.partition != block.head.partition {
            return Err(StoreError::AuditAccessDenied);
        }
        if block.head.members.len() > 4096 {
            return Err(StoreError::AuditBatchLimit);
        }
        let ids = block
            .head
            .members
            .iter()
            .map(|m| record_id(&block.head.partition, m.id))
            .collect::<Vec<_>>();
        let mut response = self
            .db
            .query(include_str!("block_records.surql"))
            .bind(("ids", ids))
            .bind(("partition", query.partition.storage_key()))
            .bind((
                "after",
                query
                    .cursor
                    .as_ref()
                    .map(|c| record_id(&c.partition, c.last_id)),
            ))
            .bind(("class", query.class.map(|c| codec::scalar(&c))))
            .bind(("actor", query.actor.as_ref().map(ToString::to_string)))
            .bind(("target", query.target.as_ref().map(codec::scalar)))
            .bind(("outcome", query.outcome.map(|c| codec::scalar(&c))))
            .bind(("trace", query.trace.as_ref().map(ToString::to_string)))
            .bind(("from", query.from))
            .bind(("until", query.until))
            .await?
            .check()?;
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::AuditIntegrity)?;
        let rows: Vec<Row> = response.take(index)?;
        let order = block
            .head
            .members
            .iter()
            .enumerate()
            .map(|(index, member)| (member.id, index))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut records = rows
            .into_iter()
            .map(Row::checked)
            .collect::<Result<Vec<_>, _>>()?;
        records.sort_by_key(|record| order.get(&record.draft.id()).copied());
        Ok(records)
    }
    pub async fn retain_audit_block(
        &self,
        lease: &AuditSealLease,
        block: &AuditBlock,
        cutoff: DateTime<Utc>,
        destinations: &[AuditDestinationId],
    ) -> Result<(), StoreError> {
        let mut response = self
            .db
            .query(include_str!("retention.surql"))
            .bind(("owner", SurrealUuid::from(lease.owner)))
            .bind(("generation", lease.generation))
            .bind(("id", block_id(&block.head.partition, block.head.sequence)))
            .bind((
                "anchor_id",
                named_id("audit_retention_anchor", &block.head.partition),
            ))
            .bind(("cutoff", cutoff))
            .bind((
                "destinations",
                destinations
                    .iter()
                    .map(|id| id.as_str().to_owned())
                    .collect::<Vec<_>>(),
            ))
            .await?;
        if let Some(error) = crate::primary_transaction_error(response.take_errors()) {
            return Err(control_error(error));
        }
        Ok(())
    }
}

pub(super) fn control_error(error: surrealdb::Error) -> StoreError {
    if error.is_thrown() {
        let message = error.message();
        for (code, mapped) in [
            ("audit_lease_busy", StoreError::AuditLeaseBusy),
            ("audit_lease_lost", StoreError::AuditLeaseLost),
            ("audit_changefeed_gap", StoreError::AuditChangefeedGap),
            (
                "audit_retention_not_admitted",
                StoreError::AuditRetentionNotAdmitted,
            ),
            ("audit_block_conflict", StoreError::AuditIntegrity),
            ("audit_export_rejected", StoreError::AuditExportRejected),
        ] {
            if message.contains(code) {
                return mapped;
            }
        }
    }
    error.into()
}
