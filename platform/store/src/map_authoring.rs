use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{Array, RecordId, SurrealValue};

use crate::{
    ArtifactGrantSubjectKind, InvocationAuthorityRecord, MapFeatureChangeSetRecord,
    MapFeatureHeadRecord, MapFeatureLayerRecord, MapFeatureRevisionRecord,
    MapFeatureSchemaRevisionRecord, MapLayerPublicationRecord, MapStyleRevisionRecord, OpenObject,
    OutboxDraft, PlatformIdentity, PlatformStore, StoreError, TenantId,
    deterministic_work_context_id,
};

const MAX_AUTHORING_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_FEATURES_PER_CHANGESET: usize = 10_000;

#[derive(Clone, Debug)]
pub struct MapFeatureSchemaDraft {
    pub schema_revision_key: String,
    pub schema_version: i64,
    pub digest_sha256: String,
    pub schema_json: String,
}

#[derive(Clone, Debug)]
pub struct MapStyleRevisionDraft {
    pub style_revision_key: String,
    pub style_version: i64,
    pub style_json: String,
}

#[derive(Clone, Debug)]
pub struct MapFeatureLayerDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub layer_key: String,
    pub title: String,
    pub description: Option<String>,
    pub content_class: String,
    pub schema: MapFeatureSchemaDraft,
    pub style: Option<MapStyleRevisionDraft>,
    pub revision: i64,
    pub archived_at: Option<DateTime<Utc>>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapFeatureLayerUpdateDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub layer_key: String,
    pub title: String,
    pub description: Option<String>,
    pub schema_version: i64,
    pub schema_revision_key: String,
    pub new_schema: Option<MapFeatureSchemaDraft>,
    pub style_version: Option<i64>,
    pub style_revision_key: Option<String>,
    pub new_style: Option<MapStyleRevisionDraft>,
    pub revision: i64,
    pub archived_at: Option<DateTime<Utc>>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapFeatureRevisionDraft {
    pub feature_key: String,
    pub feature_revision: i64,
    pub layer_revision: i64,
    pub schema_version: i64,
    pub deleted: bool,
    pub geometry_type: String,
    pub geometry_json: String,
    pub bbox_west: f64,
    pub bbox_south: f64,
    pub bbox_east: f64,
    pub bbox_north: f64,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub semantic_type: String,
    pub title: Option<String>,
    pub canonical_json: String,
    pub expected_feature_revision: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct MapFeatureCommitDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub layer_key: String,
    pub layer_canonical_json: String,
    pub expected_layer_revision: i64,
    pub changeset_key: String,
    pub idempotency_key: String,
    pub request_digest_sha256: String,
    pub changeset_canonical_json: String,
    pub revisions: Vec<MapFeatureRevisionDraft>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapFeatureCommitResult {
    pub changeset: MapFeatureChangeSetRecord,
    pub revisions: Vec<MapFeatureRevisionRecord>,
}

#[derive(Clone, Debug)]
pub struct MapLayerPublicationDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub publication_key: String,
    pub layer_key: String,
    pub layer_revision: i64,
    pub schema_version: i64,
    pub style_revision_key: Option<String>,
    pub artifact_uris: Vec<String>,
    pub canonical_json: String,
    pub published_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct LayerContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    authority: InvocationAuthorityRecord,
    created_by_key: String,
    owner_kind: ArtifactGrantSubjectKind,
    owner_key: String,
    layer_key: String,
    title: String,
    description: Option<String>,
    content_class: String,
    schema_version: i64,
    schema_revision_key: String,
    style_version: Option<i64>,
    style_revision_key: Option<String>,
    revision: i64,
    classification: Option<String>,
    data_labels: Vec<String>,
    archived_at: Option<DateTime<Utc>>,
    canonical_json: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct SchemaContent {
    tenant: RecordId,
    work_context: RecordId,
    layer_key: String,
    schema_revision_key: String,
    schema_version: i64,
    digest_sha256: String,
    schema_json: String,
    created_by: RecordId,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct StyleContent {
    tenant: RecordId,
    work_context: RecordId,
    layer_key: String,
    style_revision_key: String,
    style_version: i64,
    style_json: String,
    created_by: RecordId,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RevisionContent {
    tenant: RecordId,
    work_context: RecordId,
    layer_key: String,
    feature_key: String,
    feature_revision: i64,
    layer_revision: i64,
    schema_version: i64,
    changeset_key: String,
    deleted: bool,
    geometry_type: String,
    geometry_json: String,
    bbox_west: f64,
    bbox_south: f64,
    bbox_east: f64,
    bbox_north: f64,
    valid_from: Option<DateTime<Utc>>,
    valid_until: Option<DateTime<Utc>>,
    semantic_type: String,
    title: Option<String>,
    canonical_json: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct HeadContent {
    tenant: RecordId,
    work_context: RecordId,
    layer_key: String,
    feature_key: String,
    feature_revision: i64,
    layer_revision: i64,
    schema_version: i64,
    changeset_key: String,
    deleted: bool,
    geometry_type: String,
    geometry_json: String,
    bbox_west: f64,
    bbox_south: f64,
    bbox_east: f64,
    bbox_north: f64,
    valid_from: Option<DateTime<Utc>>,
    valid_until: Option<DateTime<Utc>>,
    semantic_type: String,
    title: Option<String>,
    canonical_json: String,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct MutationContent {
    create: bool,
    head_record: RecordId,
    revision_record: RecordId,
    expected_feature_revision: Option<i64>,
    head: HeadContent,
    revision: RevisionContent,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ChangeSetContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    actor_key: String,
    work_context_key: String,
    authority: InvocationAuthorityRecord,
    layer_key: String,
    changeset_key: String,
    base_layer_revision: i64,
    resulting_layer_revision: i64,
    feature_keys: Vec<String>,
    idempotency_key: String,
    request_digest_sha256: String,
    commit_sequence: i64,
    canonical_json: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct PublicationContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    published_by_key: String,
    work_context_key: String,
    authority: InvocationAuthorityRecord,
    publication_key: String,
    layer_key: String,
    layer_revision: i64,
    schema_version: i64,
    style_revision_key: Option<String>,
    artifact_uris: Vec<String>,
    canonical_json: String,
    published_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn create_map_feature_layer(
        &self,
        draft: MapFeatureLayerDraft,
    ) -> Result<MapFeatureLayerRecord, StoreError> {
        validate_layer_draft(&draft)?;
        let now = Utc::now();
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let layer_record = authored_record(
            "map_feature_layer",
            &draft.identity.tenant_key,
            &[&draft.layer_key],
        );
        let schema_record = authored_version_record(
            "map_feature_schema_revision",
            &draft.identity.tenant_key,
            &draft.layer_key,
            draft.schema.schema_version,
        );
        let content = LayerContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: work_context.clone(),
            authority: draft.authority.clone(),
            created_by_key: draft.identity.principal_key.clone(),
            owner_kind: draft.authority.owner_kind,
            owner_key: draft.authority.owner_key.clone(),
            layer_key: draft.layer_key.clone(),
            title: draft.title,
            description: draft.description,
            content_class: draft.content_class,
            schema_version: draft.schema.schema_version,
            schema_revision_key: draft.schema.schema_revision_key.clone(),
            style_version: draft.style.as_ref().map(|style| style.style_version),
            style_revision_key: draft
                .style
                .as_ref()
                .map(|style| style.style_revision_key.clone()),
            revision: draft.revision,
            classification: draft.authority.classification.clone(),
            data_labels: draft.authority.data_labels.clone(),
            archived_at: draft.archived_at,
            canonical_json: draft.canonical_json,
            created_at: now,
            updated_at: now,
        };
        let schema = SchemaContent {
            tenant: draft.identity.tenant_id.record_id(),
            work_context: work_context.clone(),
            layer_key: draft.layer_key.clone(),
            schema_revision_key: draft.schema.schema_revision_key,
            schema_version: draft.schema.schema_version,
            digest_sha256: draft.schema.digest_sha256,
            schema_json: draft.schema.schema_json,
            created_by: draft.identity.principal_id.record_id(),
            created_at: now,
        };
        let event = authoring_event(
            &draft.identity,
            "map_feature_layer",
            &draft.layer_key,
            "map.feature_layer.created",
            [
                ("layer_key", serde_json::json!(draft.layer_key)),
                ("revision", serde_json::json!(draft.revision)),
            ],
        );
        if let Some(style) = draft.style {
            let style_record = authored_version_record(
                "map_style_revision",
                &draft.identity.tenant_key,
                &content.layer_key,
                style.style_version,
            );
            let style = StyleContent {
                tenant: draft.identity.tenant_id.record_id(),
                work_context,
                layer_key: content.layer_key.clone(),
                style_revision_key: style.style_revision_key,
                style_version: style.style_version,
                style_json: style.style_json,
                created_by: draft.identity.principal_id.record_id(),
                created_at: now,
            };
            self.client()
                .query("BEGIN TRANSACTION; CREATE ONLY $schema_record CONTENT $schema RETURN NONE; CREATE ONLY $style_record CONTENT $style RETURN NONE; CREATE ONLY $layer_record CONTENT $layer RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
                .bind(("schema_record", schema_record))
                .bind(("schema", schema))
                .bind(("style_record", style_record))
                .bind(("style", style))
                .bind(("layer_record", layer_record.clone()))
                .bind(("layer", content))
                .bind(("event", event))
                .await?
                .check()?;
        } else {
            self.client()
                .query("BEGIN TRANSACTION; CREATE ONLY $schema_record CONTENT $schema RETURN NONE; CREATE ONLY $layer_record CONTENT $layer RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
                .bind(("schema_record", schema_record))
                .bind(("schema", schema))
                .bind(("layer_record", layer_record.clone()))
                .bind(("layer", content))
                .bind(("event", event))
                .await?
                .check()?;
        }
        select_only(self, layer_record)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map feature layer creation readback",
            })
    }

    pub async fn map_feature_layer(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
    ) -> Result<Option<MapFeatureLayerRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let record = authored_record("map_feature_layer", tenant_key, &[layer_key]);
        select_scoped(self, record, tenant_id, context_id.record_id()).await
    }

    pub async fn update_map_feature_layer(
        &self,
        draft: MapFeatureLayerUpdateDraft,
        expected_revision: i64,
    ) -> Result<MapFeatureLayerRecord, StoreError> {
        validate_layer_update_draft(&draft, expected_revision)?;
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let layer_record = authored_record(
            "map_feature_layer",
            &draft.identity.tenant_key,
            &[&draft.layer_key],
        );
        let schema = draft.new_schema.as_ref().map(|schema| {
            let record = authored_version_record(
                "map_feature_schema_revision",
                &draft.identity.tenant_key,
                &draft.layer_key,
                schema.schema_version,
            );
            let content = SchemaContent {
                tenant: draft.identity.tenant_id.record_id(),
                work_context: work_context.clone(),
                layer_key: draft.layer_key.clone(),
                schema_revision_key: schema.schema_revision_key.clone(),
                schema_version: schema.schema_version,
                digest_sha256: schema.digest_sha256.clone(),
                schema_json: schema.schema_json.clone(),
                created_by: draft.identity.principal_id.record_id(),
                created_at: Utc::now(),
            };
            (record, content)
        });
        let style = draft.new_style.as_ref().map(|style| {
            let record = authored_version_record(
                "map_style_revision",
                &draft.identity.tenant_key,
                &draft.layer_key,
                style.style_version,
            );
            let content = StyleContent {
                tenant: draft.identity.tenant_id.record_id(),
                work_context: work_context.clone(),
                layer_key: draft.layer_key.clone(),
                style_revision_key: style.style_revision_key.clone(),
                style_version: style.style_version,
                style_json: style.style_json.clone(),
                created_by: draft.identity.principal_id.record_id(),
                created_at: Utc::now(),
            };
            (record, content)
        });
        let event_type = if draft.archived_at.is_some() {
            "map.feature_layer.archived"
        } else {
            "map.feature_layer.updated"
        };
        let event = authoring_event(
            &draft.identity,
            "map_feature_layer",
            &draft.layer_key,
            event_type,
            [
                ("layer_key", serde_json::json!(draft.layer_key)),
                ("revision", serde_json::json!(draft.revision)),
            ],
        );
        let mut query = String::from("BEGIN TRANSACTION; ");
        if schema.is_some() {
            query.push_str("CREATE ONLY $schema_record CONTENT $schema RETURN NONE; ");
        }
        if style.is_some() {
            query.push_str("CREATE ONLY $style_record CONTENT $style RETURN NONE; ");
        }
        query.push_str(
            "LET $updated = (UPDATE ONLY $layer_record SET title = $title, description = $description, schema_version = $schema_version, schema_revision_key = $schema_revision_key, style_version = $style_version, style_revision_key = $style_revision_key, revision = $revision, archived_at = $archived_at, canonical_json = $canonical_json, updated_at = $now WHERE tenant = $tenant AND work_context = $context AND revision = $expected RETURN AFTER); IF $updated = NONE { THROW 'map_feature_layer_conflict'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;",
        );
        let mut query = self.client().query(query);
        if let Some((record, content)) = schema {
            query = query
                .bind(("schema_record", record))
                .bind(("schema", content));
        }
        if let Some((record, content)) = style {
            query = query
                .bind(("style_record", record))
                .bind(("style", content));
        }
        query
            .bind(("layer_record", layer_record.clone()))
            .bind(("title", draft.title))
            .bind(("description", draft.description))
            .bind(("schema_version", draft.schema_version))
            .bind(("schema_revision_key", draft.schema_revision_key))
            .bind(("style_version", draft.style_version))
            .bind(("style_revision_key", draft.style_revision_key))
            .bind(("revision", draft.revision))
            .bind(("archived_at", draft.archived_at))
            .bind(("canonical_json", draft.canonical_json))
            .bind(("now", Utc::now()))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", work_context))
            .bind(("expected", expected_revision))
            .bind(("event", event))
            .await?
            .check()?;
        select_only(self, layer_record)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map feature layer update readback",
            })
    }

    pub async fn list_map_feature_layers(
        &self,
        tenant_key: &str,
        context_key: &str,
        include_archived: bool,
    ) -> Result<Vec<MapFeatureLayerRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let archived = if include_archived {
            ""
        } else {
            "AND archived_at = NONE"
        };
        let query = format!(
            "SELECT * FROM map_feature_layer WHERE tenant = $tenant AND work_context = $context {archived} ORDER BY updated_at DESC;"
        );
        let mut response = self
            .client()
            .query(query)
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_feature_schema_revision(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
        version: i64,
    ) -> Result<Option<MapFeatureSchemaRevisionRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_version_record(
                "map_feature_schema_revision",
                tenant_key,
                layer_key,
                version,
            ),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn map_style_revision(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
        version: i64,
    ) -> Result<Option<MapStyleRevisionRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_version_record("map_style_revision", tenant_key, layer_key, version),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn map_style_revision_by_key(
        &self,
        tenant_key: &str,
        context_key: &str,
        style_revision_key: &str,
    ) -> Result<Option<MapStyleRevisionRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY map_style_revision WHERE tenant = $tenant AND work_context = $context AND style_revision_key = $style_key;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .bind(("style_key", style_revision_key.to_owned()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn commit_map_feature_changes(
        &self,
        draft: MapFeatureCommitDraft,
    ) -> Result<MapFeatureCommitResult, StoreError> {
        validate_commit_draft(&draft)?;
        if let Some(existing) = self
            .map_feature_changeset(
                &draft.identity.tenant_key,
                &draft.authority.context_key,
                &draft.layer_key,
                &draft.changeset_key,
            )
            .await?
        {
            return idempotent_commit_result(self, &draft, existing).await;
        }

        let now = Utc::now();
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let layer_record = authored_record(
            "map_feature_layer",
            &draft.identity.tenant_key,
            &[&draft.layer_key],
        );
        let changeset_record = authored_record(
            "map_feature_changeset",
            &draft.identity.tenant_key,
            &[&draft.layer_key, &draft.changeset_key],
        );
        let resulting_layer_revision = draft.expected_layer_revision + 1;
        let feature_keys = draft
            .revisions
            .iter()
            .map(|revision| revision.feature_key.clone())
            .collect::<Vec<_>>();
        let changeset = ChangeSetContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: work_context.clone(),
            actor_key: draft.identity.principal_key.clone(),
            work_context_key: draft.authority.context_key.clone(),
            authority: draft.authority.clone(),
            layer_key: draft.layer_key.clone(),
            changeset_key: draft.changeset_key.clone(),
            base_layer_revision: draft.expected_layer_revision,
            resulting_layer_revision,
            feature_keys: feature_keys.clone(),
            idempotency_key: draft.idempotency_key.clone(),
            request_digest_sha256: draft.request_digest_sha256.clone(),
            commit_sequence: 0,
            canonical_json: draft.changeset_canonical_json.clone(),
            created_at: now,
        };
        let mutations = draft
            .revisions
            .iter()
            .map(|revision| {
                let revision_record = authored_feature_revision_record(
                    &draft.identity.tenant_key,
                    &draft.layer_key,
                    &revision.feature_key,
                    revision.feature_revision,
                );
                let head_record = authored_record(
                    "map_feature_head",
                    &draft.identity.tenant_key,
                    &[&draft.layer_key, &revision.feature_key],
                );
                let content = RevisionContent {
                    tenant: draft.identity.tenant_id.record_id(),
                    work_context: work_context.clone(),
                    layer_key: draft.layer_key.clone(),
                    feature_key: revision.feature_key.clone(),
                    feature_revision: revision.feature_revision,
                    layer_revision: revision.layer_revision,
                    schema_version: revision.schema_version,
                    changeset_key: draft.changeset_key.clone(),
                    deleted: revision.deleted,
                    geometry_type: revision.geometry_type.clone(),
                    geometry_json: revision.geometry_json.clone(),
                    bbox_west: revision.bbox_west,
                    bbox_south: revision.bbox_south,
                    bbox_east: revision.bbox_east,
                    bbox_north: revision.bbox_north,
                    valid_from: revision.valid_from,
                    valid_until: revision.valid_until,
                    semantic_type: revision.semantic_type.clone(),
                    title: revision.title.clone(),
                    canonical_json: revision.canonical_json.clone(),
                    created_at: now,
                };
                let head = HeadContent {
                    tenant: content.tenant.clone(),
                    work_context: content.work_context.clone(),
                    layer_key: content.layer_key.clone(),
                    feature_key: content.feature_key.clone(),
                    feature_revision: content.feature_revision,
                    layer_revision: content.layer_revision,
                    schema_version: content.schema_version,
                    changeset_key: content.changeset_key.clone(),
                    deleted: content.deleted,
                    geometry_type: content.geometry_type.clone(),
                    geometry_json: content.geometry_json.clone(),
                    bbox_west: content.bbox_west,
                    bbox_south: content.bbox_south,
                    bbox_east: content.bbox_east,
                    bbox_north: content.bbox_north,
                    valid_from: content.valid_from,
                    valid_until: content.valid_until,
                    semantic_type: content.semantic_type.clone(),
                    title: content.title.clone(),
                    canonical_json: content.canonical_json.clone(),
                    updated_at: now,
                };
                MutationContent {
                    create: revision.expected_feature_revision.is_none(),
                    head_record,
                    revision_record,
                    expected_feature_revision: revision.expected_feature_revision,
                    head,
                    revision: content,
                }
            })
            .collect::<Vec<_>>();
        let event = authoring_event(
            &draft.identity,
            "map_feature_changeset",
            &draft.changeset_key,
            "map.feature_changes.committed",
            [
                ("layer_key", serde_json::json!(draft.layer_key)),
                ("changeset_key", serde_json::json!(draft.changeset_key)),
                ("tenant_key", serde_json::json!(draft.identity.tenant_key)),
                (
                    "work_context_key",
                    serde_json::json!(draft.authority.context_key),
                ),
            ],
        );
        let result = self
            .client()
            .query(
                "BEGIN TRANSACTION; \
                 CREATE ONLY $changeset_record CONTENT $changeset RETURN NONE; \
                 LET $layer_updated = (UPDATE ONLY $layer_record SET revision = $resulting_layer_revision, canonical_json = $layer_json, updated_at = $now WHERE tenant = $tenant AND work_context = $context AND revision = $expected_layer_revision AND archived_at = NONE RETURN AFTER); \
                 IF $layer_updated = NONE { THROW 'map_feature_layer_conflict'; }; \
                 FOR $mutation IN $mutations { \
                   LET $current = (SELECT * FROM ONLY $mutation.head_record); \
                   IF $mutation.create { \
                     IF $current != NONE { THROW 'map_feature_create_conflict'; }; \
                     CREATE ONLY $mutation.revision_record CONTENT $mutation.revision RETURN NONE; \
                     CREATE ONLY $mutation.head_record CONTENT $mutation.head RETURN NONE; \
                   } ELSE { \
                     IF $current = NONE OR $current.feature_revision != $mutation.expected_feature_revision { THROW 'map_feature_revision_conflict'; }; \
                     CREATE ONLY $mutation.revision_record CONTENT $mutation.revision RETURN NONE; \
                     UPDATE ONLY $mutation.head_record CONTENT $mutation.head RETURN NONE; \
                   }; \
                 }; \
                 LET $events = (CREATE outbox_event CONTENT $event RETURN AFTER); \
                 LET $event_record = array::first($events); \
                 UPDATE ONLY $changeset_record SET commit_sequence = $event_record.sequence RETURN NONE; \
                 COMMIT TRANSACTION;",
            )
            .bind(("changeset_record", changeset_record.clone()))
            .bind(("changeset", changeset))
            .bind(("layer_record", layer_record))
            .bind(("resulting_layer_revision", resulting_layer_revision))
            .bind(("layer_json", draft.layer_canonical_json.clone()))
            .bind(("now", now))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", work_context))
            .bind(("expected_layer_revision", draft.expected_layer_revision))
            .bind(("mutations", mutations))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .map_feature_changeset(
                    &draft.identity.tenant_key,
                    &draft.authority.context_key,
                    &draft.layer_key,
                    &draft.changeset_key,
                )
                .await?
            {
                return idempotent_commit_result(self, &draft, existing).await;
            }
            return Err(StoreError::Database(error));
        }
        let changeset =
            select_only(self, changeset_record)
                .await?
                .ok_or(StoreError::MissingRecord {
                    operation: "map feature changeset creation readback",
                })?;
        let revisions = self
            .list_map_feature_revisions_for_changeset(
                &draft.identity.tenant_key,
                &draft.authority.context_key,
                &draft.changeset_key,
            )
            .await?;
        Ok(MapFeatureCommitResult {
            changeset,
            revisions,
        })
    }

    pub async fn map_feature_changeset(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
        changeset_key: &str,
    ) -> Result<Option<MapFeatureChangeSetRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_record(
                "map_feature_changeset",
                tenant_key,
                &[layer_key, changeset_key],
            ),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn map_feature_head(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
        feature_key: &str,
    ) -> Result<Option<MapFeatureHeadRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_record("map_feature_head", tenant_key, &[layer_key, feature_key]),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn count_map_feature_heads(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
    ) -> Result<u64, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut response = self
            .client()
            .query("SELECT VALUE count() FROM map_feature_head WHERE tenant = $tenant AND work_context = $context AND layer_key = $layer GROUP ALL;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .bind(("layer", layer_key.to_owned()))
            .await?
            .check()?;
        let count = response
            .take::<Vec<i64>>(0)?
            .into_iter()
            .next()
            .unwrap_or(0);
        u64::try_from(count).map_err(|_| invalid("feature_count", "cannot be negative"))
    }

    pub async fn list_map_feature_revisions_for_changeset(
        &self,
        tenant_key: &str,
        context_key: &str,
        changeset_key: &str,
    ) -> Result<Vec<MapFeatureRevisionRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut response = self
            .client()
            .query("SELECT * FROM map_feature_revision WHERE tenant = $tenant AND work_context = $context AND changeset_key = $changeset ORDER BY feature_key ASC;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .bind(("changeset", changeset_key.to_owned()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_feature_revision(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
        feature_key: &str,
        feature_revision: i64,
    ) -> Result<Option<MapFeatureRevisionRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_feature_revision_record(tenant_key, layer_key, feature_key, feature_revision),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn create_map_layer_publication(
        &self,
        draft: MapLayerPublicationDraft,
    ) -> Result<MapLayerPublicationRecord, StoreError> {
        validate_publication_draft(&draft)?;
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let layer_record = authored_record(
            "map_feature_layer",
            &draft.identity.tenant_key,
            &[&draft.layer_key],
        );
        let publication_record = authored_record(
            "map_layer_publication",
            &draft.identity.tenant_key,
            &[&draft.layer_key, &draft.publication_key],
        );
        let content = PublicationContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: work_context.clone(),
            published_by_key: draft.identity.principal_key.clone(),
            work_context_key: draft.authority.context_key.clone(),
            authority: draft.authority.clone(),
            publication_key: draft.publication_key.clone(),
            layer_key: draft.layer_key.clone(),
            layer_revision: draft.layer_revision,
            schema_version: draft.schema_version,
            style_revision_key: draft.style_revision_key,
            artifact_uris: draft.artifact_uris,
            canonical_json: draft.canonical_json,
            published_at: draft.published_at,
        };
        let event = authoring_event(
            &draft.identity,
            "map_layer_publication",
            &draft.publication_key,
            "map.feature_layer.published",
            [
                ("layer_key", serde_json::json!(draft.layer_key)),
                ("publication_key", serde_json::json!(draft.publication_key)),
            ],
        );
        self.client()
            .query("BEGIN TRANSACTION; LET $layer = (SELECT * FROM ONLY $layer_record WHERE tenant = $tenant AND work_context = $context AND revision = $layer_revision AND archived_at = NONE); IF $layer = NONE { THROW 'map_feature_layer_conflict'; }; CREATE ONLY $publication_record CONTENT $publication RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("layer_record", layer_record))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", work_context))
            .bind(("layer_revision", draft.layer_revision))
            .bind(("publication_record", publication_record.clone()))
            .bind(("publication", content))
            .bind(("event", event))
            .await?
            .check()?;
        select_only(self, publication_record)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "map layer publication creation readback",
            })
    }

    pub async fn map_layer_publication(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &str,
        publication_key: &str,
    ) -> Result<Option<MapLayerPublicationRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_record(
                "map_layer_publication",
                tenant_key,
                &[layer_key, publication_key],
            ),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn list_map_layer_publications(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: Option<&str>,
    ) -> Result<Vec<MapLayerPublicationRecord>, StoreError> {
        let tenant_id = crate::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut query = String::from(
            "SELECT * FROM map_layer_publication WHERE tenant = $tenant AND work_context = $context",
        );
        if layer_key.is_some() {
            query.push_str(" AND layer_key = $layer");
        }
        query.push_str(" ORDER BY published_at DESC;");
        let mut query = self
            .client()
            .query(query)
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()));
        if let Some(layer_key) = layer_key {
            query = query.bind(("layer", layer_key.to_owned()));
        }
        let mut response = query.await?.check()?;
        Ok(response.take(0)?)
    }
}

async fn idempotent_commit_result(
    store: &PlatformStore,
    draft: &MapFeatureCommitDraft,
    changeset: MapFeatureChangeSetRecord,
) -> Result<MapFeatureCommitResult, StoreError> {
    if changeset.request_digest_sha256 != draft.request_digest_sha256 {
        return Err(StoreError::MapRecordConflict {
            entity: "feature changeset idempotency key",
            key: draft.idempotency_key.clone(),
        });
    }
    let revisions = store
        .list_map_feature_revisions_for_changeset(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
            &changeset.changeset_key,
        )
        .await?;
    Ok(MapFeatureCommitResult {
        changeset,
        revisions,
    })
}

async fn select_only<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &PlatformStore,
    record: RecordId,
) -> Result<Option<T>, StoreError> {
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record;")
        .bind(("record", record))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn select_scoped<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &PlatformStore,
    record: RecordId,
    tenant_id: TenantId,
    work_context: RecordId,
) -> Result<Option<T>, StoreError> {
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record WHERE tenant = $tenant AND work_context = $context;")
        .bind(("record", record))
        .bind(("tenant", tenant_id.record_id()))
        .bind(("context", work_context))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn authored_record(table: &str, tenant_key: &str, parts: &[&str]) -> RecordId {
    let mut key = Vec::with_capacity(parts.len() + 1);
    key.push(tenant_key.to_owned());
    key.extend(parts.iter().map(|part| (*part).to_owned()));
    RecordId::new(table, Array::from(key))
}

fn authored_version_record(
    table: &str,
    tenant_key: &str,
    entity_key: &str,
    version: i64,
) -> RecordId {
    authored_record(table, tenant_key, &[entity_key, &format!("{version:020}")])
}

fn authored_feature_revision_record(
    tenant_key: &str,
    layer_key: &str,
    feature_key: &str,
    version: i64,
) -> RecordId {
    authored_record(
        "map_feature_revision",
        tenant_key,
        &[layer_key, feature_key, &format!("{version:020}")],
    )
}

fn authoring_event<const N: usize>(
    identity: &PlatformIdentity,
    aggregate_type: &str,
    aggregate_id: &str,
    event_type: &str,
    payload: [(&str, serde_json::Value); N],
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        aggregate_type,
        aggregate_id,
        event_type,
        1,
        OpenObject::new(
            payload
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        ),
    )
}

fn validate_layer_draft(draft: &MapFeatureLayerDraft) -> Result<(), StoreError> {
    validate_key("layer_key", &draft.layer_key, "feature-layer-")?;
    validate_text("title", &draft.title, 256)?;
    if let Some(description) = &draft.description {
        validate_text("description", description, 4096)?;
    }
    if !matches!(
        draft.content_class.as_str(),
        "reference" | "named_locations" | "facilities" | "boundaries" | "network_candidate"
    ) {
        return Err(invalid("content_class", "unsupported value"));
    }
    if draft.revision < 0 || draft.schema.schema_version < 1 {
        return Err(invalid("revision", "must be non-negative"));
    }
    validate_json("canonical_json", &draft.canonical_json)?;
    validate_schema_draft(&draft.schema)?;
    if let Some(style) = &draft.style {
        validate_style_draft(style)?;
    }
    Ok(())
}

fn validate_layer_update_draft(
    draft: &MapFeatureLayerUpdateDraft,
    expected_revision: i64,
) -> Result<(), StoreError> {
    validate_key("layer_key", &draft.layer_key, "feature-layer-")?;
    validate_text("title", &draft.title, 256)?;
    if let Some(description) = &draft.description {
        validate_text("description", description, 4096)?;
    }
    if draft.revision != expected_revision + 1 || expected_revision < 0 {
        return Err(invalid("revision", "must increment the expected revision"));
    }
    if draft.schema_version < 1 {
        return Err(invalid("schema_version", "must be positive"));
    }
    if let Some(schema) = &draft.new_schema {
        validate_schema_draft(schema)?;
        if schema.schema_version != draft.schema_version
            || schema.schema_revision_key != draft.schema_revision_key
        {
            return Err(invalid(
                "new_schema",
                "does not match the selected schema revision",
            ));
        }
    }
    if let Some(style) = &draft.new_style {
        validate_style_draft(style)?;
        if Some(style.style_version) != draft.style_version
            || Some(&style.style_revision_key) != draft.style_revision_key.as_ref()
        {
            return Err(invalid(
                "new_style",
                "does not match the selected style revision",
            ));
        }
    }
    validate_json("canonical_json", &draft.canonical_json)
}

fn validate_schema_draft(draft: &MapFeatureSchemaDraft) -> Result<(), StoreError> {
    validate_key(
        "schema_revision_key",
        &draft.schema_revision_key,
        "feature-schema-",
    )?;
    validate_sha256(&draft.digest_sha256)?;
    validate_json("schema_json", &draft.schema_json)
}

fn validate_style_draft(draft: &MapStyleRevisionDraft) -> Result<(), StoreError> {
    validate_key("style_revision_key", &draft.style_revision_key, "style-")?;
    if draft.style_version < 1 {
        return Err(invalid("style_version", "must be positive"));
    }
    validate_json("style_json", &draft.style_json)
}

fn validate_commit_draft(draft: &MapFeatureCommitDraft) -> Result<(), StoreError> {
    validate_key("layer_key", &draft.layer_key, "feature-layer-")?;
    validate_key("changeset_key", &draft.changeset_key, "changeset-")?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_sha256(&draft.request_digest_sha256)?;
    validate_json("layer_canonical_json", &draft.layer_canonical_json)?;
    validate_json("changeset_canonical_json", &draft.changeset_canonical_json)?;
    if draft.expected_layer_revision < 0 {
        return Err(invalid("expected_layer_revision", "must be non-negative"));
    }
    if draft.revisions.is_empty() || draft.revisions.len() > MAX_FEATURES_PER_CHANGESET {
        return Err(invalid(
            "revisions",
            "must contain between one and 10000 revisions",
        ));
    }
    let mut keys = std::collections::BTreeSet::new();
    for revision in &draft.revisions {
        validate_key("feature_key", &revision.feature_key, "feature-")?;
        if !keys.insert(&revision.feature_key) {
            return Err(invalid("revisions", "contains duplicate feature keys"));
        }
        if revision.feature_revision < 1
            || revision.layer_revision != draft.expected_layer_revision + 1
            || revision.schema_version < 1
        {
            return Err(invalid("revisions", "contains an invalid revision number"));
        }
        if revision.expected_feature_revision.is_none() && revision.feature_revision != 1 {
            return Err(invalid(
                "feature_revision",
                "a created feature must start at revision one",
            ));
        }
        if revision
            .expected_feature_revision
            .is_some_and(|expected| expected < 1 || revision.feature_revision != expected + 1)
        {
            return Err(invalid(
                "feature_revision",
                "must increment expected_feature_revision",
            ));
        }
        validate_json("geometry_json", &revision.geometry_json)?;
        validate_json("canonical_json", &revision.canonical_json)?;
        if [
            revision.bbox_west,
            revision.bbox_south,
            revision.bbox_east,
            revision.bbox_north,
        ]
        .into_iter()
        .any(|value| !value.is_finite())
        {
            return Err(invalid("bbox", "must be finite"));
        }
    }
    Ok(())
}

fn validate_publication_draft(draft: &MapLayerPublicationDraft) -> Result<(), StoreError> {
    validate_key("publication_key", &draft.publication_key, "publication-")?;
    validate_key("layer_key", &draft.layer_key, "feature-layer-")?;
    if draft.layer_revision < 0 || draft.schema_version < 1 {
        return Err(invalid("layer_revision", "contains an invalid version"));
    }
    validate_json("canonical_json", &draft.canonical_json)
}

fn validate_key(field: &'static str, value: &str, prefix: &str) -> Result<(), StoreError> {
    if !value.starts_with(prefix) || value.len() > 128 || value.contains('/') {
        return Err(invalid(field, "is not a canonical public id"));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(invalid(field, "is empty or exceeds its byte limit"));
    }
    Ok(())
}

fn validate_json(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.len() > MAX_AUTHORING_JSON_BYTES
        || serde_json::from_str::<serde_json::Value>(value).is_err()
    {
        return Err(invalid(field, "is invalid JSON or exceeds its byte limit"));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), StoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            "digest_sha256",
            "must be 64 hexadecimal characters",
        ));
    }
    Ok(())
}

fn invalid(field: &'static str, reason: &'static str) -> StoreError {
    StoreError::InvalidMapField { field, reason }
}

pub fn map_authoring_idempotency_key(
    tenant_key: &str,
    context_key: &str,
    layer_key: &str,
    idempotency_key: &str,
) -> String {
    let digest = Sha256::digest(
        [tenant_key, context_key, layer_key, idempotency_key]
            .join("\0")
            .as_bytes(),
    );
    hex_digest(&digest)
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_record_ids_preserve_tenant_layer_locality() {
        let record = authored_record(
            "map_feature_revision",
            "tenant-a",
            &["feature-layer-a", "feature-a", "0001"],
        );
        assert!(format!("{record:?}").contains("feature-layer-a"));
    }

    #[test]
    fn idempotency_scope_changes_the_digest() {
        let first = map_authoring_idempotency_key("tenant", "mission-a", "layer", "request");
        let second = map_authoring_idempotency_key("tenant", "mission-b", "layer", "request");
        assert_ne!(first, second);
    }
}
