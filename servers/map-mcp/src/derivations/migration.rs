//! Schema-10 import runs once while Map writers are drained, before service admission.
use anyhow::{Context, Result, ensure};
use duckdb::params;
use veoveo_platform_store::{MapDerivationDraft, MapDerivationKind, TenantId};

use crate::{
    analytics::MapAnalytics,
    catalog::MapCatalog,
    contract::{RasterDerivation, SpatialDerivation},
};

const BATCH_SIZE: usize = 16;
struct LegacyRow {
    tenant: String,
    context: String,
    principal: String,
    key: String,
    json: String,
}

impl MapCatalog {
    pub async fn migrate_local_derivations(&self, analytics: &MapAnalytics) -> Result<()> {
        let version: i64 = analytics.read_connection()?.query_row(
            "SELECT version FROM map_schema",
            [],
            |row| row.get(0),
        )?;
        if version == 11 {
            return Ok(());
        }
        ensure!(
            version == 10,
            "Map derivation transfer requires local schema 10"
        );
        // The old file is unchanged until every durable write succeeds. Restart
        // replays these pages; the Store rejects a conflicting immutable identity.
        for kind in [MapDerivationKind::Raster, MapDerivationKind::Spatial] {
            let mut after = None;
            loop {
                let rows = legacy_page(analytics, kind, after.as_ref())?;
                if rows.is_empty() {
                    break;
                }
                for row in rows {
                    let scope = self
                        .store()
                        .map_derivation_legacy_scope(row.tenant.parse::<TenantId>()?, &row.context)
                        .await?;
                    let created_at = match kind {
                        MapDerivationKind::Raster => {
                            let value: RasterDerivation = serde_json::from_str(&row.json)?;
                            value.validate()?;
                            ensure!(
                                value.derivation_id.as_str() == row.key
                                    && value.created_by.as_str() == row.principal
                                    && value.work_context.as_str() == row.context,
                                "legacy raster derivation metadata mismatch"
                            );
                            value.created_at
                        }
                        MapDerivationKind::Spatial => {
                            let value: SpatialDerivation = serde_json::from_str(&row.json)?;
                            value.validate()?;
                            ensure!(
                                value.derivation_id.as_str() == row.key
                                    && value.created_by.as_str() == row.principal
                                    && value.work_context.as_str() == row.context,
                                "legacy spatial derivation metadata mismatch"
                            );
                            value.created_at
                        }
                    };
                    self.store()
                        .put_map_derivation(MapDerivationDraft {
                            scope,
                            kind,
                            derivation_key: row.key.clone(),
                            created_by: row.principal,
                            created_at,
                            canonical_json: row.json,
                        })
                        .await
                        .with_context(|| {
                            format!("transferring Map {} derivation {}", kind.as_str(), row.key)
                        })?;
                    after = Some([row.tenant, row.context, row.key]);
                }
            }
        }
        let mut connection = analytics.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute_batch("DROP TABLE map_raster_derivation; DROP TABLE map_spatial_derivation; UPDATE map_schema SET version = 11;")?;
        transaction.commit()?;
        Ok(())
    }
}

fn legacy_page(
    analytics: &MapAnalytics,
    kind: MapDerivationKind,
    after: Option<&[String; 3]>,
) -> Result<Vec<LegacyRow>> {
    let table = match kind {
        MapDerivationKind::Raster => "map_raster_derivation",
        MapDerivationKind::Spatial => "map_spatial_derivation",
    };
    let connection = analytics.read_connection()?;
    let predicate = if after.is_some() {
        "WHERE (tenant_key, work_context_key, derivation_key) > (?, ?, ?)"
    } else {
        ""
    };
    let mut statement = connection.prepare(&format!("SELECT tenant_key, work_context_key, principal_key, derivation_key, canonical_json FROM {table} {predicate} ORDER BY tenant_key, work_context_key, derivation_key LIMIT {BATCH_SIZE}"))?;
    let mut rows = if let Some(after) = after {
        statement.query(params![after[0], after[1], after[2]])?
    } else {
        statement.query([])?
    };
    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(LegacyRow {
            tenant: row.get(0)?,
            context: row.get(1)?,
            principal: row.get(2)?,
            key: row.get(3)?,
            json: row.get(4)?,
        });
    }
    Ok(result)
}
