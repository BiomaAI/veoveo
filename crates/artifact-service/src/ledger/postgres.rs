//! Postgres implementation of the grant ledger (sqlx runtime queries).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use veoveo_mcp_contract::access::{ArtifactSha256, Grant, Subject};
use veoveo_mcp_contract::gateway::{DataLabelId, PrincipalId, TenantId};

use super::{
    GrantLedger, LedgerError, StoredArtifact, labels_from_json, labels_to_json, level_str,
    parse_level, parse_subject, rebuild_metadata, subject_parts,
};

/// The authoritative grant ledger, backed by Postgres.
#[derive(Clone)]
pub struct PostgresGrantLedger {
    pool: PgPool,
}

impl PostgresGrantLedger {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, LedgerError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .map_err(|e| LedgerError::Backend(e.to_string()))?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Apply schema migrations.
    pub async fn migrate(&self) -> Result<(), LedgerError> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .map_err(|e| LedgerError::Backend(e.to_string()))
    }

    async fn grants_for(&self, sha: &ArtifactSha256) -> Result<Vec<Grant>, LedgerError> {
        let rows = sqlx::query(
            "select subject_kind, subject_id, level, data_labels, retention_expires_at, tenant_id \
             from artifact_grants where sha256 = $1 order by subject_kind, subject_id",
        )
        .bind(sha.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| LedgerError::Backend(e.to_string()))?;

        rows.into_iter()
            .map(|row| {
                let kind: String = row.try_get("subject_kind").map_err(backend)?;
                let id: String = row.try_get("subject_id").map_err(backend)?;
                let level: String = row.try_get("level").map_err(backend)?;
                let labels_json: Value = row.try_get("data_labels").map_err(backend)?;
                let retention: Option<DateTime<Utc>> =
                    row.try_get("retention_expires_at").map_err(backend)?;
                let tenant: String = row.try_get("tenant_id").map_err(backend)?;
                Ok(Grant {
                    artifact: sha.clone(),
                    subject: parse_subject(&kind, &id)?,
                    level: parse_level(&level)?,
                    tenant: TenantId::new(tenant)
                        .map_err(|e| LedgerError::Corrupt(e.to_string()))?,
                    data_labels: labels_from_json(&labels_json)?,
                    retention_expires_at: retention,
                })
            })
            .collect()
    }
}

fn backend(e: sqlx::Error) -> LedgerError {
    LedgerError::Backend(e.to_string())
}

impl GrantLedger for PostgresGrantLedger {
    async fn insert_artifact(&self, stored: StoredArtifact) -> Result<(), LedgerError> {
        let mut tx = self.pool.begin().await.map_err(backend)?;

        let compliance = &stored.metadata.compliance;
        let owner = compliance
            .owner_id
            .as_ref()
            .ok_or_else(|| LedgerError::Conflict("artifact has no owner".to_string()))?;

        // Dedup within tenant: identical sha is a no-op.
        let inserted = sqlx::query(
            "insert into artifacts \
               (sha256, tenant_id, byte_len, mime_type, filename, classification, owner_id, \
                data_labels, retention_expires_at, metadata, created_at) \
             values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
             on conflict (sha256) do nothing",
        )
        .bind(&stored.metadata.sha256)
        .bind(stored.tenant.as_str())
        .bind(stored.metadata.byte_len as i64)
        .bind(&stored.metadata.mime_type)
        .bind(&stored.metadata.filename)
        .bind(compliance.classification.as_ref().map(|c| c.as_str()))
        .bind(owner.as_str())
        .bind(labels_to_json(&stored.labels))
        .bind(compliance.retention_expires_at)
        .bind(&stored.metadata.metadata)
        .bind(stored.metadata.created_at)
        .execute(&mut *tx)
        .await
        .map_err(backend)?;

        if inserted.rows_affected() == 0 {
            // Already existed; leave existing grants untouched.
            tx.commit().await.map_err(backend)?;
            return Ok(());
        }

        for grant in &stored.grants {
            let (kind, id) = subject_parts(&grant.subject);
            sqlx::query(
                "insert into artifact_grants \
                   (sha256, subject_kind, subject_id, level, data_labels, retention_expires_at, tenant_id) \
                 values ($1,$2,$3,$4,$5,$6,$7) \
                 on conflict (sha256, subject_kind, subject_id) do nothing",
            )
            .bind(grant.artifact.as_str())
            .bind(kind)
            .bind(&id)
            .bind(level_str(grant.level))
            .bind(labels_to_json(&grant.data_labels))
            .bind(grant.retention_expires_at)
            .bind(grant.tenant.as_str())
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        }

        tx.commit().await.map_err(backend)?;
        Ok(())
    }

    async fn get_artifact(
        &self,
        sha: &ArtifactSha256,
    ) -> Result<Option<StoredArtifact>, LedgerError> {
        let row = sqlx::query(
            "select tenant_id, byte_len, mime_type, filename, classification, owner_id, \
                    data_labels, retention_expires_at, metadata, created_at \
             from artifacts where sha256 = $1",
        )
        .bind(sha.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(backend)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let tenant_s: String = row.try_get("tenant_id").map_err(backend)?;
        let tenant = TenantId::new(tenant_s).map_err(|e| LedgerError::Corrupt(e.to_string()))?;
        let byte_len: i64 = row.try_get("byte_len").map_err(backend)?;
        let mime_type: Option<String> = row.try_get("mime_type").map_err(backend)?;
        let filename: Option<String> = row.try_get("filename").map_err(backend)?;
        let classification: Option<String> = row.try_get("classification").map_err(backend)?;
        let classification = classification
            .map(DataLabelId::new)
            .transpose()
            .map_err(|e| LedgerError::Corrupt(e.to_string()))?;
        let owner_s: String = row.try_get("owner_id").map_err(backend)?;
        let owner = PrincipalId::new(owner_s).map_err(|e| LedgerError::Corrupt(e.to_string()))?;
        let labels_json: Value = row.try_get("data_labels").map_err(backend)?;
        let labels = labels_from_json(&labels_json)?;
        let retention: Option<DateTime<Utc>> =
            row.try_get("retention_expires_at").map_err(backend)?;
        let metadata: Value = row.try_get("metadata").map_err(backend)?;
        let created_at: DateTime<Utc> = row.try_get("created_at").map_err(backend)?;

        let metadata = rebuild_metadata(
            sha,
            byte_len,
            mime_type,
            filename,
            classification,
            &tenant,
            &owner,
            &labels,
            retention,
            metadata,
            created_at,
        );
        let grants = self.grants_for(sha).await?;
        Ok(Some(StoredArtifact {
            metadata,
            tenant,
            labels,
            grants,
        }))
    }

    async fn upsert_grant(&self, grant: Grant) -> Result<(), LedgerError> {
        let (kind, id) = subject_parts(&grant.subject);
        sqlx::query(
            "insert into artifact_grants \
               (sha256, subject_kind, subject_id, level, data_labels, retention_expires_at, tenant_id) \
             values ($1,$2,$3,$4,$5,$6,$7) \
             on conflict (sha256, subject_kind, subject_id) \
             do update set level = excluded.level, data_labels = excluded.data_labels, \
                           retention_expires_at = excluded.retention_expires_at",
        )
        .bind(grant.artifact.as_str())
        .bind(kind)
        .bind(&id)
        .bind(level_str(grant.level))
        .bind(labels_to_json(&grant.data_labels))
        .bind(grant.retention_expires_at)
        .bind(grant.tenant.as_str())
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn remove_grant(
        &self,
        sha: &ArtifactSha256,
        subject: &Subject,
    ) -> Result<(), LedgerError> {
        let (kind, id) = subject_parts(subject);
        sqlx::query(
            "delete from artifact_grants where sha256 = $1 and subject_kind = $2 and subject_id = $3",
        )
        .bind(sha.as_str())
        .bind(kind)
        .bind(&id)
        .execute(&self.pool)
        .await
        .map_err(backend)?;
        Ok(())
    }

    async fn list_grants(&self, sha: &ArtifactSha256) -> Result<Vec<Grant>, LedgerError> {
        self.grants_for(sha).await
    }

    async fn delete_artifact(&self, sha: &ArtifactSha256) -> Result<(), LedgerError> {
        sqlx::query("delete from artifacts where sha256 = $1")
            .bind(sha.as_str())
            .execute(&self.pool)
            .await
            .map_err(backend)?;
        Ok(())
    }
}
