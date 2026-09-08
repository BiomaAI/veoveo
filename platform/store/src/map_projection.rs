//! Bounded replay of the canonical Map changeset log.

use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::{PlatformStore, StoreError};

/// The immutable commit envelope needed to rebuild authored Map features.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapFeatureProjectionCommit {
    pub tenant_key: String,
    pub work_context_key: String,
    pub layer_key: String,
    pub changeset_key: String,
    pub commit_sequence: i64,
    pub resulting_layer_revision: i64,
    pub feature_keys: Vec<String>,
}

impl PlatformStore {
    /// Last Map sequence whose changeset and serialization head committed together.
    /// The global outbox maximum cannot provide this boundary: unrelated writers
    /// may commit a higher allocated sequence while a Map transaction is pending.
    pub async fn latest_map_feature_commit_sequence(&self) -> Result<i64, StoreError> {
        let mut response = self
            .client()
            .query("SELECT VALUE last_sequence FROM ONLY map_projection_state:authored_features;")
            .await?
            .check()?;
        response
            .take::<Option<i64>>(0)?
            .ok_or(StoreError::MissingRecord {
                operation: "Map projection committed sequence (run installation migrations)",
            })
    }

    /// Read Map commits in `(after_sequence, through_sequence]`. The caller
    /// captures the upper bound once, so unrelated traffic cannot extend replay.
    /// Changesets and their outbox sequences commit in the same transaction.
    pub async fn read_map_feature_commits(
        &self,
        after_sequence: i64,
        through_sequence: i64,
        limit: u32,
    ) -> Result<Vec<MapFeatureProjectionCommit>, StoreError> {
        if after_sequence < 0 || through_sequence < after_sequence {
            return Err(StoreError::InvalidMapField {
                field: "commit_sequence",
                reason: "replay requires 0 <= after_sequence <= through_sequence",
            });
        }
        if limit == 0 || limit > 1_000 {
            return Err(StoreError::InvalidMapField {
                field: "limit",
                reason: "Map commit page size must be between 1 and 1000",
            });
        }
        let mut response = self
            .client()
            .query(
                "SELECT tenant.slug AS tenant_key, work_context_key, layer_key, changeset_key, commit_sequence, resulting_layer_revision, feature_keys \
                 FROM map_feature_changeset \
                 WHERE commit_sequence > $after AND commit_sequence <= $through \
                 ORDER BY commit_sequence ASC LIMIT $limit;",
            )
            .bind(("after", after_sequence))
            .bind(("through", through_sequence))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
