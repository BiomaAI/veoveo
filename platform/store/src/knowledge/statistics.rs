use super::*;
use crate::PlatformStore;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use veoveo_knowledge_contract::CollectionStatistics;

/// The next authority/freshness deadline schedules one re-read, never a polling loop.
pub struct CollectionStatisticsSnapshot {
    pub statistics: CollectionStatistics,
    pub next_expiry: Option<DateTime<Utc>>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    indexed_members: u64,
    indexed_chunks: u64,
    last_observed_at: Option<DateTime<Utc>>,
    last_modified_at: Option<DateTime<Utc>>,
    next_expiry: Option<DateTime<Utc>>,
}

impl PlatformStore {
    pub async fn knowledge_collection_statistics(
        &self,
        scope: &CandidateScope,
        generation: GenerationId,
    ) -> Result<CollectionStatisticsSnapshot, StoreError> {
        scope.validate()?;
        if scope.collections.len() != 1 {
            return Err(StoreError::Knowledge(
                "statistics require one admitted collection",
            ));
        }
        let sql = include_str!("statistics.surql")
            .replace("__ADMISSION__", include_str!("admitted.surql"))
            .replace(
                "__URI_SELECTION__",
                include_str!("resource_selection.surql"),
            )
            .replace("__TABLE__", &chunk_table(generation));
        let mut response = scope
            .bind(self.client().query(sql), generation)
            .await?
            .knowledge_check()?;
        let rows: Vec<Document<Row>> = response.take(response.num_statements() - 1)?;
        match rows.as_slice() {
            [] => Ok(CollectionStatisticsSnapshot {
                statistics: CollectionStatistics::empty(),
                next_expiry: None,
            }),
            [Document(row)] => Ok(CollectionStatisticsSnapshot {
                statistics: CollectionStatistics::new(
                    row.indexed_members,
                    row.indexed_chunks,
                    row.last_observed_at,
                    row.last_modified_at,
                )
                .map_err(|e| StoreError::Knowledge(e.0))?,
                next_expiry: row.next_expiry,
            }),
            _ => integrity(),
        }
    }
}
