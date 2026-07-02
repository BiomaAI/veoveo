use anyhow::Result;
use duckdb::{OptionalExt, params};
use veoveo_mcp_contract::{
    GatewayProfileId, GatewayTaskId, GatewayTaskMapping, PrincipalId, ServerSlug, UpstreamTaskId,
};

use super::GatewayState;

impl GatewayState {
    pub fn record_task_mapping(&self, mapping: &GatewayTaskMapping) -> Result<()> {
        let mapping_json = serde_json::to_string(mapping)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO gateway_task_mappings (
                gateway_task_id, upstream_server, upstream_task_id, profile, owner,
                created_at, updated_at, mapping_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(gateway_task_id) DO UPDATE SET
                upstream_server = excluded.upstream_server,
                upstream_task_id = excluded.upstream_task_id,
                profile = excluded.profile,
                owner = excluded.owner,
                updated_at = excluded.updated_at,
                mapping_json = excluded.mapping_json
            "#,
            params![
                mapping.gateway_task_id.as_str(),
                mapping.upstream_server.as_str(),
                mapping.upstream_task_id.as_str(),
                mapping.profile.as_str(),
                mapping.owner.as_str(),
                mapping.created_at,
                mapping.updated_at,
                mapping_json,
            ],
        )?;
        Ok(())
    }

    pub fn task_mapping(
        &self,
        gateway_task_id: &GatewayTaskId,
    ) -> Result<Option<GatewayTaskMapping>> {
        self.query_mapping(
            "SELECT mapping_json FROM gateway_task_mappings WHERE gateway_task_id = ?1",
            params![gateway_task_id.as_str()],
        )
    }

    pub fn task_mapping_by_upstream(
        &self,
        upstream_server: &ServerSlug,
        upstream_task_id: &UpstreamTaskId,
    ) -> Result<Option<GatewayTaskMapping>> {
        self.query_mapping(
            "SELECT mapping_json FROM gateway_task_mappings WHERE upstream_server = ?1 AND upstream_task_id = ?2",
            params![upstream_server.as_str(), upstream_task_id.as_str()],
        )
    }

    pub fn task_mappings_for_owner(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
        upstream_server: &ServerSlug,
    ) -> Result<Vec<GatewayTaskMapping>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT mapping_json
            FROM gateway_task_mappings
            WHERE profile = ?1 AND owner = ?2 AND upstream_server = ?3
            ORDER BY updated_at
            "#,
        )?;
        let rows = stmt.query_map(
            params![profile.as_str(), owner.as_str(), upstream_server.as_str()],
            |row| row.get::<_, String>(0),
        )?;
        let mut mappings = Vec::new();
        for row in rows {
            mappings.push(serde_json::from_str(&row?)?);
        }
        Ok(mappings)
    }

    pub fn task_mappings_for_profile_owner(
        &self,
        profile: &GatewayProfileId,
        owner: &PrincipalId,
    ) -> Result<Vec<GatewayTaskMapping>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
            SELECT mapping_json
            FROM gateway_task_mappings
            WHERE profile = ?1 AND owner = ?2
            ORDER BY updated_at, gateway_task_id
            "#,
        )?;
        let rows = stmt.query_map(params![profile.as_str(), owner.as_str()], |row| {
            row.get::<_, String>(0)
        })?;
        let mut mappings = Vec::new();
        for row in rows {
            mappings.push(serde_json::from_str(&row?)?);
        }
        Ok(mappings)
    }

    fn query_mapping<P>(&self, sql: &str, params: P) -> Result<Option<GatewayTaskMapping>>
    where
        P: duckdb::Params,
    {
        let conn = self.conn.lock();
        let mapping_json = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()?;
        Ok(mapping_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }
}
