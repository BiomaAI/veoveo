use std::{collections::BTreeSet, sync::Arc};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use duckdb::{Transaction, params};
use tokio::sync::Mutex;
use veoveo_platform_store::{MapFeatureProjectionCommit, MapFeatureRevisionRecord, PlatformStore};

use crate::{
    analytics::MapAnalytics,
    contract::{MapFeature, QueryFeaturesOutput, QueryFeaturesRequest},
};

use super::query;

const CONSUMER: &str = "map-mcp-authored-features-v1";
const PAGE_SIZE: u32 = 1_000;
const INSERT_REVISION_SQL: &str = "INSERT INTO map_authored_feature_revision VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?, ?, ?, ?, ?, ?, ?, ?::JSON, ?::JSON, ?)";
const INSERT_HEAD_SQL: &str = "INSERT INTO map_authored_feature_head VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?, ?, ?, ?, ?, ?, ?, ?::JSON, ?::JSON, ?)";

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
        let connection = self.analytics.read_connection()?;
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
        let minimum = minimum_sequence.map(i64::try_from).transpose()?;
        if minimum.is_some_and(|minimum| sequence >= minimum) {
            return Ok(u64::try_from(sequence)?);
        }
        // The Map head commits atomically with each changeset. Its serialization
        // prevents later commits appearing below this captured recovery boundary.
        let through = self.store.latest_map_feature_commit_sequence().await?;
        if let Some(minimum) = minimum.filter(|minimum| *minimum > through) {
            bail!(
                "authored map projection cannot reach required sequence {minimum} beyond committed sequence {through}"
            );
        }
        while sequence < through {
            let commits = self
                .store
                .read_map_feature_commits(sequence, through, PAGE_SIZE)
                .await?;

            let mut revisions = Vec::new();
            for commit in &commits {
                revisions.extend(self.revisions_for_commit(commit).await?);
            }
            // A short page proves that every Map commit through the snapshot is
            // included. Gaps belong to other domains and need no projection work.
            let next_sequence = if commits.len() < PAGE_SIZE as usize {
                through
            } else {
                commits
                    .last()
                    .context("full Map commit page is empty")?
                    .commit_sequence
            };
            self.apply_page(&revisions, next_sequence)?;
            sequence = next_sequence;
            self.store.checkpoint_outbox(CONSUMER, sequence).await?;
        }
        Ok(u64::try_from(sequence)?)
    }

    async fn revisions_for_commit(
        &self,
        commit: &MapFeatureProjectionCommit,
    ) -> Result<Vec<ProjectedRevision>> {
        let records = self
            .store
            .list_map_feature_revisions_for_changeset(
                &commit.tenant_key,
                &commit.work_context_key,
                &commit.changeset_key,
            )
            .await?;
        let expected = commit.feature_keys.iter().collect::<BTreeSet<_>>();
        let actual = records
            .iter()
            .map(|record| &record.feature_key)
            .collect::<BTreeSet<_>>();
        if records.is_empty() || expected != actual || records.len() != commit.feature_keys.len() {
            bail!(
                "authored map commit {} has incomplete feature revisions",
                commit.commit_sequence
            );
        }
        for record in &records {
            if record.layer_key != commit.layer_key
                || record.changeset_key != commit.changeset_key
                || record.layer_revision != commit.resulting_layer_revision
            {
                bail!(
                    "authored map commit {} has inconsistent revisions",
                    commit.commit_sequence
                );
            }
        }
        Ok(records
            .into_iter()
            .map(|record| ProjectedRevision {
                tenant_key: commit.tenant_key.clone(),
                work_context_key: commit.work_context_key.clone(),
                commit_sequence: commit.commit_sequence,
                record,
            })
            .collect())
    }

    fn apply_page(&self, revisions: &[ProjectedRevision], sequence: i64) -> Result<()> {
        let mut connection = self.analytics.connection()?;
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
        INSERT_REVISION_SQL,
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
        INSERT_HEAD_SQL,
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

#[cfg(test)]
#[path = "projection/recovery_tests.rs"]
mod recovery_tests;

#[cfg(test)]
mod tests {
    use duckdb::params;
    use tempfile::TempDir;

    use crate::analytics::{MapAnalytics, MapAnalyticsConfig};

    use super::*;

    #[test]
    fn heterogeneous_geometry_transaction_keeps_both_rtrees_queryable() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let config = MapAnalyticsConfig {
            database_path: root.path().join("map.duckdb"),
            authoring_task_root: root.path().join("tasks"),
            spill_dir: root.path().join("spill"),
            spatial_extension: extension.into(),
            memory_limit: "256MB".to_owned(),
            threads: 1,
        };
        let analytics = MapAnalytics::open(config.clone()).unwrap();
        drop(analytics);
        let analytics = MapAnalytics::open(config).unwrap();
        let geometries = [
            (
                "Point",
                r#"{"type":"Point","coordinates":[-89.215,13.695]}"#,
            ),
            (
                "LineString",
                r#"{"type":"LineString","coordinates":[[-89.222,13.698],[-89.207,13.691]]}"#,
            ),
            (
                "Polygon",
                r#"{"type":"Polygon","coordinates":[[[-89.219,13.701],[-89.219,13.692],[-89.209,13.692],[-89.209,13.701],[-89.219,13.701]]]}"#,
            ),
        ];
        let mut connection = analytics.connection().unwrap();
        let transaction = connection.transaction().unwrap();
        for (index, (geometry_type, geometry_json)) in geometries.iter().enumerate() {
            let feature_key = format!("feature-{index}");
            let changeset_key = format!("changeset-{index}");
            let now = Utc::now();
            let values = params![
                "tenant",
                "operations",
                "mixed-layer",
                feature_key,
                1_i64,
                1_i64,
                1_i64,
                changeset_key,
                index as i64 + 1,
                false,
                geometry_type,
                geometry_json,
                -89.222_f64,
                13.691_f64,
                -89.207_f64,
                13.701_f64,
                Option::<chrono::DateTime<Utc>>::None,
                Option::<chrono::DateTime<Utc>>::None,
                "AcceptanceFeature",
                Option::<String>::None,
                "{}",
                "{}",
                now,
            ];
            transaction.execute(INSERT_REVISION_SQL, values).unwrap();
            let head_values = params![
                "tenant",
                "operations",
                "mixed-layer",
                feature_key,
                1_i64,
                1_i64,
                1_i64,
                changeset_key,
                index as i64 + 1,
                false,
                geometry_type,
                geometry_json,
                -89.222_f64,
                13.691_f64,
                -89.207_f64,
                13.701_f64,
                Option::<chrono::DateTime<Utc>>::None,
                Option::<chrono::DateTime<Utc>>::None,
                "AcceptanceFeature",
                Option::<String>::None,
                "{}",
                "{}",
                now,
            ];
            transaction.execute(INSERT_HEAD_SQL, head_values).unwrap();
        }
        transaction.commit().unwrap();

        let read = analytics.read_connection().unwrap();
        for index in [
            "map_authored_revision_geometry",
            "map_authored_head_geometry",
        ] {
            let indexed: u64 = read
                .query_row(
                    &format!(
                        "SELECT count(*) FROM rtree_index_dump('{index}') WHERE row_id IS NOT NULL"
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(indexed, 3, "{index} lost heterogeneous geometry entries");
        }
        let intersecting: u64 = read
            .query_row(
                "SELECT count(*) FROM map_authored_feature_head WHERE ST_Intersects(geometry, ST_MakeEnvelope(-89.23, 13.68, -89.20, 13.71))",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(intersecting, 3);
    }
}
