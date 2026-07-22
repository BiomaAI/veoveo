use std::collections::BTreeSet;
use std::fmt::Write as _;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    AccessLevel, AccessSubject, GatewayInternalIdentity, InvocationMode, InvocationProvenance,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, GrantPermission, InvocationAuthorityRecord,
    InvocationMode as StoreInvocationMode, MapFeatureCommitDraft, MapFeatureLayerDraft,
    MapFeatureLayerUpdateDraft, MapFeatureRevisionDraft, MapFeatureSchemaDraft,
    MapLayerPublicationDraft, MapStyleRevisionDraft, PlatformStore, WorkContextInitialGrantRecord,
    map_authoring_idempotency_key,
};

use crate::analytics::MapAnalytics;
use crate::catalog::MapScope;
use crate::contract::{
    ArchiveFeatureLayerRequest, CommitFeatureChangesOutput, CommitFeatureChangesRequest,
    CreateFeatureLayerRequest, FeatureChangeSet, FeatureChangeSetId, FeatureInput, FeatureLayer,
    FeatureMutation, FeatureProvenance, FeatureSchemaRevision, FeatureValidationFinding,
    GeoJsonFeatureType, JSON_FG_CORE_CONFORMANCE, JSON_FG_TYPES_SCHEMAS_CONFORMANCE,
    LayerPublication, LayerPublicationId, MAX_DIRECT_FEATURE_BYTES, MAX_DIRECT_FEATURE_MUTATIONS,
    MapFeature, MapFeatureId, MapStyleRevision, ProjectionState, PublishFeatureLayerRequest,
    QueryFeaturesOutput, QueryFeaturesRequest, RestoreFeatureRequest, StyleRevisionId,
    UpdateFeatureLayerRequest, ValidateFeatureChangesOutput, ValidateFeatureChangesRequest,
};

use super::{
    AuthoringProjection,
    validation::{
        ValidatedSchema, canonical_json, validate_feature, validate_input, validate_schema,
        validate_style,
    },
};

#[derive(Clone, Debug)]
pub struct AuthoringService {
    store: PlatformStore,
    projection: AuthoringProjection,
    pub(super) analytics: MapAnalytics,
}

#[derive(Debug)]
struct PreparedChanges {
    layer: FeatureLayer,
    features: Vec<MapFeature>,
    findings: Vec<FeatureValidationFinding>,
}

#[derive(Clone, Copy)]
enum PreparationMode<'a> {
    Interactive(&'a str),
    BulkImport(&'a str),
}

impl<'a> PreparationMode<'a> {
    fn stable_seed(self) -> &'a str {
        match self {
            Self::Interactive(seed) | Self::BulkImport(seed) => seed,
        }
    }

    fn checks_create_conflicts(self) -> bool {
        matches!(self, Self::Interactive(_))
    }
}

impl AuthoringService {
    pub fn new(store: PlatformStore, analytics: MapAnalytics) -> Self {
        Self {
            projection: AuthoringProjection::new(store.clone(), analytics.clone()),
            analytics,
            store,
        }
    }

    pub fn store(&self) -> &PlatformStore {
        &self.store
    }

    pub async fn reconcile_projection(&self) -> Result<u64> {
        self.projection.reconcile().await
    }

    pub async fn create_layer(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: CreateFeatureLayerRequest,
    ) -> Result<FeatureLayer> {
        require_access(identity, AccessLevel::Write)?;
        validate_layer_title(&request.title)?;
        if let Some(description) = &request.description {
            validate_description(description)?;
        }
        let schema = validate_schema(&request.property_schema)?;
        if let Some(style) = &request.style {
            validate_style(style)?;
        }
        let now = Utc::now();
        let layer_id = crate::contract::FeatureLayerId::new();
        let schema_revision = FeatureSchemaRevision {
            schema_revision_id: crate::contract::FeatureSchemaRevisionId::new(),
            layer_id: layer_id.clone(),
            version: 1,
            digest_sha256: schema.digest_sha256.clone(),
            schema: schema.value.clone(),
            created_at: now,
        };
        let style_revision = request.style.map(|style| MapStyleRevision {
            style_revision_id: StyleRevisionId::new(),
            layer_id: layer_id.clone(),
            version: 1,
            style,
            created_at: now,
        });
        let layer = FeatureLayer {
            layer_id: layer_id.clone(),
            title: request.title,
            description: request.description,
            content_class: request.content_class,
            schema: schema_revision,
            style: style_revision,
            revision: 0,
            owner: identity.authority.output_policy.owner.clone(),
            created_by: identity.actor.id.clone(),
            work_context: identity.authority.work_context.clone(),
            classification: identity.authority.output_policy.classification.clone(),
            data_labels: identity.authority.output_policy.data_labels.clone(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        };
        self.store
            .create_map_feature_layer(MapFeatureLayerDraft {
                identity: scope.identity.clone(),
                authority: authority_record(identity),
                layer_key: layer_id.to_string(),
                title: layer.title.clone(),
                description: layer.description.clone(),
                content_class: wire(&layer.content_class)?,
                schema: schema_draft(&layer.schema, &schema),
                style: layer.style.as_ref().map(style_draft).transpose()?,
                revision: 0,
                archived_at: None,
                canonical_json: serde_json::to_string(&layer)?,
            })
            .await?;
        Ok(layer)
    }

    pub async fn update_layer(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: UpdateFeatureLayerRequest,
    ) -> Result<FeatureLayer> {
        require_access(identity, AccessLevel::Write)?;
        let mut layer = self
            .layer(identity, scope, &request.layer_id)
            .await?
            .context("unknown feature layer")?;
        ensure_current_layer(&layer, request.expected_layer_revision)?;
        if let Some(title) = request.title {
            validate_layer_title(&title)?;
            layer.title = title;
        }
        if let Some(description) = request.description {
            validate_description(&description)?;
            layer.description = Some(description);
        }
        let new_schema = if let Some(property_schema) = request.property_schema {
            if self
                .store
                .count_map_feature_heads(
                    &scope.identity.tenant_key,
                    identity.authority.work_context.as_str(),
                    layer.layer_id.as_str(),
                )
                .await?
                > 0
            {
                bail!(
                    "property schema changes on a non-empty layer require a versioned migration task"
                );
            }
            let schema = validate_schema(&property_schema)?;
            let revision = FeatureSchemaRevision {
                schema_revision_id: crate::contract::FeatureSchemaRevisionId::new(),
                layer_id: layer.layer_id.clone(),
                version: layer.schema.version + 1,
                digest_sha256: schema.digest_sha256.clone(),
                schema: schema.value.clone(),
                created_at: Utc::now(),
            };
            layer.schema = revision.clone();
            Some(schema_draft(&revision, &schema))
        } else {
            None
        };
        let new_style = if let Some(style) = request.style {
            validate_style(&style)?;
            let revision = MapStyleRevision {
                style_revision_id: StyleRevisionId::new(),
                layer_id: layer.layer_id.clone(),
                version: layer
                    .style
                    .as_ref()
                    .map_or(1, |current| current.version + 1),
                style,
                created_at: Utc::now(),
            };
            layer.style = Some(revision.clone());
            Some(style_draft(&revision)?)
        } else {
            None
        };
        layer.revision += 1;
        layer.updated_at = Utc::now();
        self.store
            .update_map_feature_layer(
                MapFeatureLayerUpdateDraft {
                    identity: scope.identity.clone(),
                    authority: authority_record(identity),
                    layer_key: layer.layer_id.to_string(),
                    title: layer.title.clone(),
                    description: layer.description.clone(),
                    schema_version: integer(layer.schema.version)?,
                    schema_revision_key: layer.schema.schema_revision_id.to_string(),
                    new_schema,
                    style_version: layer
                        .style
                        .as_ref()
                        .map(|style| integer(style.version))
                        .transpose()?,
                    style_revision_key: layer
                        .style
                        .as_ref()
                        .map(|style| style.style_revision_id.to_string()),
                    new_style,
                    revision: integer(layer.revision)?,
                    archived_at: None,
                    canonical_json: serde_json::to_string(&layer)?,
                },
                integer(request.expected_layer_revision)?,
            )
            .await?;
        Ok(layer)
    }

    pub async fn archive_layer(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: ArchiveFeatureLayerRequest,
    ) -> Result<FeatureLayer> {
        require_access(identity, AccessLevel::Admin)?;
        let mut layer = self
            .layer(identity, scope, &request.layer_id)
            .await?
            .context("unknown feature layer")?;
        ensure_current_layer(&layer, request.expected_layer_revision)?;
        let archived_at = Utc::now();
        layer.archived_at = Some(archived_at);
        layer.revision += 1;
        layer.updated_at = archived_at;
        self.store
            .update_map_feature_layer(
                MapFeatureLayerUpdateDraft {
                    identity: scope.identity.clone(),
                    authority: authority_record(identity),
                    layer_key: layer.layer_id.to_string(),
                    title: layer.title.clone(),
                    description: layer.description.clone(),
                    schema_version: integer(layer.schema.version)?,
                    schema_revision_key: layer.schema.schema_revision_id.to_string(),
                    new_schema: None,
                    style_version: layer
                        .style
                        .as_ref()
                        .map(|style| integer(style.version))
                        .transpose()?,
                    style_revision_key: layer
                        .style
                        .as_ref()
                        .map(|style| style.style_revision_id.to_string()),
                    new_style: None,
                    revision: integer(layer.revision)?,
                    archived_at: Some(archived_at),
                    canonical_json: serde_json::to_string(&layer)?,
                },
                integer(request.expected_layer_revision)?,
            )
            .await?;
        Ok(layer)
    }

    pub async fn validate_changes(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: ValidateFeatureChangesRequest,
    ) -> Result<ValidateFeatureChangesOutput> {
        require_access(identity, AccessLevel::Write)?;
        validate_mutation_envelope(&request.mutations)?;
        let prepared = self
            .prepare_changes(
                identity,
                scope,
                request.layer_id.clone(),
                request.expected_layer_revision,
                &request.mutations,
                PreparationMode::Interactive("validation"),
            )
            .await?;
        Ok(ValidateFeatureChangesOutput {
            valid: prepared.findings.is_empty(),
            layer_id: request.layer_id,
            expected_layer_revision: request.expected_layer_revision,
            findings: prepared.findings,
        })
    }

    pub async fn commit_changes(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: CommitFeatureChangesRequest,
    ) -> Result<CommitFeatureChangesOutput> {
        require_access(identity, AccessLevel::Write)?;
        validate_idempotency_key(&request.idempotency_key)?;
        validate_mutation_envelope(&request.mutations)?;
        self.commit_prevalidated_changes(identity, scope, request, true)
            .await
    }

    pub async fn commit_import_changes(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: CommitFeatureChangesRequest,
    ) -> Result<CommitFeatureChangesOutput> {
        require_access(identity, AccessLevel::Write)?;
        validate_idempotency_key(&request.idempotency_key)?;
        if request.mutations.is_empty()
            || request.mutations.len() > crate::contract::MAX_IMPORT_FEATURES
        {
            bail!(
                "an import must contain between one and {} features",
                crate::contract::MAX_IMPORT_FEATURES
            );
        }
        self.commit_prevalidated_changes(identity, scope, request, false)
            .await
    }

    async fn commit_prevalidated_changes(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: CommitFeatureChangesRequest,
        check_create_conflicts: bool,
    ) -> Result<CommitFeatureChangesOutput> {
        let request_value = serde_json::to_value(&request)?;
        let request_digest_sha256 = sha256(&canonical_json(&request_value)?);
        let prepared = self
            .prepare_changes(
                identity,
                scope,
                request.layer_id.clone(),
                request.expected_layer_revision,
                &request.mutations,
                if check_create_conflicts {
                    PreparationMode::Interactive(&request.idempotency_key)
                } else {
                    PreparationMode::BulkImport(&request.idempotency_key)
                },
            )
            .await?;
        if !prepared.findings.is_empty() {
            bail!(
                "feature changes are invalid: {}",
                prepared
                    .findings
                    .iter()
                    .map(|finding| format!("{}: {}", finding.code, finding.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        let mut resulting_layer = prepared.layer.clone();
        resulting_layer.revision += 1;
        resulting_layer.updated_at = Utc::now();
        let scoped_key = map_authoring_idempotency_key(
            &scope.identity.tenant_key,
            identity.authority.work_context.as_str(),
            request.layer_id.as_str(),
            &request.idempotency_key,
        );
        let changeset_id = FeatureChangeSetId::from_stable_key(scoped_key.as_bytes());
        let revisions = prepared
            .features
            .iter()
            .zip(request.mutations.iter())
            .map(|(feature, mutation)| feature_revision_draft(feature, mutation, &changeset_id))
            .collect::<Result<Vec<_>>>()?;
        let result = self
            .store
            .commit_map_feature_changes(MapFeatureCommitDraft {
                identity: scope.identity.clone(),
                authority: authority_record(identity),
                layer_key: request.layer_id.to_string(),
                layer_canonical_json: serde_json::to_string(&resulting_layer)?,
                expected_layer_revision: integer(request.expected_layer_revision)?,
                changeset_key: changeset_id.to_string(),
                idempotency_key: request.idempotency_key.clone(),
                request_digest_sha256: request_digest_sha256.clone(),
                changeset_canonical_json: serde_json::to_string(&serde_json::json!({
                    "changeset_id": changeset_id,
                    "layer_id": request.layer_id,
                    "request_digest_sha256": request_digest_sha256,
                }))?,
                revisions,
            })
            .await?;
        let features = result
            .revisions
            .iter()
            .map(|revision| decode(&revision.canonical_json, "map feature revision"))
            .collect::<Result<Vec<_>>>()?;
        let changeset = changeset_from_record(result.changeset)?;
        let projection_state = match self
            .projection
            .reconcile_through(changeset.commit_sequence)
            .await
        {
            Ok(_) => ProjectionState::Ready,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    changeset_id = %changeset.changeset_id,
                    commit_sequence = changeset.commit_sequence,
                    "canonical map changes committed while the query projection remains pending"
                );
                ProjectionState::Pending
            }
        };
        Ok(CommitFeatureChangesOutput {
            changeset,
            features,
            projection_state,
        })
    }

    pub async fn restore_feature(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: RestoreFeatureRequest,
    ) -> Result<CommitFeatureChangesOutput> {
        self.commit_changes(
            identity,
            scope,
            CommitFeatureChangesRequest {
                layer_id: request.layer_id,
                expected_layer_revision: request.expected_layer_revision,
                idempotency_key: request.idempotency_key,
                mutations: vec![FeatureMutation::Restore {
                    feature_id: request.feature_id,
                    expected_feature_revision: request.expected_feature_revision,
                }],
            },
        )
        .await
    }

    pub async fn query_features(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: QueryFeaturesRequest,
    ) -> Result<QueryFeaturesOutput> {
        require_access(identity, AccessLevel::Read)?;
        self.layer(identity, scope, &request.layer_id)
            .await?
            .context("unknown feature layer")?;
        let publication_revision = if let Some(publication_id) = &request.publication_id {
            Some(
                self.publication(identity, scope, &request.layer_id, publication_id)
                    .await?
                    .context("unknown layer publication")?
                    .layer_revision,
            )
        } else {
            None
        };
        let projection_sequence = if let Some(minimum) = request.minimum_commit_sequence {
            self.projection.reconcile_through(minimum).await?
        } else {
            self.projection.reconcile().await?
        };
        self.projection.query(
            &scope.identity.tenant_key,
            identity.authority.work_context.as_str(),
            &request,
            publication_revision,
            projection_sequence,
        )
    }

    pub async fn publish_layer(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: PublishFeatureLayerRequest,
    ) -> Result<LayerPublication> {
        require_access(identity, AccessLevel::Admin)?;
        let layer = self
            .layer(identity, scope, &request.layer_id)
            .await?
            .context("unknown feature layer")?;
        ensure_current_layer(&layer, request.expected_layer_revision)?;
        let publication = LayerPublication {
            publication_id: LayerPublicationId::new(),
            layer_id: layer.layer_id.clone(),
            layer_revision: layer.revision,
            schema_version: layer.schema.version,
            style_revision_id: layer
                .style
                .as_ref()
                .map(|style| style.style_revision_id.clone()),
            title: request.title,
            artifact_uris: Vec::new(),
            published_by: identity.actor.id.clone(),
            work_context: identity.authority.work_context.clone(),
            published_at: Utc::now(),
        };
        self.store
            .create_map_layer_publication(MapLayerPublicationDraft {
                identity: scope.identity.clone(),
                authority: authority_record(identity),
                publication_key: publication.publication_id.to_string(),
                layer_key: publication.layer_id.to_string(),
                layer_revision: integer(publication.layer_revision)?,
                schema_version: integer(publication.schema_version)?,
                style_revision_key: publication
                    .style_revision_id
                    .as_ref()
                    .map(ToString::to_string),
                artifact_uris: publication.artifact_uris.clone(),
                canonical_json: serde_json::to_string(&publication)?,
                published_at: publication.published_at,
            })
            .await?;
        Ok(publication)
    }

    pub async fn layer(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
    ) -> Result<Option<FeatureLayer>> {
        require_access(identity, AccessLevel::Read)?;
        let layer = self
            .store
            .map_feature_layer(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
            )
            .await?
            .map(|record| decode(&record.canonical_json, "feature layer"))
            .transpose()?;
        Ok(layer.filter(|layer| has_layer_clearance(identity, layer)))
    }

    pub async fn list_layers(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        include_archived: bool,
    ) -> Result<Vec<FeatureLayer>> {
        require_access(identity, AccessLevel::Read)?;
        let layers = self
            .store
            .list_map_feature_layers(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                include_archived,
            )
            .await?
            .into_iter()
            .map(|record| decode(&record.canonical_json, "feature layer"))
            .collect::<Result<Vec<_>>>()?;
        Ok(layers
            .into_iter()
            .filter(|layer| has_layer_clearance(identity, layer))
            .collect())
    }

    pub async fn feature(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        feature_id: &MapFeatureId,
    ) -> Result<Option<MapFeature>> {
        require_access(identity, AccessLevel::Read)?;
        if self.layer(identity, scope, layer_id).await?.is_none() {
            return Ok(None);
        }
        self.store
            .map_feature_head(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
                feature_id.as_str(),
            )
            .await?
            .map(|record| decode(&record.canonical_json, "map feature"))
            .transpose()
    }

    pub async fn schema_revision(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        version: u64,
    ) -> Result<Option<FeatureSchemaRevision>> {
        require_access(identity, AccessLevel::Read)?;
        if self.layer(identity, scope, layer_id).await?.is_none() {
            return Ok(None);
        }
        self.store
            .map_feature_schema_revision(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
                integer(version)?,
            )
            .await?
            .map(|record| {
                Ok(FeatureSchemaRevision {
                    schema_revision_id: record.schema_revision_key.parse()?,
                    layer_id: record.layer_key.parse()?,
                    version: u64::try_from(record.schema_version)?,
                    digest_sha256: record.digest_sha256,
                    schema: serde_json::from_str(&record.schema_json)?,
                    created_at: record.created_at,
                })
            })
            .transpose()
    }

    pub async fn style_revision(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        version: u64,
    ) -> Result<Option<MapStyleRevision>> {
        require_access(identity, AccessLevel::Read)?;
        if self.layer(identity, scope, layer_id).await?.is_none() {
            return Ok(None);
        }
        self.store
            .map_style_revision(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
                integer(version)?,
            )
            .await?
            .map(|record| {
                Ok(MapStyleRevision {
                    style_revision_id: record.style_revision_key.parse()?,
                    layer_id: record.layer_key.parse()?,
                    version: u64::try_from(record.style_version)?,
                    style: serde_json::from_str(&record.style_json)?,
                    created_at: record.created_at,
                })
            })
            .transpose()
    }

    pub async fn feature_revision(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        feature_id: &MapFeatureId,
        revision: u64,
    ) -> Result<Option<MapFeature>> {
        require_access(identity, AccessLevel::Read)?;
        if self.layer(identity, scope, layer_id).await?.is_none() {
            return Ok(None);
        }
        self.store
            .map_feature_revision(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
                feature_id.as_str(),
                integer(revision)?,
            )
            .await?
            .map(|record| decode(&record.canonical_json, "map feature revision"))
            .transpose()
    }

    pub async fn changeset(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        changeset_id: &FeatureChangeSetId,
    ) -> Result<Option<FeatureChangeSet>> {
        require_access(identity, AccessLevel::Read)?;
        if self.layer(identity, scope, layer_id).await?.is_none() {
            return Ok(None);
        }
        self.store
            .map_feature_changeset(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
                changeset_id.as_str(),
            )
            .await?
            .map(changeset_from_record)
            .transpose()
    }

    pub async fn publication(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        publication_id: &LayerPublicationId,
    ) -> Result<Option<LayerPublication>> {
        require_access(identity, AccessLevel::Read)?;
        if self.layer(identity, scope, layer_id).await?.is_none() {
            return Ok(None);
        }
        self.store
            .map_layer_publication(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.as_str(),
                publication_id.as_str(),
            )
            .await?
            .map(|record| decode::<LayerPublication>(&record.canonical_json, "layer publication"))
            .transpose()
    }

    pub async fn list_publications(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: Option<&crate::contract::FeatureLayerId>,
    ) -> Result<Vec<LayerPublication>> {
        require_access(identity, AccessLevel::Read)?;
        if let Some(layer_id) = layer_id
            && self.layer(identity, scope, layer_id).await?.is_none()
        {
            return Ok(Vec::new());
        }
        let visible_layers = if layer_id.is_none() {
            Some(
                self.list_layers(identity, scope, true)
                    .await?
                    .into_iter()
                    .map(|layer| layer.layer_id)
                    .collect::<BTreeSet<_>>(),
            )
        } else {
            None
        };
        self.store
            .list_map_layer_publications(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                layer_id.map(crate::contract::FeatureLayerId::as_str),
            )
            .await?
            .into_iter()
            .map(|record| decode::<LayerPublication>(&record.canonical_json, "layer publication"))
            .collect::<Result<Vec<_>>>()
            .map(|publications| {
                publications
                    .into_iter()
                    .filter(|publication| {
                        visible_layers
                            .as_ref()
                            .is_none_or(|layers| layers.contains(&publication.layer_id))
                    })
                    .collect()
            })
    }

    async fn prepare_changes(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: crate::contract::FeatureLayerId,
        expected_layer_revision: u64,
        mutations: &[FeatureMutation],
        mode: PreparationMode<'_>,
    ) -> Result<PreparedChanges> {
        let layer = self
            .layer(identity, scope, &layer_id)
            .await?
            .context("unknown feature layer")?;
        let mut findings = Vec::new();
        if layer.archived_at.is_some() {
            findings.push(finding(
                0,
                "layer_archived",
                "the feature layer is archived",
            ));
        }
        if layer.revision != expected_layer_revision {
            findings.push(finding(
                0,
                "layer_revision_conflict",
                format!(
                    "expected layer revision {expected_layer_revision}, current revision is {}",
                    layer.revision
                ),
            ));
        }
        let mut seen = BTreeSet::new();
        let mut features = Vec::with_capacity(mutations.len());
        for (index, mutation) in mutations.iter().enumerate() {
            let result = self
                .prepare_mutation(
                    identity,
                    scope,
                    &layer,
                    expected_layer_revision + 1,
                    mutation,
                    mode.stable_seed(),
                    index,
                    mode.checks_create_conflicts(),
                )
                .await;
            match result {
                Ok(feature) => {
                    if !seen.insert(feature.id.clone()) {
                        findings.push(finding(
                            index,
                            "duplicate_feature",
                            "a changeset may mutate a feature only once",
                        ));
                    }
                    features.push(feature);
                }
                Err(error) => findings.push(finding(
                    index,
                    "invalid_feature_mutation",
                    error.to_string(),
                )),
            }
        }
        Ok(PreparedChanges {
            layer,
            features,
            findings,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_mutation(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer: &FeatureLayer,
        resulting_layer_revision: u64,
        mutation: &FeatureMutation,
        stable_seed: &str,
        index: usize,
        check_create_conflicts: bool,
    ) -> Result<MapFeature> {
        let now = Utc::now();
        let provenance = provenance(identity);
        let feature = match mutation {
            FeatureMutation::Create { feature } => {
                validate_input(&layer.schema.schema, feature)?;
                let feature_id = feature.feature_id.clone().unwrap_or_else(|| {
                    MapFeatureId::from_stable_key(
                        format!(
                            "{}\0{}\0{}\0{stable_seed}\0{index}",
                            scope.identity.tenant_key,
                            identity.authority.work_context,
                            layer.layer_id
                        )
                        .as_bytes(),
                    )
                });
                if check_create_conflicts
                    && self
                        .feature(identity, scope, &layer.layer_id, &feature_id)
                        .await?
                        .is_some()
                {
                    bail!("feature {feature_id} already exists");
                }
                map_feature(
                    feature_id,
                    layer,
                    1,
                    resulting_layer_revision,
                    feature,
                    provenance,
                    now,
                )
            }
            FeatureMutation::Replace {
                feature_id,
                expected_feature_revision,
                feature,
            } => {
                if feature
                    .feature_id
                    .as_ref()
                    .is_some_and(|input_id| input_id != feature_id)
                {
                    bail!("replacement feature_id does not match the mutation target");
                }
                validate_input(&layer.schema.schema, feature)?;
                let current = self
                    .feature(identity, scope, &layer.layer_id, feature_id)
                    .await?
                    .context("replacement target does not exist")?;
                if current.deleted {
                    bail!("a tombstoned feature must be restored before replacement");
                }
                ensure_feature_revision(&current, *expected_feature_revision)?;
                map_feature(
                    feature_id.clone(),
                    layer,
                    current.feature_revision + 1,
                    resulting_layer_revision,
                    feature,
                    provenance,
                    now,
                )
            }
            FeatureMutation::Tombstone {
                feature_id,
                expected_feature_revision,
            } => {
                let mut current = self
                    .feature(identity, scope, &layer.layer_id, feature_id)
                    .await?
                    .context("tombstone target does not exist")?;
                if current.deleted {
                    bail!("feature is already tombstoned");
                }
                ensure_feature_revision(&current, *expected_feature_revision)?;
                current.feature_revision += 1;
                current.layer_revision = resulting_layer_revision;
                current.deleted = true;
                current.provenance = provenance;
                current.created_at = now;
                current
            }
            FeatureMutation::Restore {
                feature_id,
                expected_feature_revision,
            } => {
                let mut current = self
                    .feature(identity, scope, &layer.layer_id, feature_id)
                    .await?
                    .context("restore target does not exist")?;
                if !current.deleted {
                    bail!("feature is not tombstoned");
                }
                ensure_feature_revision(&current, *expected_feature_revision)?;
                current.feature_revision += 1;
                current.layer_revision = resulting_layer_revision;
                current.deleted = false;
                current.provenance = provenance;
                current.created_at = now;
                current
            }
        };
        validate_feature(&feature, &layer.schema.schema)?;
        Ok(feature)
    }
}

fn map_feature(
    feature_id: MapFeatureId,
    layer: &FeatureLayer,
    feature_revision: u64,
    layer_revision: u64,
    input: &FeatureInput,
    provenance: FeatureProvenance,
    created_at: chrono::DateTime<Utc>,
) -> MapFeature {
    MapFeature {
        feature_type: GeoJsonFeatureType::Feature,
        conforms_to: vec![
            JSON_FG_CORE_CONFORMANCE.to_owned(),
            JSON_FG_TYPES_SCHEMAS_CONFORMANCE.to_owned(),
        ],
        id: feature_id,
        geometry: input.geometry.clone(),
        properties: input.properties.clone(),
        semantic_type: input.semantic_type.clone(),
        time: input.time.clone(),
        layer_id: layer.layer_id.clone(),
        feature_revision,
        layer_revision,
        schema_version: layer.schema.version,
        deleted: false,
        title: input.title.clone(),
        related_resources: input.related_resources.clone(),
        evidence_resources: input.evidence_resources.clone(),
        provenance,
        created_at,
    }
}

fn feature_revision_draft(
    feature: &MapFeature,
    mutation: &FeatureMutation,
    _changeset_id: &FeatureChangeSetId,
) -> Result<MapFeatureRevisionDraft> {
    let bbox = feature.geometry.bounding_box();
    let expected_feature_revision = match mutation {
        FeatureMutation::Create { .. } => None,
        FeatureMutation::Replace {
            expected_feature_revision,
            ..
        }
        | FeatureMutation::Tombstone {
            expected_feature_revision,
            ..
        }
        | FeatureMutation::Restore {
            expected_feature_revision,
            ..
        } => Some(integer(*expected_feature_revision)?),
    };
    Ok(MapFeatureRevisionDraft {
        feature_key: feature.id.to_string(),
        feature_revision: integer(feature.feature_revision)?,
        layer_revision: integer(feature.layer_revision)?,
        schema_version: integer(feature.schema_version)?,
        deleted: feature.deleted,
        geometry_type: wire(&feature.geometry.geometry_type())?,
        geometry_json: feature.geometry.to_geojson_string()?,
        bbox_west: bbox.west,
        bbox_south: bbox.south,
        bbox_east: bbox.east,
        bbox_north: bbox.north,
        valid_from: feature
            .time
            .as_ref()
            .and_then(|time| time.interval[0].as_timestamp()),
        valid_until: feature
            .time
            .as_ref()
            .and_then(|time| time.interval[1].as_timestamp()),
        semantic_type: feature.semantic_type.clone(),
        title: feature.title.clone(),
        canonical_json: serde_json::to_string(feature)?,
        expected_feature_revision,
    })
}

fn schema_draft(
    revision: &FeatureSchemaRevision,
    validated: &ValidatedSchema,
) -> MapFeatureSchemaDraft {
    MapFeatureSchemaDraft {
        schema_revision_key: revision.schema_revision_id.to_string(),
        schema_version: i64::try_from(revision.version).expect("schema version is bounded"),
        digest_sha256: validated.digest_sha256.clone(),
        schema_json: validated.canonical_json.clone(),
    }
}

fn style_draft(revision: &MapStyleRevision) -> Result<MapStyleRevisionDraft> {
    Ok(MapStyleRevisionDraft {
        style_revision_key: revision.style_revision_id.to_string(),
        style_version: integer(revision.version)?,
        style_json: serde_json::to_string(&revision.style)?,
    })
}

fn changeset_from_record(
    record: veoveo_platform_store::MapFeatureChangeSetRecord,
) -> Result<FeatureChangeSet> {
    Ok(FeatureChangeSet {
        changeset_id: record.changeset_key.parse()?,
        layer_id: record.layer_key.parse()?,
        base_layer_revision: u64::try_from(record.base_layer_revision)?,
        resulting_layer_revision: u64::try_from(record.resulting_layer_revision)?,
        feature_ids: record
            .feature_keys
            .into_iter()
            .map(|key| key.parse())
            .collect::<Result<Vec<_>, _>>()?,
        idempotency_key: record.idempotency_key,
        request_digest_sha256: record.request_digest_sha256,
        actor_id: record.actor_key.parse()?,
        work_context: record.work_context_key.parse()?,
        commit_sequence: u64::try_from(record.commit_sequence)?,
        created_at: record.created_at,
    })
}

fn ensure_current_layer(layer: &FeatureLayer, expected: u64) -> Result<()> {
    if layer.archived_at.is_some() {
        bail!("feature layer is archived");
    }
    if layer.revision != expected {
        bail!(
            "feature layer revision conflict: expected {expected}, current revision is {}",
            layer.revision
        );
    }
    Ok(())
}

fn ensure_feature_revision(feature: &MapFeature, expected: u64) -> Result<()> {
    if feature.feature_revision != expected {
        bail!(
            "feature revision conflict: expected {expected}, current revision is {}",
            feature.feature_revision
        );
    }
    Ok(())
}

fn validate_mutation_envelope(mutations: &[FeatureMutation]) -> Result<()> {
    if mutations.is_empty() || mutations.len() > MAX_DIRECT_FEATURE_MUTATIONS {
        bail!("a direct changeset must contain between one and 100 mutations");
    }
    if serde_json::to_vec(mutations)?.len() > MAX_DIRECT_FEATURE_BYTES {
        bail!("a direct changeset exceeds 1 MiB; use import_feature_layer");
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("idempotency_key must be 1..=256 bytes without control characters");
    }
    Ok(())
}

fn validate_layer_title(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("feature layer title must be 1..=256 bytes without control characters");
    }
    Ok(())
}

fn validate_description(value: &str) -> Result<()> {
    if value.len() > 4096 || value.chars().any(char::is_control) {
        bail!("feature layer description exceeds 4096 bytes or contains control characters");
    }
    Ok(())
}

pub(super) fn require_access(
    identity: &GatewayInternalIdentity,
    required: AccessLevel,
) -> Result<()> {
    if !identity.authority.artifact_access().allows(required) {
        bail!(
            "Work Context membership {:?} does not grant {required:?} authoring access",
            identity.authority.membership
        );
    }
    Ok(())
}

fn has_layer_clearance(identity: &GatewayInternalIdentity, layer: &FeatureLayer) -> bool {
    layer.data_labels.is_subset(&identity.actor.data_labels)
}

fn provenance(identity: &GatewayInternalIdentity) -> FeatureProvenance {
    let (invocation_mode, initiator_id, delegation_id) = match &identity.authority.provenance {
        InvocationProvenance::Direct { initiator } => {
            (InvocationMode::Direct, Some(initiator.clone()), None)
        }
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            InvocationMode::Delegated,
            Some(initiator.clone()),
            Some(delegation_id.clone()),
        ),
        InvocationProvenance::Automated => (InvocationMode::Automated, None, None),
    };
    FeatureProvenance {
        actor_id: identity.actor.id.clone(),
        work_context: identity.authority.work_context.clone(),
        policy_revision: identity.authority.policy_revision.clone(),
        invocation_mode,
        initiator_id,
        delegation_id,
    }
}

pub(super) fn authority_record(identity: &GatewayInternalIdentity) -> InvocationAuthorityRecord {
    let authority = &identity.authority;
    let (invocation_mode, initiator_key, delegation_id) = match &authority.provenance {
        InvocationProvenance::Direct { initiator } => (
            StoreInvocationMode::Direct,
            Some(initiator.to_string()),
            None,
        ),
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            StoreInvocationMode::Delegated,
            Some(initiator.to_string()),
            Some(delegation_id.to_string()),
        ),
        InvocationProvenance::Automated => (StoreInvocationMode::Automated, None, None),
    };
    let (owner_kind, owner_key) = subject_record(&authority.output_policy.owner);
    InvocationAuthorityRecord {
        context_key: authority.work_context.to_string(),
        membership: store_membership(authority.membership),
        policy_revision: authority.policy_revision.to_string(),
        owner_kind,
        owner_key,
        initial_grants: authority
            .output_policy
            .initial_grants
            .iter()
            .map(|grant| {
                let (subject_kind, subject_key) = subject_record(&grant.subject);
                WorkContextInitialGrantRecord {
                    subject_kind,
                    subject_key,
                    permission: match grant.level {
                        AccessLevel::Read => GrantPermission::Read,
                        AccessLevel::Write => GrantPermission::Write,
                        AccessLevel::Admin => GrantPermission::Admin,
                    },
                }
            })
            .collect(),
        classification: authority
            .output_policy
            .classification
            .as_ref()
            .map(ToString::to_string),
        data_labels: authority
            .output_policy
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        invocation_mode,
        initiator_key,
        delegation_id,
    }
}

fn subject_record(subject: &AccessSubject) -> (ArtifactGrantSubjectKind, String) {
    match subject {
        AccessSubject::Principal(principal) => {
            (ArtifactGrantSubjectKind::Principal, principal.to_string())
        }
        AccessSubject::Group(group) => (ArtifactGrantSubjectKind::Group, group.to_string()),
    }
}

fn store_membership(
    membership: WorkContextMembershipLevel,
) -> veoveo_platform_store::WorkContextMembershipLevel {
    match membership {
        WorkContextMembershipLevel::Viewer => {
            veoveo_platform_store::WorkContextMembershipLevel::Viewer
        }
        WorkContextMembershipLevel::Contributor => {
            veoveo_platform_store::WorkContextMembershipLevel::Contributor
        }
        WorkContextMembershipLevel::Custodian => {
            veoveo_platform_store::WorkContextMembershipLevel::Custodian
        }
        WorkContextMembershipLevel::Owner => {
            veoveo_platform_store::WorkContextMembershipLevel::Owner
        }
    }
}

pub(super) fn wire<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(ToOwned::to_owned)
        .context("wire enum did not serialize as a string")
}

pub(super) fn integer(value: u64) -> Result<i64> {
    i64::try_from(value).context("revision exceeds signed database range")
}

fn sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn finding(
    mutation_index: usize,
    code: impl Into<String>,
    message: impl Into<String>,
) -> FeatureValidationFinding {
    FeatureValidationFinding {
        mutation_index,
        code: code.into(),
        message: message.into(),
    }
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(value: &str, kind: &str) -> Result<T> {
    serde_json::from_str(value).with_context(|| format!("decoding persisted {kind}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_envelope_bounds_mutations_and_bytes() {
        assert!(validate_mutation_envelope(&[]).is_err());
    }

    #[test]
    fn idempotency_keys_reject_control_characters() {
        assert!(validate_idempotency_key("request-1").is_ok());
        assert!(validate_idempotency_key("request\n1").is_err());
    }
}
