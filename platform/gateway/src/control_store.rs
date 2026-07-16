use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Serialize;
use veoveo_mcp_contract::{
    GatewayControlPlane, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, PrincipalId, TenantId,
};
use veoveo_platform_store::{
    GatewayControlActiveRecord, GatewayControlObjectContent, GatewayControlRevisionContent,
    GatewayControlRevisionRecord, GatewayControlRevisionSource, OpenObject, OutboxDraft,
    PlatformStore, RecordId, StoreConfig,
};

const ACTIVE_CONTROL_PLANE_RECORD: &str = "gateway_control_active:current";

pub fn new_gateway_control_plane_revision_id() -> Result<GatewayControlPlaneRevisionId> {
    GatewayControlPlaneRevisionId::new(format!("gcp-{}", uuid::Uuid::now_v7()))
        .context("failed to construct UUIDv7 gateway control-plane revision id")
}

#[derive(Debug, Clone)]
pub struct GatewayControlStore {
    platform: PlatformStore,
}

#[derive(Debug)]
struct ControlPlaneObjectRow {
    tenant: Option<String>,
    kind: &'static str,
    id: String,
    document: OpenObject,
}

impl GatewayControlStore {
    pub async fn connect(config: StoreConfig) -> Result<Self> {
        let platform = PlatformStore::connect(config)
            .await
            .context("failed to connect to the SurrealDB platform store")?;
        Ok(Self { platform })
    }

    pub async fn migrate(&self) -> Result<()> {
        self.platform
            .migrate()
            .await
            .context("failed to migrate the SurrealDB platform store")?;
        Ok(())
    }

    pub async fn load_active_revision(&self) -> Result<Option<GatewayControlPlaneRevision>> {
        let mut response = self
            .platform
            .client()
            .query(format!("SELECT * FROM ONLY {ACTIVE_CONTROL_PLANE_RECORD};"))
            .await
            .context("failed to load the active gateway control-plane pointer")?
            .check()
            .context("active gateway control-plane pointer query failed")?;
        let active: Option<GatewayControlActiveRecord> = response
            .take(0)
            .context("failed to decode the active gateway control-plane pointer")?;
        let Some(active) = active else {
            return Ok(None);
        };

        let mut response = self
            .platform
            .client()
            .query("SELECT * FROM ONLY $revision;")
            .bind(("revision", active.revision))
            .await
            .context("failed to load the active gateway control-plane revision")?
            .check()
            .context("active gateway control-plane revision query failed")?;
        let revision: Option<GatewayControlRevisionRecord> = response
            .take(0)
            .context("failed to decode the active gateway control-plane revision")?;
        revision
            .map(revision_from_record)
            .transpose()
            .context("active gateway control-plane revision is invalid")
    }

    pub async fn record_revision(&self, revision: &GatewayControlPlaneRevision) -> Result<()> {
        revision
            .control_plane
            .validate()
            .context("refusing to persist invalid gateway control plane")?;
        let revision_record =
            RecordId::new("gateway_control_revision", revision.revision_id.as_str());
        let revision_content = GatewayControlRevisionContent {
            revision_id: revision.revision_id.to_string(),
            sha256: revision.sha256.clone(),
            source: revision_source_to_store(revision.source),
            applied_at: revision.applied_at,
            applied_by: revision.applied_by.to_string(),
            tenant: revision.tenant.as_ref().map(ToString::to_string),
            control_plane: serialize_object(&revision.control_plane)
                .context("failed to serialize gateway control plane")?,
        };
        let object_rows = control_plane_object_rows(&revision.control_plane)?;
        let objects: Vec<_> = object_rows
            .into_iter()
            .map(|row| GatewayControlObjectContent {
                revision: revision_record.clone(),
                tenant: row.tenant,
                object_kind: row.kind.to_owned(),
                object_id: row.id,
                document: row.document,
            })
            .collect();
        let outbox = OutboxDraft::now(
            None,
            "gateway_control_plane",
            revision.revision_id.to_string(),
            "gateway.control_plane.activated",
            1,
            OpenObject::new(BTreeMap::from([
                (
                    "revision_id".to_owned(),
                    serde_json::Value::String(revision.revision_id.to_string()),
                ),
                (
                    "sha256".to_owned(),
                    serde_json::Value::String(revision.sha256.clone()),
                ),
                (
                    "tenant".to_owned(),
                    revision
                        .tenant
                        .as_ref()
                        .map(|tenant| serde_json::Value::String(tenant.to_string()))
                        .unwrap_or(serde_json::Value::Null),
                ),
            ])),
        );

        self.platform
            .client()
            .query(
                r#"
                BEGIN TRANSACTION;
                CREATE ONLY $revision_record CONTENT $revision;
                FOR $object IN $objects {
                    CREATE gateway_control_object CONTENT $object;
                };
                UPSERT ONLY gateway_control_active:current CONTENT {
                    revision: $revision_record,
                    revision_id: $revision_id,
                    updated_at: $applied_at
                };
                CREATE outbox_event CONTENT $outbox;
                COMMIT TRANSACTION;
                "#,
            )
            .bind(("revision_record", revision_record))
            .bind(("revision", revision_content))
            .bind(("revision_id", revision.revision_id.as_str()))
            .bind(("applied_at", revision.applied_at))
            .bind(("objects", objects))
            .bind(("outbox", outbox))
            .await
            .context("failed to publish gateway control-plane revision")?
            .check()
            .context("gateway control-plane publication transaction failed")?;
        Ok(())
    }

    pub async fn revision_count(&self) -> Result<u64> {
        count_query(
            &self.platform,
            "SELECT VALUE count FROM (SELECT count() AS count FROM gateway_control_revision GROUP ALL);",
            None,
        )
        .await
        .context("failed to count gateway control-plane revisions")
    }

    pub async fn object_count_for_active_revision(&self) -> Result<u64> {
        let mut response = self
            .platform
            .client()
            .query(format!("SELECT * FROM ONLY {ACTIVE_CONTROL_PLANE_RECORD};"))
            .await
            .context("failed to load active gateway control-plane pointer for object count")?
            .check()?;
        let active: Option<GatewayControlActiveRecord> = response.take(0)?;
        let Some(active) = active else {
            return Ok(0);
        };
        count_query(
            &self.platform,
            "SELECT VALUE count FROM (SELECT count() AS count FROM gateway_control_object WHERE revision = $revision GROUP ALL);",
            Some(active.revision),
        )
        .await
        .context("failed to count active gateway control-plane objects")
    }

    pub fn platform_store(&self) -> &PlatformStore {
        &self.platform
    }
}

async fn count_query(
    platform: &PlatformStore,
    statement: &str,
    revision: Option<RecordId>,
) -> Result<u64> {
    let query = platform.client().query(statement);
    let mut response = match revision {
        Some(revision) => query.bind(("revision", revision)).await?,
        None => query.await?,
    }
    .check()?;
    let counts: Vec<i64> = response.take(0)?;
    let count = counts.first().copied().unwrap_or(0);
    u64::try_from(count).context("SurrealDB returned a negative count")
}

fn revision_from_record(
    record: GatewayControlRevisionRecord,
) -> Result<GatewayControlPlaneRevision> {
    let control_plane: GatewayControlPlane = serde_json::from_value(
        serde_json::to_value(record.control_plane)
            .context("failed to convert stored control plane to JSON")?,
    )
    .context("failed to deserialize stored gateway control plane")?;
    control_plane
        .validate()
        .context("stored gateway control plane failed validation")?;

    Ok(GatewayControlPlaneRevision {
        revision_id: GatewayControlPlaneRevisionId::new(record.revision_id)?,
        sha256: record.sha256,
        source: revision_source_from_store(record.source),
        applied_at: record.applied_at,
        applied_by: PrincipalId::new(record.applied_by)?,
        tenant: record.tenant.map(TenantId::new).transpose()?,
        control_plane,
    })
}

fn revision_source_to_store(
    source: GatewayControlPlaneRevisionSource,
) -> GatewayControlRevisionSource {
    match source {
        GatewayControlPlaneRevisionSource::AdminApi => GatewayControlRevisionSource::AdminApi,
        GatewayControlPlaneRevisionSource::SeedFile => GatewayControlRevisionSource::SeedFile,
    }
}

fn revision_source_from_store(
    source: GatewayControlRevisionSource,
) -> GatewayControlPlaneRevisionSource {
    match source {
        GatewayControlRevisionSource::AdminApi => GatewayControlPlaneRevisionSource::AdminApi,
        GatewayControlRevisionSource::SeedFile => GatewayControlPlaneRevisionSource::SeedFile,
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
    for resource in &control_plane.recording_ingest_resources {
        rows.push(object_row(
            None,
            "recording_ingest_resource",
            resource.id.as_str(),
            resource,
        )?);
        for producer in &resource.producers {
            rows.push(object_row(
                Some(producer.tenant.as_str().to_owned()),
                "recording_producer",
                producer.id.as_str(),
                producer,
            )?);
        }
    }
    for tenant in &control_plane.tenants {
        rows.push(object_row(
            Some(tenant.id.as_str().to_owned()),
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
        document: serialize_object(value)?,
    })
}

fn serialize_object(value: impl Serialize) -> Result<OpenObject> {
    let value = serde_json::to_value(value)?;
    let serde_json::Value::Object(object) = value else {
        anyhow::bail!("gateway control-plane object did not serialize as an object");
    };
    Ok(OpenObject::new(object.into_iter().collect()))
}

fn single_tenant<'a>(mut tenants: impl Iterator<Item = &'a str>) -> Option<String> {
    let tenant = tenants.next()?;
    if tenants.next().is_none() {
        Some(tenant.to_owned())
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
            revision_source_from_store(revision_source_to_store(
                GatewayControlPlaneRevisionSource::SeedFile
            )),
            GatewayControlPlaneRevisionSource::SeedFile
        );
    }

    #[test]
    fn generated_revision_ids_use_uuid_v7() {
        let id = new_gateway_control_plane_revision_id().unwrap();
        let uuid = uuid::Uuid::parse_str(id.as_str().strip_prefix("gcp-").unwrap()).unwrap();
        assert_eq!(uuid.get_version_num(), 7);
    }

    #[test]
    fn control_plane_object_rows_include_queryable_top_level_objects() {
        let tenant_id = TenantId::new("tenant-fixture").unwrap();
        let control_plane = GatewayControlPlane {
            branding: None,
            identity_providers: Vec::new(),
            authorization_servers: Vec::new(),
            servers: Vec::new(),
            profiles: Vec::new(),
            recording_ingest_resources: Vec::new(),
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
