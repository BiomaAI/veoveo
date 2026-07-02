use anyhow::Result;
use duckdb::{OptionalExt, params};
use veoveo_mcp_contract::{GatewayControlPlaneRevision, GatewayControlPlaneRevisionSource};

use super::GatewayState;

impl GatewayState {
    pub fn record_control_plane_revision(
        &self,
        revision: &GatewayControlPlaneRevision,
    ) -> Result<()> {
        let revision_json = serde_json::to_string(revision)?;
        let source = match revision.source {
            GatewayControlPlaneRevisionSource::AdminApi => "admin_api",
            GatewayControlPlaneRevisionSource::MountedFileReload => "mounted_file_reload",
        };
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO gateway_control_plane_revisions (
                revision_id, sha256, source, applied_at, applied_by, tenant, revision_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                revision.revision_id.as_str(),
                revision.sha256.as_str(),
                source,
                revision.applied_at,
                revision.applied_by.as_str(),
                revision.tenant.as_ref().map(|value| value.as_str()),
                revision_json,
            ],
        )?;
        Ok(())
    }

    pub fn latest_control_plane_revision(&self) -> Result<Option<GatewayControlPlaneRevision>> {
        let conn = self.conn.lock();
        let revision_json: Option<String> = conn
            .query_row(
                r#"
                SELECT revision_json
                FROM gateway_control_plane_revisions
                ORDER BY applied_at DESC, revision_id DESC
                LIMIT 1
                "#,
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(revision_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }

    pub fn control_plane_revision_count(&self) -> Result<u64> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM gateway_control_plane_revisions",
            [],
            |row| row.get(0),
        )?;
        Ok(u64::try_from(count)?)
    }
}
