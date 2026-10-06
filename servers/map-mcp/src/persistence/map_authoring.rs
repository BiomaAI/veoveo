mod reads;
pub use reads::{MapAuthoringCompletion, MapAuthoringReadScope};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{Array, RecordId, SurrealValue};

use crate::persistence::{
    MapFeatureChangeSetRecord, MapFeatureHeadRecord, MapFeatureLayerRecord,
    MapFeatureRevisionRecord, MapFeatureSchemaRevisionRecord, MapLayerPublicationRecord,
    MapRepository, MapStoreError, MapStyleRevisionRecord,
};
use veoveo_platform_store::primary_transaction_error;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, InvocationAuthorityRecord, PlatformIdentity, TenantId,
    deterministic_work_context_id,
};

const MAX_AUTHORING_JSON_BYTES: usize = 2 * 1024 * 1024;
const MAX_FEATURES_PER_CHANGESET: usize = 10_000;

#[derive(Clone, Debug)]
pub struct MapFeatureSchemaDraft {
    pub schema_revision_key: crate::contract::FeatureSchemaRevisionId,
    pub schema_version: i64,
    pub digest_sha256: String,
    pub schema_json: String,
}

#[derive(Clone, Debug)]
pub struct MapStyleRevisionDraft {
    pub style_revision_key: crate::contract::StyleRevisionId,
    pub style_version: i64,
    pub style_json: String,
}

#[derive(Clone, Debug)]
pub struct MapFeatureLayerDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub layer_key: crate::contract::FeatureLayerId,
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
    pub layer_key: crate::contract::FeatureLayerId,
    pub title: String,
    pub description: Option<String>,
    pub schema_version: i64,
    pub schema_revision_key: crate::contract::FeatureSchemaRevisionId,
    pub new_schema: Option<MapFeatureSchemaDraft>,
    pub style_version: Option<i64>,
    pub style_revision_key: Option<crate::contract::StyleRevisionId>,
    pub new_style: Option<MapStyleRevisionDraft>,
    pub revision: i64,
    pub archived_at: Option<DateTime<Utc>>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapFeatureRevisionDraft {
    pub feature_key: crate::contract::MapFeatureId,
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
    pub layer_key: crate::contract::FeatureLayerId,
    pub layer_canonical_json: String,
    pub expected_layer_revision: i64,
    pub changeset_key: crate::contract::FeatureChangeSetId,
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
    pub publication_key: crate::contract::LayerPublicationId,
    pub layer_key: crate::contract::FeatureLayerId,
    pub layer_revision: i64,
    pub schema_version: i64,
    pub style_revision_key: Option<crate::contract::StyleRevisionId>,
    pub artifact_uris: Vec<veoveo_artifact_contract::ArtifactUri>,
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

impl MapRepository {
    pub async fn create_map_feature_layer(
        &self,
        draft: MapFeatureLayerDraft,
    ) -> Result<MapFeatureLayerRecord, MapStoreError> {
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
            &[draft.layer_key.as_str()],
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
            layer_key: draft.layer_key.to_string(),
            title: draft.title,
            description: draft.description,
            content_class: draft.content_class,
            schema_version: draft.schema.schema_version,
            schema_revision_key: draft.schema.schema_revision_key.to_string(),
            style_version: draft.style.as_ref().map(|style| style.style_version),
            style_revision_key: draft
                .style
                .as_ref()
                .map(|style| style.style_revision_key.to_string()),
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
            layer_key: draft.layer_key.to_string(),
            schema_revision_key: draft.schema.schema_revision_key.to_string(),
            schema_version: draft.schema.schema_version,
            digest_sha256: draft.schema.digest_sha256,
            schema_json: draft.schema.schema_json,
            created_by: draft.identity.principal_id.record_id(),
            created_at: now,
        };
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
                style_revision_key: style.style_revision_key.to_string(),
                style_version: style.style_version,
                style_json: style.style_json,
                created_by: draft.identity.principal_id.record_id(),
                created_at: now,
            };
            self.client()
                .query(include_str!(
                    "queries/map_authoring/create_map_feature_layer.surql"
                ))
                .bind(("schema_record", schema_record))
                .bind(("schema", schema))
                .bind(("style_record", style_record))
                .bind(("style", style))
                .bind(("layer_record", layer_record.clone()))
                .bind(("layer", content))
                .await?
                .check()?;
        } else {
            self.client()
                .query(include_str!(
                    "queries/map_authoring/create_map_feature_layer_2.surql"
                ))
                .bind(("schema_record", schema_record))
                .bind(("schema", schema))
                .bind(("layer_record", layer_record.clone()))
                .bind(("layer", content))
                .await?
                .check()?;
        }
        select_only(self, layer_record)
            .await?
            .ok_or(MapStoreError::MissingRecord {
                operation: "map feature layer creation readback",
            })
    }

    pub async fn update_map_feature_layer(
        &self,
        draft: MapFeatureLayerUpdateDraft,
        expected_revision: i64,
    ) -> Result<MapFeatureLayerRecord, MapStoreError> {
        validate_layer_update_draft(&draft, expected_revision)?;
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let layer_record = authored_record(
            "map_feature_layer",
            &draft.identity.tenant_key,
            &[draft.layer_key.as_str()],
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
                layer_key: draft.layer_key.to_string(),
                schema_revision_key: schema.schema_revision_key.to_string(),
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
                layer_key: draft.layer_key.to_string(),
                style_revision_key: style.style_revision_key.to_string(),
                style_version: style.style_version,
                style_json: style.style_json.clone(),
                created_by: draft.identity.principal_id.record_id(),
                created_at: Utc::now(),
            };
            (record, content)
        });
        let mut query = String::from(include_str!(
            "queries/map_authoring/update_map_feature_layer.surql"
        ));
        if schema.is_some() {
            query.push_str(include_str!(
                "queries/map_authoring/update_map_feature_layer_2.surql"
            ));
        }
        if style.is_some() {
            query.push_str(include_str!(
                "queries/map_authoring/update_map_feature_layer_3.surql"
            ));
        }
        query.push_str(include_str!(
            "queries/map_authoring/update_map_feature_layer_4.surql"
        ));
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
            .bind(("schema_revision_key", draft.schema_revision_key.to_string()))
            .bind(("style_version", draft.style_version))
            .bind((
                "style_revision_key",
                draft.style_revision_key.as_ref().map(ToString::to_string),
            ))
            .bind(("revision", draft.revision))
            .bind(("archived_at", draft.archived_at))
            .bind(("canonical_json", draft.canonical_json))
            .bind(("now", Utc::now()))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", work_context))
            .bind(("expected", expected_revision))
            .await?
            .check()?;
        select_only(self, layer_record)
            .await?
            .ok_or(MapStoreError::MissingRecord {
                operation: "map feature layer update readback",
            })
    }

    pub async fn map_feature_schema_revision(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &crate::contract::FeatureLayerId,
        version: i64,
    ) -> Result<Option<MapFeatureSchemaRevisionRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
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
        layer_key: &crate::contract::FeatureLayerId,
        version: i64,
    ) -> Result<Option<MapStyleRevisionRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
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
        style_revision_key: &crate::contract::StyleRevisionId,
    ) -> Result<Option<MapStyleRevisionRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut response = self
            .client()
            .query(include_str!(
                "queries/map_authoring/map_style_revision_by_key.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .bind(("style_key", style_revision_key.to_string()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn commit_map_feature_changes(
        &self,
        draft: MapFeatureCommitDraft,
    ) -> Result<MapFeatureCommitResult, MapStoreError> {
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
            &[draft.layer_key.as_str()],
        );
        let changeset_record = authored_record(
            "map_feature_changeset",
            &draft.identity.tenant_key,
            &[draft.layer_key.as_str(), draft.changeset_key.as_str()],
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
            layer_key: draft.layer_key.to_string(),
            changeset_key: draft.changeset_key.to_string(),
            base_layer_revision: draft.expected_layer_revision,
            resulting_layer_revision,
            feature_keys: feature_keys.iter().map(ToString::to_string).collect(),
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
                    &[draft.layer_key.as_str(), revision.feature_key.as_str()],
                );
                let content = RevisionContent {
                    tenant: draft.identity.tenant_id.record_id(),
                    work_context: work_context.clone(),
                    layer_key: draft.layer_key.to_string(),
                    feature_key: revision.feature_key.to_string(),
                    feature_revision: revision.feature_revision,
                    layer_revision: revision.layer_revision,
                    schema_version: revision.schema_version,
                    changeset_key: draft.changeset_key.to_string(),
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
        let result = self
            .client()
            .query(include_str!(
                "queries/map_authoring/commit_map_feature_changes.surql"
            ))
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
            .await
            .and_then(
                |mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                },
            );
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
            return Err(MapStoreError::Database(error));
        }
        let changeset =
            select_only(self, changeset_record)
                .await?
                .ok_or(MapStoreError::MissingRecord {
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
        layer_key: &crate::contract::FeatureLayerId,
        changeset_key: &crate::contract::FeatureChangeSetId,
    ) -> Result<Option<MapFeatureChangeSetRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_record(
                "map_feature_changeset",
                tenant_key,
                &[layer_key.as_str(), changeset_key.as_str()],
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
        layer_key: &crate::contract::FeatureLayerId,
        feature_key: &crate::contract::MapFeatureId,
    ) -> Result<Option<MapFeatureHeadRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_record(
                "map_feature_head",
                tenant_key,
                &[layer_key.as_str(), feature_key.as_str()],
            ),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }

    pub async fn count_map_feature_heads(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &crate::contract::FeatureLayerId,
    ) -> Result<u64, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut response = self
            .client()
            .query(include_str!(
                "queries/map_authoring/count_map_feature_heads.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .bind(("layer", layer_key.to_string()))
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
        changeset_key: &crate::contract::FeatureChangeSetId,
    ) -> Result<Vec<MapFeatureRevisionRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        let mut response = self
            .client()
            .query(include_str!(
                "queries/map_authoring/list_map_feature_revisions_for_changeset.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("context", context_id.record_id()))
            .bind(("changeset", changeset_key.to_string()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_feature_revision(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &crate::contract::FeatureLayerId,
        feature_key: &crate::contract::MapFeatureId,
        feature_revision: i64,
    ) -> Result<Option<MapFeatureRevisionRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
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
    ) -> Result<MapLayerPublicationRecord, MapStoreError> {
        validate_publication_draft(&draft)?;
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let layer_record = authored_record(
            "map_feature_layer",
            &draft.identity.tenant_key,
            &[draft.layer_key.as_str()],
        );
        let publication_record = authored_record(
            "map_layer_publication",
            &draft.identity.tenant_key,
            &[draft.layer_key.as_str(), draft.publication_key.as_str()],
        );
        let content = PublicationContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: work_context.clone(),
            published_by_key: draft.identity.principal_key.clone(),
            work_context_key: draft.authority.context_key.clone(),
            authority: draft.authority.clone(),
            publication_key: draft.publication_key.to_string(),
            layer_key: draft.layer_key.to_string(),
            layer_revision: draft.layer_revision,
            schema_version: draft.schema_version,
            style_revision_key: draft.style_revision_key.as_ref().map(ToString::to_string),
            artifact_uris: draft.artifact_uris.into_iter().map(String::from).collect(),
            canonical_json: draft.canonical_json,
            published_at: draft.published_at,
        };
        self.client()
            .query(include_str!(
                "queries/map_authoring/create_map_layer_publication.surql"
            ))
            .bind(("layer_record", layer_record))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", work_context))
            .bind(("layer_revision", draft.layer_revision))
            .bind(("publication_record", publication_record.clone()))
            .bind(("publication", content))
            .await?
            .check()?;
        select_only(self, publication_record)
            .await?
            .ok_or(MapStoreError::MissingRecord {
                operation: "map layer publication creation readback",
            })
    }

    pub async fn map_layer_publication(
        &self,
        tenant_key: &str,
        context_key: &str,
        layer_key: &crate::contract::FeatureLayerId,
        publication_key: &crate::contract::LayerPublicationId,
    ) -> Result<Option<MapLayerPublicationRecord>, MapStoreError> {
        let tenant_id = veoveo_platform_store::deterministic_tenant_id(tenant_key)?;
        let context_id = deterministic_work_context_id(tenant_key, context_key)?;
        select_scoped(
            self,
            authored_record(
                "map_layer_publication",
                tenant_key,
                &[layer_key.as_str(), publication_key.as_str()],
            ),
            tenant_id,
            context_id.record_id(),
        )
        .await
    }
}

async fn idempotent_commit_result(
    store: &MapRepository,
    draft: &MapFeatureCommitDraft,
    changeset: MapFeatureChangeSetRecord,
) -> Result<MapFeatureCommitResult, MapStoreError> {
    if changeset.request_digest_sha256 != draft.request_digest_sha256 {
        return Err(MapStoreError::MapRecordConflict {
            entity: "feature changeset idempotency key",
            key: draft.idempotency_key.clone(),
        });
    }
    let revisions = store
        .list_map_feature_revisions_for_changeset(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
            &crate::contract::FeatureChangeSetId::parse(&changeset.changeset_key).map_err(
                |_| {
                    invalid(
                        "changeset_key",
                        "stored identity must satisfy its owning profile",
                    )
                },
            )?,
        )
        .await?;
    Ok(MapFeatureCommitResult {
        changeset,
        revisions,
    })
}

async fn select_only<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &MapRepository,
    record: RecordId,
) -> Result<Option<T>, MapStoreError> {
    let mut response = store
        .client()
        .query(include_str!("queries/map_authoring/select_only.surql"))
        .bind(("record", record))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn select_scoped<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &MapRepository,
    record: RecordId,
    tenant_id: TenantId,
    work_context: RecordId,
) -> Result<Option<T>, MapStoreError> {
    let mut response = store
        .client()
        .query(include_str!("queries/map_authoring/select_scoped.surql"))
        .bind(("record", record))
        .bind(("tenant", tenant_id.record_id()))
        .bind(("context", work_context))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn authored_record(table: &str, tenant_key: &str, parts: &[&str]) -> RecordId {
    let mut key = Vec::with_capacity(parts.len() + 1);
    key.push(tenant_key.to_string());
    key.extend(parts.iter().map(|part| (*part).to_owned()));
    RecordId::new(table, Array::from(key))
}

fn authored_version_record(
    table: &str,
    tenant_key: &str,
    entity_key: impl AsRef<str>,
    version: i64,
) -> RecordId {
    let entity_key = entity_key.as_ref();
    authored_record(table, tenant_key, &[entity_key, &format!("{version:020}")])
}

fn authored_feature_revision_record(
    tenant_key: &str,
    layer_key: impl AsRef<str>,
    feature_key: impl AsRef<str>,
    version: i64,
) -> RecordId {
    let feature_key = feature_key.as_ref();
    let layer_key = layer_key.as_ref();
    authored_record(
        "map_feature_revision",
        tenant_key,
        &[layer_key, feature_key, &format!("{version:020}")],
    )
}

fn validate_layer_draft(draft: &MapFeatureLayerDraft) -> Result<(), MapStoreError> {
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
) -> Result<(), MapStoreError> {
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

fn validate_schema_draft(draft: &MapFeatureSchemaDraft) -> Result<(), MapStoreError> {
    validate_sha256(&draft.digest_sha256)?;
    validate_json("schema_json", &draft.schema_json)
}

fn validate_style_draft(draft: &MapStyleRevisionDraft) -> Result<(), MapStoreError> {
    if draft.style_version < 1 {
        return Err(invalid("style_version", "must be positive"));
    }
    validate_json("style_json", &draft.style_json)
}

fn validate_commit_draft(draft: &MapFeatureCommitDraft) -> Result<(), MapStoreError> {
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

fn validate_publication_draft(draft: &MapLayerPublicationDraft) -> Result<(), MapStoreError> {
    if draft.layer_revision < 0 || draft.schema_version < 1 {
        return Err(invalid("layer_revision", "contains an invalid version"));
    }
    validate_json("canonical_json", &draft.canonical_json)
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), MapStoreError> {
    if value.trim().is_empty() || value.len() > maximum {
        return Err(invalid(field, "is empty or exceeds its byte limit"));
    }
    Ok(())
}

fn validate_json(field: &'static str, value: &str) -> Result<(), MapStoreError> {
    if value.len() > MAX_AUTHORING_JSON_BYTES
        || serde_json::from_str::<serde_json::Value>(value).is_err()
    {
        return Err(invalid(field, "is invalid JSON or exceeds its byte limit"));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), MapStoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            "digest_sha256",
            "must be 64 hexadecimal characters",
        ));
    }
    Ok(())
}

fn invalid(field: &'static str, reason: &'static str) -> MapStoreError {
    MapStoreError::InvalidMapField { field, reason }
}

pub fn map_authoring_idempotency_key(
    tenant_key: &str,
    context_key: &str,
    layer_key: &crate::contract::FeatureLayerId,
    idempotency_key: &str,
) -> String {
    let digest = Sha256::digest(
        [tenant_key, context_key, layer_key.as_str(), idempotency_key]
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
        let layer = crate::contract::FeatureLayerId::new();
        let first = map_authoring_idempotency_key("tenant", "mission-a", &layer, "request");
        let second = map_authoring_idempotency_key("tenant", "mission-b", &layer, "request");
        assert_ne!(first, second);
    }
}
