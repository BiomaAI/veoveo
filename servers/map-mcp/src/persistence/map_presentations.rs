use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Array, RecordId, SurrealValue};

use crate::persistence::{
    MapCompositionRecord, MapCompositionRevisionRecord, MapLayerProductRecord, MapRepository,
    MapStoreError,
};
use veoveo_platform_store::primary_transaction_error;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, InvocationAuthorityRecord, PlatformIdentity, deterministic_tenant_id,
    deterministic_work_context_id,
};

const MAX_CANONICAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct MapLayerProductDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub product_key: String,
    pub publication_key: String,
    pub layer_key: String,
    pub layer_revision: i64,
    pub format: String,
    pub artifact_uri: veoveo_artifact_contract::ArtifactUri,
    pub mime_type: String,
    pub digest_sha256: String,
    pub size_bytes: i64,
    pub feature_count: i64,
    pub canonical_json: String,
    pub created_by_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct MapCompositionRevisionDraft {
    pub composition_revision_key: String,
    pub revision: i64,
    pub publication_keys: Vec<String>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapCompositionDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub composition_key: String,
    pub title: String,
    pub revision: MapCompositionRevisionDraft,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct MapCompositionUpdateDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub composition_key: String,
    pub title: String,
    pub revision: MapCompositionRevisionDraft,
    pub canonical_json: String,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ProductContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    authority: InvocationAuthorityRecord,
    product_key: String,
    publication_key: String,
    layer_key: String,
    layer_revision: i64,
    format: String,
    artifact_uri: String,
    mime_type: String,
    digest_sha256: String,
    size_bytes: i64,
    feature_count: i64,
    canonical_json: String,
    created_by_key: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct CompositionContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    authority: InvocationAuthorityRecord,
    created_by_key: String,
    owner_kind: ArtifactGrantSubjectKind,
    owner_key: String,
    composition_key: String,
    title: String,
    current_revision: i64,
    canonical_json: String,
    archived_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct CompositionRevisionContent {
    tenant: RecordId,
    work_context: RecordId,
    authority: InvocationAuthorityRecord,
    composition_key: String,
    composition_revision_key: String,
    revision: i64,
    publication_keys: Vec<String>,
    canonical_json: String,
    created_by: RecordId,
    created_at: DateTime<Utc>,
}

impl MapRepository {
    pub async fn create_map_layer_product(
        &self,
        draft: MapLayerProductDraft,
    ) -> Result<MapLayerProductRecord, MapStoreError> {
        validate_product(&draft)?;
        let context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let product_record = record(
            "map_layer_product",
            &draft.identity.tenant_key,
            &[&draft.product_key],
        );
        if let Some(existing) = select_scoped::<MapLayerProductRecord>(
            self,
            product_record.clone(),
            draft.identity.tenant_id,
            context.clone(),
        )
        .await?
        {
            return matching_product(existing, &draft);
        }
        let publication_record = record(
            "map_layer_publication",
            &draft.identity.tenant_key,
            &[&draft.layer_key, &draft.publication_key],
        );
        let content = ProductContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: context.clone(),
            authority: draft.authority.clone(),
            product_key: draft.product_key.clone(),
            publication_key: draft.publication_key.clone(),
            layer_key: draft.layer_key.clone(),
            layer_revision: draft.layer_revision,
            format: draft.format.clone(),
            artifact_uri: draft.artifact_uri.to_string(),
            mime_type: draft.mime_type.clone(),
            digest_sha256: draft.digest_sha256.clone(),
            size_bytes: draft.size_bytes,
            feature_count: draft.feature_count,
            canonical_json: draft.canonical_json.clone(),
            created_by_key: draft.created_by_key.clone(),
            created_at: draft.created_at,
        };

        let result = self
            .client()
            .query(include_str!(
                "queries/map_presentations/create_map_layer_product.surql"
            ))
            .bind(("publication_record", publication_record))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", context.clone()))
            .bind(("layer_revision", draft.layer_revision))
            .bind(("product_record", product_record.clone()))
            .bind(("product", content))
            .await
            .and_then(
                |mut response| match primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(()),
                },
            );
        if let Err(error) = result {
            if let Some(existing) = select_scoped::<MapLayerProductRecord>(
                self,
                product_record,
                draft.identity.tenant_id,
                context,
            )
            .await?
            {
                return matching_product(existing, &draft);
            }
            return Err(MapStoreError::Database(error));
        }
        select_scoped(self, product_record, draft.identity.tenant_id, context)
            .await?
            .ok_or(MapStoreError::MissingRecord {
                operation: "map layer product creation readback",
            })
    }

    pub async fn create_map_composition(
        &self,
        draft: MapCompositionDraft,
    ) -> Result<MapCompositionRecord, MapStoreError> {
        validate_composition_revision(&draft.revision)?;
        validate_text("title", &draft.title, 256)?;
        let now = Utc::now();
        let context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let root_record = record(
            "map_composition",
            &draft.identity.tenant_key,
            &[&draft.composition_key],
        );
        let revision_record = version_record(
            "map_composition_revision",
            &draft.identity.tenant_key,
            &draft.composition_key,
            draft.revision.revision,
        );
        let root = CompositionContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: context.clone(),
            authority: draft.authority.clone(),
            created_by_key: draft.identity.principal_key.clone(),
            owner_kind: draft.authority.owner_kind,
            owner_key: draft.authority.owner_key.clone(),
            composition_key: draft.composition_key.clone(),
            title: draft.title,
            current_revision: draft.revision.revision,
            canonical_json: draft.canonical_json,
            archived_at: None,
            created_at: now,
            updated_at: now,
        };
        let revision = revision_content(
            &draft.identity,
            &draft.authority,
            &draft.composition_key,
            draft.revision,
        )?;

        self.client()
            .query(include_str!(
                "queries/map_presentations/create_map_composition.surql"
            ))
            .bind(("revision_record", revision_record))
            .bind(("revision", revision))
            .bind(("root_record", root_record.clone()))
            .bind(("root", root))
            .await?
            .check()?;
        select_only(self, root_record)
            .await?
            .ok_or(MapStoreError::MissingRecord {
                operation: "map composition creation readback",
            })
    }

    pub async fn update_map_composition(
        &self,
        draft: MapCompositionUpdateDraft,
        expected_revision: i64,
    ) -> Result<MapCompositionRecord, MapStoreError> {
        validate_composition_revision(&draft.revision)?;
        if draft.revision.revision != expected_revision + 1 {
            return Err(invalid("revision", "must increment expected revision"));
        }
        validate_text("title", &draft.title, 256)?;
        let context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?
        .record_id();
        let root_record = record(
            "map_composition",
            &draft.identity.tenant_key,
            &[&draft.composition_key],
        );
        let revision_record = version_record(
            "map_composition_revision",
            &draft.identity.tenant_key,
            &draft.composition_key,
            draft.revision.revision,
        );
        let revision = revision_content(
            &draft.identity,
            &draft.authority,
            &draft.composition_key,
            draft.revision,
        )?;

        self.client()
            .query(include_str!(
                "queries/map_presentations/update_map_composition.surql"
            ))
            .bind(("revision_record", revision_record))
            .bind(("revision", revision))
            .bind(("root_record", root_record.clone()))
            .bind(("title", draft.title))
            .bind(("current_revision", expected_revision + 1))
            .bind(("canonical_json", draft.canonical_json))
            .bind(("archived_at", draft.archived_at))
            .bind(("now", Utc::now()))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("context", context))
            .bind(("expected", expected_revision))
            .await?
            .check()?;
        select_only(self, root_record)
            .await?
            .ok_or(MapStoreError::MissingRecord {
                operation: "map composition update readback",
            })
    }

    pub async fn map_composition_revision(
        &self,
        tenant_key: &str,
        context_key: &str,
        composition_key: &str,
        revision: i64,
    ) -> Result<Option<MapCompositionRevisionRecord>, MapStoreError> {
        select_scoped(
            self,
            version_record(
                "map_composition_revision",
                tenant_key,
                composition_key,
                revision,
            ),
            deterministic_tenant_id(tenant_key)?,
            deterministic_work_context_id(tenant_key, context_key)?.record_id(),
        )
        .await
    }
}

fn revision_content(
    identity: &PlatformIdentity,
    authority: &InvocationAuthorityRecord,
    composition_key: &str,
    draft: MapCompositionRevisionDraft,
) -> Result<CompositionRevisionContent, MapStoreError> {
    Ok(CompositionRevisionContent {
        tenant: identity.tenant_id.record_id(),
        work_context: deterministic_work_context_id(&identity.tenant_key, &authority.context_key)?
            .record_id(),
        authority: authority.clone(),
        composition_key: composition_key.to_owned(),
        composition_revision_key: draft.composition_revision_key,
        revision: draft.revision,
        publication_keys: draft.publication_keys,
        canonical_json: draft.canonical_json,
        created_by: identity.principal_id.record_id(),
        created_at: Utc::now(),
    })
}

fn matching_product(
    existing: MapLayerProductRecord,
    draft: &MapLayerProductDraft,
) -> Result<MapLayerProductRecord, MapStoreError> {
    if existing.canonical_json == draft.canonical_json {
        Ok(existing)
    } else {
        Err(MapStoreError::MapRecordConflict {
            entity: "map layer product",
            key: draft.product_key.clone(),
        })
    }
}

fn validate_product(draft: &MapLayerProductDraft) -> Result<(), MapStoreError> {
    if !supported_product_format(&draft.format) {
        return Err(invalid("format", "unsupported layer product format"));
    }
    if draft.layer_revision < 0 || draft.size_bytes < 0 || draft.feature_count < 0 {
        return Err(invalid("counts", "product counts cannot be negative"));
    }
    if draft.digest_sha256.len() != 64
        || !draft
            .digest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid(
            "digest_sha256",
            "must be a hexadecimal SHA-256 digest",
        ));
    }
    validate_json("canonical_json", &draft.canonical_json)
}

fn supported_product_format(value: &str) -> bool {
    matches!(
        value,
        "geojson_seq" | "geoparquet" | "geo_package" | "mvt_bundle"
    )
}

fn validate_composition_revision(draft: &MapCompositionRevisionDraft) -> Result<(), MapStoreError> {
    if draft.revision < 1 {
        return Err(invalid("revision", "must be positive"));
    }
    if draft.publication_keys.len() > 64 {
        return Err(invalid("publication_keys", "cannot exceed 64 entries"));
    }
    validate_json("canonical_json", &draft.canonical_json)
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), MapStoreError> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(invalid(
            field,
            "must be non-empty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

fn validate_json(field: &'static str, value: &str) -> Result<(), MapStoreError> {
    if value.len() > MAX_CANONICAL_BYTES
        || serde_json::from_str::<serde_json::Value>(value).is_err()
    {
        return Err(invalid(field, "must be bounded valid JSON"));
    }
    Ok(())
}

async fn select_only<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &MapRepository,
    record: RecordId,
) -> Result<Option<T>, MapStoreError> {
    let mut response = store
        .client()
        .query(include_str!("queries/map_presentations/select_only.surql"))
        .bind(("record", record))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn select_scoped<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &MapRepository,
    record: RecordId,
    tenant: veoveo_platform_store::TenantId,
    context: RecordId,
) -> Result<Option<T>, MapStoreError> {
    let mut response = store
        .client()
        .query(include_str!(
            "queries/map_presentations/select_scoped.surql"
        ))
        .bind(("record", record))
        .bind(("tenant", tenant.record_id()))
        .bind(("context", context))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn record(table: &str, tenant_key: &str, parts: &[&str]) -> RecordId {
    let mut key = Vec::with_capacity(parts.len() + 1);
    key.push(tenant_key.to_owned());
    key.extend(parts.iter().map(|part| (*part).to_owned()));
    RecordId::new(table, Array::from(key))
}

fn version_record(table: &str, tenant_key: &str, key: &str, version: i64) -> RecordId {
    record(table, tenant_key, &[key, &format!("{version:020}")])
}

fn invalid(field: &'static str, reason: &'static str) -> MapStoreError {
    MapStoreError::InvalidMapField { field, reason }
}

#[cfg(test)]
mod tests {
    use super::supported_product_format;

    #[test]
    fn map_layer_product_formats_match_the_public_contract() {
        for format in ["geojson_seq", "geoparquet", "geo_package", "mvt_bundle"] {
            assert!(supported_product_format(format), "rejected {format}");
        }
        assert!(!supported_product_format("shapefile"));
    }
}
