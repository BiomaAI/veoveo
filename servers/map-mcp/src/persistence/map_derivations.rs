//! Immutable Map derivations with SQL-scoped reads, pages, and completion.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{Array, RecordId, SurrealValue};

use crate::persistence::{MapRepository, MapStoreError};
use veoveo_platform_store::{
    TenantId, WorkContextId, deterministic_tenant_id, deterministic_work_context_id,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, SurrealValue)]
#[surreal(untagged)]
pub enum MapDerivationKind {
    #[serde(rename = "raster")]
    #[surreal(value = "raster")]
    Raster,
    #[serde(rename = "spatial")]
    #[surreal(value = "spatial")]
    Spatial,
}
impl MapDerivationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raster => "raster",
            Self::Spatial => "spatial",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MapDerivationScope {
    pub tenant: TenantId,
    pub work_context: WorkContextId,
}
impl MapDerivationScope {
    pub fn from_keys(tenant: &str, context: &str) -> Result<Self, MapStoreError> {
        Ok(Self {
            tenant: deterministic_tenant_id(tenant)?,
            work_context: deterministic_work_context_id(tenant, context)?,
        })
    }
    fn record(self, kind: MapDerivationKind, key: &str) -> RecordId {
        RecordId::new(
            "map_derivation",
            Array::from(vec![
                self.tenant.to_string(),
                self.work_context.to_string(),
                kind.as_str().to_owned(),
                key.to_owned(),
            ]),
        )
    }
}

#[derive(Clone, Debug)]
pub struct MapDerivationDraft {
    pub scope: MapDerivationScope,
    pub kind: MapDerivationKind,
    pub derivation_key: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    /// The owning Map contract serializes and validates this versioned document.
    pub canonical_json: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapDerivationRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub kind: MapDerivationKind,
    pub derivation_key: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub canonical_json: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapDerivationSummary {
    pub derivation_key: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl MapRepository {
    pub async fn put_map_derivation(&self, draft: MapDerivationDraft) -> Result<(), MapStoreError> {
        validate_key(draft.kind, &draft.derivation_key)?;
        if draft.created_by.is_empty() || draft.created_by.len() > 1024 {
            return Err(invalid("created_by", "must contain 1..=1024 bytes"));
        }
        if draft.canonical_json.is_empty() || draft.canonical_json.len() > 16 * 1024 * 1024 {
            return Err(invalid(
                "canonical_json",
                "must contain 1 byte through 16 MiB",
            ));
        }
        serde_json::from_str::<serde_json::Value>(&draft.canonical_json)
            .map_err(|_| invalid("canonical_json", "must be a JSON document"))?;
        let row = MapDerivationRecord {
            id: draft.scope.record(draft.kind, &draft.derivation_key),
            tenant: draft.scope.tenant.record_id(),
            work_context: draft.scope.work_context.record_id(),
            kind: draft.kind,
            derivation_key: draft.derivation_key,
            created_by: draft.created_by,
            created_at: draft.created_at,
            canonical_json: draft.canonical_json,
        };
        self.client()
            .query(include_str!(
                "queries/map_derivations/put_map_derivation.surql"
            ))
            .bind(("record", row.id.clone()))
            .bind(("row", row))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn map_derivation(
        &self,
        scope: MapDerivationScope,
        kind: MapDerivationKind,
        key: &str,
    ) -> Result<Option<MapDerivationRecord>, MapStoreError> {
        validate_key(kind, key)?;
        let mut response = self
            .client()
            .query(include_str!("queries/map_derivations/map_derivation.surql"))
            .bind(("record", scope.record(kind, key)))
            .bind(("tenant", scope.tenant.record_id()))
            .bind(("context", scope.work_context.record_id()))
            .bind(("kind", kind))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_derivations_page(
        &self,
        scope: MapDerivationScope,
        kind: MapDerivationKind,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapDerivationSummary>, MapStoreError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid("limit", "must be within 1..=101"));
        }
        if let Some(after) = after {
            validate_key(kind, after)?;
        }
        let sql = if after.is_some() {
            include_str!("queries/map_derivations/map_derivations_page.surql")
        } else {
            include_str!("queries/map_derivations/map_derivations_page_2.surql")
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.record_id()))
            .bind(("context", scope.work_context.record_id()))
            .bind(("kind", kind))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn complete_map_derivations(
        &self,
        scope: MapDerivationScope,
        kind: MapDerivationKind,
        needle: &str,
    ) -> Result<Vec<String>, MapStoreError> {
        if needle.len() > 512 || needle.chars().any(char::is_control) {
            return Err(invalid(
                "completion",
                "must be at most 512 bytes without control characters",
            ));
        }
        let mut response = self
            .client()
            .query(include_str!(
                "queries/map_derivations/complete_map_derivations.surql"
            ))
            .bind(("tenant", scope.tenant.record_id()))
            .bind(("context", scope.work_context.record_id()))
            .bind(("kind", kind))
            .bind(("needle", needle.to_lowercase()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn validate_key(kind: MapDerivationKind, key: &str) -> Result<(), MapStoreError> {
    let prefix = match kind {
        MapDerivationKind::Raster => "raster-derivation-",
        MapDerivationKind::Spatial => "spatial-derivation-",
    };
    if key
        .strip_prefix(prefix)
        .and_then(|id| uuid::Uuid::parse_str(id).ok())
        .filter(|id| matches!(id.get_version_num(), 5 | 7))
        .is_none()
    {
        return Err(invalid(
            "derivation_key",
            "must use its kind's prefix and a UUID",
        ));
    }
    Ok(())
}
fn invalid(field: &'static str, reason: &'static str) -> MapStoreError {
    MapStoreError::InvalidMapField { field, reason }
}
