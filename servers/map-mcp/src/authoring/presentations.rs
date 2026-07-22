use std::collections::BTreeSet;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use veoveo_mcp_contract::{AccessLevel, GatewayInternalIdentity};
use veoveo_platform_store::{
    MapCompositionDraft, MapCompositionRevisionDraft, MapCompositionUpdateDraft,
    MapLayerProductDraft,
};

use crate::{
    catalog::MapScope,
    contract::{
        ArchiveMapCompositionRequest, CompositionLayer, CreateMapCompositionRequest, LayerProduct,
        LayerProductId, MAX_COMPOSITION_LAYERS, MapComposition, MapCompositionId,
        MapCompositionRevision, MapCompositionRevisionId, UpdateMapCompositionRequest,
    },
};

use super::{
    AuthoringService,
    service::{authority_record, decode, integer, require_access, wire},
};

impl AuthoringService {
    pub async fn create_composition(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: CreateMapCompositionRequest,
    ) -> Result<MapComposition> {
        require_access(identity, AccessLevel::Write)?;
        validate_title(&request.title)?;
        self.validate_composition_layers(identity, scope, &request.layers)
            .await?;
        request.view.validate().map_err(anyhow::Error::msg)?;
        let now = Utc::now();
        let composition_id = MapCompositionId::new();
        let revision = MapCompositionRevision {
            composition_revision_id: MapCompositionRevisionId::new(),
            composition_id: composition_id.clone(),
            revision: 1,
            layers: request.layers,
            view: request.view,
            created_by: identity.actor.id.clone(),
            created_at: now,
        };
        let composition = MapComposition {
            composition_id: composition_id.clone(),
            title: request.title,
            current: revision.clone(),
            owner: identity.authority.output_policy.owner.clone(),
            created_by: identity.actor.id.clone(),
            work_context: identity.authority.work_context.clone(),
            classification: identity.authority.output_policy.classification.clone(),
            data_labels: identity.authority.output_policy.data_labels.clone(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        };
        self.store()
            .create_map_composition(MapCompositionDraft {
                identity: scope.identity.clone(),
                authority: authority_record(identity),
                composition_key: composition_id.to_string(),
                title: composition.title.clone(),
                revision: revision_draft(&revision)?,
                canonical_json: serde_json::to_string(&composition)?,
            })
            .await?;
        Ok(composition)
    }

    pub async fn update_composition(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: UpdateMapCompositionRequest,
    ) -> Result<MapComposition> {
        require_access(identity, AccessLevel::Write)?;
        let mut composition = self
            .composition(identity, scope, &request.composition_id)
            .await?
            .context("unknown map composition")?;
        if composition.archived_at.is_some() {
            bail!("map composition is archived");
        }
        if composition.current.revision != request.expected_revision {
            bail!(
                "map composition revision conflict: expected {}, current revision is {}",
                request.expected_revision,
                composition.current.revision
            );
        }
        if let Some(title) = request.title {
            validate_title(&title)?;
            composition.title = title;
        }
        self.validate_composition_layers(identity, scope, &request.layers)
            .await?;
        request.view.validate().map_err(anyhow::Error::msg)?;
        let revision = MapCompositionRevision {
            composition_revision_id: MapCompositionRevisionId::new(),
            composition_id: composition.composition_id.clone(),
            revision: request.expected_revision + 1,
            layers: request.layers,
            view: request.view,
            created_by: identity.actor.id.clone(),
            created_at: Utc::now(),
        };
        composition.current = revision.clone();
        composition.updated_at = revision.created_at;
        self.store()
            .update_map_composition(
                MapCompositionUpdateDraft {
                    identity: scope.identity.clone(),
                    authority: authority_record(identity),
                    composition_key: composition.composition_id.to_string(),
                    title: composition.title.clone(),
                    revision: revision_draft(&revision)?,
                    canonical_json: serde_json::to_string(&composition)?,
                    archived_at: None,
                },
                integer(request.expected_revision)?,
            )
            .await?;
        Ok(composition)
    }

    pub async fn archive_composition(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: ArchiveMapCompositionRequest,
    ) -> Result<MapComposition> {
        require_access(identity, AccessLevel::Admin)?;
        let mut composition = self
            .composition(identity, scope, &request.composition_id)
            .await?
            .context("unknown map composition")?;
        if composition.archived_at.is_some() {
            bail!("map composition is already archived");
        }
        if composition.current.revision != request.expected_revision {
            bail!("map composition revision conflict");
        }
        let now = Utc::now();
        let revision = MapCompositionRevision {
            composition_revision_id: MapCompositionRevisionId::new(),
            composition_id: composition.composition_id.clone(),
            revision: request.expected_revision + 1,
            layers: composition.current.layers.clone(),
            view: composition.current.view.clone(),
            created_by: identity.actor.id.clone(),
            created_at: now,
        };
        composition.current = revision.clone();
        composition.archived_at = Some(now);
        composition.updated_at = now;
        self.store()
            .update_map_composition(
                MapCompositionUpdateDraft {
                    identity: scope.identity.clone(),
                    authority: authority_record(identity),
                    composition_key: composition.composition_id.to_string(),
                    title: composition.title.clone(),
                    revision: revision_draft(&revision)?,
                    canonical_json: serde_json::to_string(&composition)?,
                    archived_at: Some(now),
                },
                integer(request.expected_revision)?,
            )
            .await?;
        Ok(composition)
    }

    pub async fn composition(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        composition_id: &MapCompositionId,
    ) -> Result<Option<MapComposition>> {
        require_access(identity, AccessLevel::Read)?;
        let value = self
            .store()
            .map_composition(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                composition_id.as_str(),
            )
            .await?
            .map(|record| decode::<MapComposition>(&record.canonical_json, "map composition"))
            .transpose()?;
        Ok(value.filter(|composition| {
            composition
                .data_labels
                .is_subset(&identity.actor.data_labels)
        }))
    }

    pub async fn list_compositions(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        include_archived: bool,
    ) -> Result<Vec<MapComposition>> {
        require_access(identity, AccessLevel::Read)?;
        Ok(self
            .store()
            .list_map_compositions(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                include_archived,
            )
            .await?
            .into_iter()
            .map(|record| decode::<MapComposition>(&record.canonical_json, "map composition"))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|composition| {
                composition
                    .data_labels
                    .is_subset(&identity.actor.data_labels)
            })
            .collect())
    }

    pub async fn composition_revision(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        composition_id: &MapCompositionId,
        revision: u64,
    ) -> Result<Option<MapCompositionRevision>> {
        require_access(identity, AccessLevel::Read)?;
        if self
            .composition(identity, scope, composition_id)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        self.store()
            .map_composition_revision(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                composition_id.as_str(),
                integer(revision)?,
            )
            .await?
            .map(|record| decode(&record.canonical_json, "map composition revision"))
            .transpose()
    }

    pub async fn record_layer_product(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        product: &LayerProduct,
    ) -> Result<LayerProduct> {
        require_access(identity, AccessLevel::Admin)?;
        let publication = self
            .product_publication(identity, scope, &product.layer_id, &product.publication_id)
            .await?;
        if publication.layer_revision != product.layer_revision
            || publication.work_context != product.work_context
            || product.created_by != identity.actor.id
        {
            bail!("layer product authority or publication pin is inconsistent");
        }
        let record = self
            .store()
            .create_map_layer_product(MapLayerProductDraft {
                identity: scope.identity.clone(),
                authority: authority_record(identity),
                product_key: product.product_id.to_string(),
                publication_key: product.publication_id.to_string(),
                layer_key: product.layer_id.to_string(),
                layer_revision: integer(product.layer_revision)?,
                format: wire(&product.format)?,
                artifact_uri: product.artifact_uri.clone(),
                mime_type: product.mime_type.clone(),
                digest_sha256: product.digest_sha256.clone(),
                size_bytes: integer(product.size_bytes)?,
                feature_count: integer(product.feature_count)?,
                canonical_json: serde_json::to_string(product)?,
                created_by_key: product.created_by.to_string(),
                created_at: product.created_at,
            })
            .await?;
        decode(&record.canonical_json, "map layer product")
    }

    pub async fn product_publication(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        publication_id: &crate::contract::LayerPublicationId,
    ) -> Result<crate::contract::LayerPublication> {
        require_access(identity, AccessLevel::Admin)?;
        self.publication(identity, scope, layer_id, publication_id)
            .await?
            .context("unknown layer publication")
    }

    pub async fn layer_product(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        product_id: &LayerProductId,
    ) -> Result<Option<LayerProduct>> {
        require_access(identity, AccessLevel::Read)?;
        self.store()
            .map_layer_product(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                product_id.as_str(),
            )
            .await?
            .map(|record| decode(&record.canonical_json, "map layer product"))
            .transpose()
    }

    pub async fn list_layer_products(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        publication_id: Option<&crate::contract::LayerPublicationId>,
    ) -> Result<Vec<LayerProduct>> {
        require_access(identity, AccessLevel::Read)?;
        self.store()
            .list_map_layer_products(
                &scope.identity.tenant_key,
                identity.authority.work_context.as_str(),
                publication_id.map(crate::contract::LayerPublicationId::as_str),
            )
            .await?
            .into_iter()
            .map(|record| decode(&record.canonical_json, "map layer product"))
            .collect()
    }

    async fn validate_composition_layers(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layers: &[CompositionLayer],
    ) -> Result<()> {
        if layers.is_empty() || layers.len() > MAX_COMPOSITION_LAYERS {
            bail!("a map composition must contain between 1 and {MAX_COMPOSITION_LAYERS} layers");
        }
        let mut seen = BTreeSet::new();
        for layer in layers {
            if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
                bail!("composition layer opacity must be within [0, 1]");
            }
            if !seen.insert(layer.layer_id.clone()) {
                bail!("a map composition may include a feature layer only once");
            }
            let publication = self
                .publication(identity, scope, &layer.layer_id, &layer.publication_id)
                .await?
                .context("composition references an unknown layer publication")?;
            if let Some(style_revision_id) = &layer.style_revision_id
                && publication.style_revision_id.as_ref() != Some(style_revision_id)
            {
                bail!("composition style must be the revision pinned by its publication");
            }
        }
        Ok(())
    }
}

fn revision_draft(revision: &MapCompositionRevision) -> Result<MapCompositionRevisionDraft> {
    Ok(MapCompositionRevisionDraft {
        composition_revision_key: revision.composition_revision_id.to_string(),
        revision: integer(revision.revision)?,
        publication_keys: revision
            .layers
            .iter()
            .map(|layer| layer.publication_id.to_string())
            .collect(),
        canonical_json: serde_json::to_string(revision)?,
    })
}

fn validate_title(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("map composition title must be 1..=256 bytes without control characters");
    }
    Ok(())
}
