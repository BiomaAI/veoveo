use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use duckdb::{Transaction, params};
use tokio::sync::Mutex;
use veoveo_platform_store::{MapFeatureRevisionRecord, OutboxEventRecord, PlatformStore};

use crate::{
    analytics::MapAnalytics,
    contract::{MapFeature, QueryFeaturesOutput, QueryFeaturesRequest},
};

use super::query;

const CONSUMER: &str = "map-mcp-authored-features-v1";
const PAGE_SIZE: u32 = 1_000;
const COMMITTED_EVENT: &str = "map.feature_changes.committed";

#[derive(Clone, Debug)]
pub struct AuthoringProjection {
    store: PlatformStore,
    analytics: MapAnalytics,
    writer: Arc<Mutex<()>>,
}

#[derive(Debug)]
struct ProjectedRevision {
    tenant_key: String,
    work_context_key: String,
    commit_sequence: i64,
    record: MapFeatureRevisionRecord,
}

impl AuthoringProjection {
    pub fn new(store: PlatformStore, analytics: MapAnalytics) -> Self {
        Self {
            store,
            analytics,
            writer: Arc::new(Mutex::new(())),
        }
    }

    pub fn sequence(&self) -> Result<u64> {
        let connection = self.analytics.connection(true)?;
        let sequence = connection.query_row(
            "SELECT coalesce(max(last_sequence), 0) FROM map_authored_projection WHERE consumer = ?",
            params![CONSUMER],
            |row| row.get::<_, i64>(0),
        )?;
        u64::try_from(sequence).context("authored map projection sequence is negative")
    }

    pub async fn reconcile(&self) -> Result<u64> {
        self.reconcile_to(None).await
    }

    pub async fn reconcile_through(&self, minimum_sequence: u64) -> Result<u64> {
        self.reconcile_to(Some(minimum_sequence)).await
    }

    pub fn query(
        &self,
        tenant_key: &str,
        work_context_key: &str,
        request: &QueryFeaturesRequest,
        publication_revision: Option<u64>,
        projection_sequence: u64,
    ) -> Result<QueryFeaturesOutput> {
        query::query_features(
            &self.analytics,
            tenant_key,
            work_context_key,
            request,
            publication_revision,
            projection_sequence,
        )
    }

    async fn reconcile_to(&self, minimum_sequence: Option<u64>) -> Result<u64> {
        let _writer = self.writer.lock().await;
        let mut sequence = i64::try_from(self.sequence()?)?;
        loop {
            if minimum_sequence.is_some_and(|minimum| sequence >= minimum as i64) {
                return u64::try_from(sequence).context("projection sequence is negative");
            }
            let page = self.store.read_outbox(sequence, PAGE_SIZE).await?;
            if page.events.is_empty() {
                if let Some(minimum) = minimum_sequence {
                    bail!(
                        "authored map projection stopped at sequence {sequence} before required sequence {minimum}"
                    );
                }
                return u64::try_from(sequence).context("projection sequence is negative");
            }

            let mut revisions = Vec::new();
            for event in &page.events {
                if event.event_type == COMMITTED_EVENT {
                    revisions.extend(self.revisions_for_event(event).await?);
                }
            }
            self.apply_page(&revisions, page.next_sequence)?;
            sequence = page.next_sequence;
            self.store.checkpoint_outbox(CONSUMER, sequence).await?;

            if page.events.len() < PAGE_SIZE as usize && minimum_sequence.is_none() {
                return u64::try_from(sequence).context("projection sequence is negative");
            }
        }
    }

    async fn revisions_for_event(
        &self,
        event: &OutboxEventRecord,
    ) -> Result<Vec<ProjectedRevision>> {
        if event.schema_version != 1 {
            bail!(
                "authored map event {} uses unsupported schema version {}",
                event.sequence,
                event.schema_version
            );
        }
        let tenant_key = event_string(event, "tenant_key")?;
        let work_context_key = event_string(event, "work_context_key")?;
        let layer_key = event_string(event, "layer_key")?;
        let changeset_key = event_string(event, "changeset_key")?;
        if event.aggregate_id != changeset_key {
            bail!(
                "authored map event {} aggregate id does not match its changeset",
                event.sequence
            );
        }
        let records = self
            .store
            .list_map_feature_revisions_for_changeset(
                &tenant_key,
                &work_context_key,
                &changeset_key,
            )
            .await?;
        if records.is_empty() {
            bail!(
                "authored map event {} references an empty changeset",
                event.sequence
            );
        }
        for record in &records {
            if record.layer_key != layer_key || record.changeset_key != changeset_key {
                bail!(
                    "authored map event {} has inconsistent revisions",
                    event.sequence
                );
            }
        }
        Ok(records
            .into_iter()
            .map(|record| ProjectedRevision {
                tenant_key: tenant_key.clone(),
                work_context_key: work_context_key.clone(),
                commit_sequence: event.sequence,
                record,
            })
            .collect())
    }

    fn apply_page(&self, revisions: &[ProjectedRevision], sequence: i64) -> Result<()> {
        let mut connection = self.analytics.connection(false)?;
        let transaction = connection.transaction()?;
        for revision in revisions {
            insert_revision(&transaction, revision)?;
            replace_head(&transaction, revision)?;
        }
        transaction.execute(
            "INSERT INTO map_authored_projection VALUES (?, ?, ?) ON CONFLICT (consumer) DO UPDATE SET last_sequence = excluded.last_sequence, updated_at = excluded.updated_at",
            params![CONSUMER, sequence, Utc::now()],
        )?;
        transaction.commit()?;
        Ok(())
    }
}

fn insert_revision(transaction: &Transaction<'_>, projected: &ProjectedRevision) -> Result<()> {
    let record = &projected.record;
    let feature: MapFeature = serde_json::from_str(&record.canonical_json)
        .context("decoding canonical authored map feature")?;
    if feature.id.as_str() != record.feature_key
        || feature.layer_id.as_str() != record.layer_key
        || i64::try_from(feature.feature_revision)? != record.feature_revision
    {
        bail!("canonical authored feature does not match its revision record");
    }
    let properties = serde_json::to_string(&feature.properties)?;
    transaction.execute(
        "INSERT OR IGNORE INTO map_authored_feature_revision VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?, ?, ?, ?, ?, ?, ?, ?::JSON, ?::JSON, ?)",
        params![
            projected.tenant_key,
            projected.work_context_key,
            record.layer_key,
            record.feature_key,
            record.feature_revision,
            record.layer_revision,
            record.schema_version,
            record.changeset_key,
            projected.commit_sequence,
            record.deleted,
            record.geometry_type,
            record.geometry_json,
            record.bbox_west,
            record.bbox_south,
            record.bbox_east,
            record.bbox_north,
            record.valid_from,
            record.valid_until,
            record.semantic_type,
            record.title,
            properties,
            record.canonical_json,
            record.created_at,
        ],
    )?;
    Ok(())
}

fn replace_head(transaction: &Transaction<'_>, projected: &ProjectedRevision) -> Result<()> {
    let record = &projected.record;
    let feature: MapFeature = serde_json::from_str(&record.canonical_json)
        .context("decoding canonical authored map feature")?;
    let properties = serde_json::to_string(&feature.properties)?;
    transaction.execute(
        "DELETE FROM map_authored_feature_head WHERE tenant_key = ? AND layer_key = ? AND feature_key = ? AND feature_revision <= ?",
        params![
            projected.tenant_key,
            record.layer_key,
            record.feature_key,
            record.feature_revision,
        ],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO map_authored_feature_head VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?, ?, ?, ?, ?, ?, ?, ?::JSON, ?::JSON, ?)",
        params![
            projected.tenant_key,
            projected.work_context_key,
            record.layer_key,
            record.feature_key,
            record.feature_revision,
            record.layer_revision,
            record.schema_version,
            record.changeset_key,
            projected.commit_sequence,
            record.deleted,
            record.geometry_type,
            record.geometry_json,
            record.bbox_west,
            record.bbox_south,
            record.bbox_east,
            record.bbox_north,
            record.valid_from,
            record.valid_until,
            record.semantic_type,
            record.title,
            properties,
            record.canonical_json,
            record.created_at,
        ],
    )?;
    Ok(())
}

fn event_string(event: &OutboxEventRecord, field: &'static str) -> Result<String> {
    event
        .payload
        .as_map()
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .with_context(|| format!("authored map event {} lacks {field}", event.sequence))
}
