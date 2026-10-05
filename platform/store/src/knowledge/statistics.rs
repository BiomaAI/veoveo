use super::*;
use crate::PlatformStore;
use chrono::{DateTime, Utc};
use veoveo_knowledge_contract::CollectionStatistics;

/// The next authority/freshness deadline schedules one re-read, never a polling loop.
pub struct CollectionStatisticsSnapshot {
    pub statistics: CollectionStatistics,
    pub next_expiry: Option<DateTime<Utc>>,
}
#[derive(SurrealValue)]
struct Row {
    indexed_members: u64,
    indexed_chunks: u64,
    last_observed_at: Option<DateTime<Utc>>,
    last_modified_at: Option<DateTime<Utc>>,
    next_expiry: Option<DateTime<Utc>>,
}

#[derive(SurrealValue)]
struct StatisticsPage {
    rows: Vec<Row>,
    registration: Option<RegistrationRow>,
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
        let sql = include_str!("../queries/knowledge/statistics.surql");
        let mut response = scope
            .bind(self.client().query(sql), generation)
            .bind((
                "registration",
                collection_record(
                    &scope.tenant,
                    scope.collections.keys().next().expect("one collection"),
                ),
            ))
            .await?
            .knowledge_check()?;
        let page: Option<StatisticsPage> = response.take(response.num_statements() - 2)?;
        let page = page.ok_or(StoreError::Knowledge("statistics response missing"))?;
        match page.registration {
            Some(registration) => {
                registration.checked(&scope.tenant)?;
            }
            None if page
                .rows
                .iter()
                .any(|row| row.indexed_members != 0 || row.indexed_chunks != 0) =>
            {
                return integrity();
            }
            None => {}
        }
        let rows = page.rows;
        match rows.as_slice() {
            [] => Ok(CollectionStatisticsSnapshot {
                statistics: CollectionStatistics::empty(),
                next_expiry: None,
            }),
            [row] => Ok(CollectionStatisticsSnapshot {
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
