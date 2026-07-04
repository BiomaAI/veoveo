use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, PrincipalId, TenantId,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct GatewayControlDb {
    pool: PgPool,
}

#[derive(Debug)]
struct ControlPlaneObjectRow {
    tenant: Option<String>,
    kind: &'static str,
    id: String,
    json: Value,
}

impl GatewayControlDb {
    pub async fn connect(database_url: impl AsRef<str>) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url.as_ref())
            .await
            .context("failed to connect to gateway control-plane Postgres")?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        MIGRATOR
            .run(&self.pool)
            .await
            .context("failed to migrate gateway control-plane Postgres")?;
        Ok(())
    }

    pub async fn load_active_revision(&self) -> Result<Option<GatewayControlPlaneRevision>> {
        let row = sqlx::query(
            r#"
            SELECT
                r.revision_id,
                r.sha256,
                r.source,
                r.applied_at,
                r.applied_by,
                r.tenant,
                r.control_plane_json
            FROM gateway_control_plane_active a
            JOIN gateway_control_plane_revisions r ON r.revision_id = a.revision_id
            WHERE a.singleton = TRUE
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .context("failed to load active gateway control-plane revision")?;

        row.map(revision_from_row).transpose()
    }

    pub async fn record_revision(&self, revision: &GatewayControlPlaneRevision) -> Result<()> {
        revision
            .control_plane
            .validate()
            .context("refusing to persist invalid gateway control plane")?;
        let control_plane_json = serde_json::to_value(&revision.control_plane)
            .context("failed to serialize gateway control plane")?;
        let object_rows = control_plane_object_rows(&revision.control_plane)?;

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to start gateway control-plane transaction")?;
        sqlx::query(
            r#"
            INSERT INTO gateway_control_plane_revisions (
                revision_id, sha256, source, applied_at, applied_by, tenant, control_plane_json
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(revision.revision_id.as_str())
        .bind(revision.sha256.as_str())
        .bind(revision_source_to_str(revision.source))
        .bind(revision.applied_at)
        .bind(revision.applied_by.as_str())
        .bind(revision.tenant.as_ref().map(TenantId::as_str))
        .bind(control_plane_json)
        .execute(&mut *tx)
        .await
        .context("failed to insert gateway control-plane revision")?;

        for row in object_rows {
            sqlx::query(
                r#"
                INSERT INTO gateway_control_plane_objects (
                    revision_id, tenant, object_kind, object_id, object_json
                ) VALUES ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(revision.revision_id.as_str())
            .bind(row.tenant.as_deref())
            .bind(row.kind)
            .bind(row.id.as_str())
            .bind(row.json)
            .execute(&mut *tx)
            .await
            .with_context(|| {
                format!(
                    "failed to insert gateway control-plane object {}:{}",
                    row.kind, row.id
                )
            })?;
        }

        sqlx::query(
            r#"
            INSERT INTO gateway_control_plane_active (singleton, revision_id)
            VALUES (TRUE, $1)
            ON CONFLICT (singleton)
            DO UPDATE SET revision_id = EXCLUDED.revision_id
            "#,
        )
        .bind(revision.revision_id.as_str())
        .execute(&mut *tx)
        .await
        .context("failed to update active gateway control-plane revision")?;

        tx.commit()
            .await
            .context("failed to commit gateway control-plane revision")?;
        Ok(())
    }

    pub async fn revision_count(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gateway_control_plane_revisions")
            .fetch_one(&self.pool)
            .await
            .context("failed to count gateway control-plane revisions")?;
        Ok(u64::try_from(count)?)
    }

    pub async fn object_count_for_active_revision(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM gateway_control_plane_objects o
            JOIN gateway_control_plane_active a ON a.revision_id = o.revision_id
            WHERE a.singleton = TRUE
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .context("failed to count active gateway control-plane objects")?;
        Ok(u64::try_from(count)?)
    }
}

fn revision_from_row(row: sqlx::postgres::PgRow) -> Result<GatewayControlPlaneRevision> {
    let revision_id: String = row.try_get("revision_id")?;
    let source: String = row.try_get("source")?;
    let applied_by: String = row.try_get("applied_by")?;
    let tenant: Option<String> = row.try_get("tenant")?;
    let control_plane_json: Value = row.try_get("control_plane_json")?;
    let control_plane: GatewayControlPlane = serde_json::from_value(control_plane_json)
        .context("failed to deserialize active gateway control plane")?;
    control_plane
        .validate()
        .context("active gateway control plane is invalid")?;

    Ok(GatewayControlPlaneRevision {
        revision_id: GatewayControlPlaneRevisionId::new(revision_id)?,
        sha256: row.try_get("sha256")?,
        source: revision_source_from_str(&source)?,
        applied_at: row.try_get("applied_at")?,
        applied_by: PrincipalId::new(applied_by)?,
        tenant: tenant.map(TenantId::new).transpose()?,
        control_plane,
    })
}

fn revision_source_to_str(source: GatewayControlPlaneRevisionSource) -> &'static str {
    match source {
        GatewayControlPlaneRevisionSource::AdminApi => "admin_api",
        GatewayControlPlaneRevisionSource::MountedFileReload => "mounted_file_reload",
        GatewayControlPlaneRevisionSource::SeedFile => "seed_file",
    }
}

fn revision_source_from_str(value: &str) -> Result<GatewayControlPlaneRevisionSource> {
    match value {
        "admin_api" => Ok(GatewayControlPlaneRevisionSource::AdminApi),
        "mounted_file_reload" => Ok(GatewayControlPlaneRevisionSource::MountedFileReload),
        "seed_file" => Ok(GatewayControlPlaneRevisionSource::SeedFile),
        _ => Err(anyhow!(
            "unknown gateway control-plane revision source `{value}`"
        )),
    }
}

fn control_plane_object_rows(
    control_plane: &GatewayControlPlane,
) -> Result<Vec<ControlPlaneObjectRow>> {
    let mut rows = Vec::new();
    for identity_provider in &control_plane.identity_providers {
        rows.push(object_row(
            None,
            "identity_provider",
            identity_provider.id.as_str(),
            identity_provider,
        )?);
    }
    for authorization_server in &control_plane.authorization_servers {
        rows.push(object_row(
            None,
            "authorization_server",
            authorization_server.id.as_str(),
            authorization_server,
        )?);
    }
    for server in &control_plane.servers {
        rows.push(object_row(None, "server", server.slug.as_str(), server)?);
    }
    for profile in &control_plane.profiles {
        rows.push(object_row(None, "profile", profile.id.as_str(), profile)?);
    }
    for tenant in &control_plane.tenants {
        rows.push(object_row(
            Some(tenant.id.as_str().to_string()),
            "tenant",
            tenant.id.as_str(),
            tenant,
        )?);
    }
    for policy in &control_plane.policies {
        rows.push(object_row(None, "policy", policy.version.as_str(), policy)?);
        for rule in &policy.rules {
            let tenant = single_tenant(rule.tenant_ids.iter().map(TenantId::as_str));
            rows.push(object_row(
                tenant,
                "policy_rule",
                format!("{}/{}", policy.version, rule.id),
                rule,
            )?);
        }
    }
    for data_label in &control_plane.data_labels {
        rows.push(object_row(
            None,
            "data_label",
            data_label.id.as_str(),
            data_label,
        )?);
    }
    for client in &control_plane.oauth_clients {
        rows.push(object_row(
            None,
            "oauth_client",
            client.id.as_str(),
            client,
        )?);
    }
    for client in &control_plane.oidc_clients {
        rows.push(object_row(None, "oidc_client", client.id.as_str(), client)?);
    }
    for secret in &control_plane.secrets {
        rows.push(object_row(None, "secret", secret.id.as_str(), secret)?);
    }
    Ok(rows)
}

fn object_row(
    tenant: Option<String>,
    kind: &'static str,
    id: impl Into<String>,
    value: impl Serialize,
) -> Result<ControlPlaneObjectRow> {
    Ok(ControlPlaneObjectRow {
        tenant,
        kind,
        id: id.into(),
        json: serde_json::to_value(value)?,
    })
}

fn single_tenant<'a>(mut tenants: impl Iterator<Item = &'a str>) -> Option<String> {
    let tenant = tenants.next()?;
    if tenants.next().is_none() {
        Some(tenant.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use veoveo_mcp_contract::{
        GatewayAction, PolicyEffect, PolicyRule, PolicyRuleId, PolicySet, PolicyVersion,
        TenantDefinition,
    };

    #[test]
    fn revision_source_round_trips_seed_file() {
        assert_eq!(
            revision_source_from_str(revision_source_to_str(
                GatewayControlPlaneRevisionSource::SeedFile
            ))
            .unwrap(),
            GatewayControlPlaneRevisionSource::SeedFile
        );
    }

    #[test]
    fn control_plane_object_rows_include_queryable_top_level_objects() {
        let tenant_id = TenantId::new("tenant-fixture").unwrap();
        let control_plane = GatewayControlPlane {
            identity_providers: Vec::new(),
            authorization_servers: Vec::new(),
            servers: Vec::new(),
            profiles: Vec::new(),
            tenants: vec![TenantDefinition {
                id: tenant_id.clone(),
                title: None,
                description: None,
                metadata: serde_json::json!({}),
            }],
            policies: vec![PolicySet {
                version: PolicyVersion::new("policy-fixture").unwrap(),
                rules: vec![PolicyRule {
                    id: PolicyRuleId::new("allow-fixture").unwrap(),
                    effect: PolicyEffect::Allow,
                    actions: BTreeSet::from([GatewayAction::ToolsCall]),
                    profiles: BTreeSet::new(),
                    servers: BTreeSet::new(),
                    tools: BTreeSet::new(),
                    resource_schemes: BTreeSet::new(),
                    prompts: BTreeSet::new(),
                    principal_ids: BTreeSet::new(),
                    tenant_ids: BTreeSet::from([tenant_id.clone()]),
                    groups: BTreeSet::new(),
                    roles: BTreeSet::new(),
                    required_scopes: BTreeSet::new(),
                    required_data_labels: BTreeSet::new(),
                    required_assurances: BTreeSet::new(),
                    metadata: serde_json::json!({}),
                }],
                metadata: serde_json::json!({}),
            }],
            data_labels: Vec::new(),
            oauth_clients: Vec::new(),
            oidc_clients: Vec::new(),
            secrets: Vec::new(),
            metadata: serde_json::json!({}),
        };
        let rows = control_plane_object_rows(&control_plane).unwrap();

        assert!(
            rows.iter()
                .any(|row| row.kind == "tenant" && row.id == "tenant-fixture")
        );
        assert!(
            rows.iter()
                .any(|row| row.kind == "policy" && row.id == "policy-fixture")
        );
        let rule = rows
            .iter()
            .find(|row| row.kind == "policy_rule" && row.id == "policy-fixture/allow-fixture")
            .expect("policy rule row");
        assert_eq!(rule.tenant.as_deref(), Some("tenant-fixture"));
    }
}
