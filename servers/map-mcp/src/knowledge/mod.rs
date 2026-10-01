//! Knowledge reads reuse Map's source policy and publish typed resource links.
mod authoring;
mod geography;
mod summaries;

use crate::{
    analytics::MapAnalytics,
    catalog::{MapAccessContext, MapCatalog},
    contract::*,
};
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_mcp_knowledge_extension::{
    AccessDescriptor, AccessModel, ChangeSignal, CollectionDescriptor, Freshness, IndexingMode,
    ModifiedBy, Observation, content_digest,
};

pub const PAGE_SIZE: usize = 100;

pub fn search_declaration() -> veoveo_mcp_knowledge_extension::SearchDeclaration {
    veoveo_mcp_knowledge_extension::SearchDeclaration::new(
        [
            MapKnowledgeCollection::Locations,
            MapKnowledgeCollection::Facilities,
        ]
        .into_iter()
        .map(|c| c.descriptor().collection().clone())
        .collect(),
    )
    .expect("Map search collections")
}

impl MapKnowledgeCollection {
    pub fn descriptor(self) -> CollectionDescriptor {
        let authored = matches!(self, Self::Layers | Self::Features | Self::Publications);
        CollectionDescriptor::new(
            format!("map.{}", self.name())
                .parse()
                .expect("Map collection"),
            match self {
                Self::Layers => "feature-layer",
                Self::Features => "feature",
                Self::Publications => "layer-publication",
                Self::Locations => "location",
                Self::Facilities => "facility",
                Self::Releases => "dataset-release",
            }
            .parse()
            .expect("Map entity kind"),
            veoveo_types::ResourceTemplateUri::new(self.page_template())
                .expect("Map page template"),
            Freshness::max_age(300),
            // Projection visibility can advance after the catalog activation event.
            if matches!(self, Self::Locations | Self::Facilities) {
                ChangeSignal::Revalidate
            } else {
                ChangeSignal::Listen
            },
            if authored {
                AccessModel::WorkContext
            } else {
                AccessModel::Profile
            },
            IndexingMode::Content,
        )
        .expect("Map descriptor")
        .with_required_scopes([self.scope().into()])
    }
}

pub struct ObservedMap {
    pub address: MapKnowledgeMember,
    pub title: String,
    text: String,
    access: Option<AccessDescriptor>,
    modified_at: Option<DateTime<Utc>>,
    modified_by: Option<ModifiedBy>,
}
impl ObservedMap {
    fn new<T: summaries::Summarize>(
        address: MapKnowledgeMember,
        title: String,
        value: &T,
        access: Option<AccessDescriptor>,
        modified_at: Option<DateTime<Utc>>,
        modified_by: Option<ModifiedBy>,
    ) -> Result<Self> {
        let title = summaries::excerpt(&title, 256);
        let summary = MapKnowledgeSummary {
            source: address.source_uri(),
            source_sha256: content_digest(&serde_json::to_string(value)?),
            title: title.clone(),
            details: value.details(),
        };
        let text = serde_json::to_string(&summary)?;
        ensure!(
            text.len() <= 64 * 1024,
            "Map knowledge member exceeds 64 KiB"
        );
        Ok(Self {
            address,
            title,
            text,
            access,
            modified_at,
            modified_by,
        })
    }
    pub fn document(&self) -> Result<(String, Observation)> {
        let descriptor = self.address.collection().descriptor();
        let revision = content_digest(&serde_json::to_string(&(&self.text, &self.access))?);
        let mut builder = Observation::builder(
            descriptor.collection().clone(),
            revision.to_string().parse()?,
            content_digest(&self.text),
            Utc::now(),
        );
        if let Some(access) = &self.access {
            builder = builder.access(access.clone());
        }
        if let Some(time) = self.modified_at {
            builder = builder.modified_at(time);
        }
        if let Some(actor) = &self.modified_by {
            builder = builder.modified_by(actor.clone());
        }
        Ok((self.text.clone(), builder.build(&descriptor)?))
    }
}

fn page(mut rows: Vec<ObservedMap>) -> MapKnowledgePage {
    let more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let next_cursor = more
        .then(|| MapKnowledgeCursor::after(rows.last().expect("lookahead page").address.clone()));
    MapKnowledgePage {
        items: rows
            .into_iter()
            .map(|row| MapKnowledgeLink {
                uri: row.address.to_uri(),
                title: summaries::link_title(&row.title),
            })
            .collect(),
        next_cursor,
    }
}

pub async fn enumerate(
    catalog: &MapCatalog,
    analytics: &MapAnalytics,
    identity: &GatewayInternalIdentity,
    scope: &MapAccessContext,
    address: &MapKnowledgePageUri,
) -> Result<MapKnowledgePage> {
    let rows = match address.collection() {
        MapKnowledgeCollection::Layers
        | MapKnowledgeCollection::Features
        | MapKnowledgeCollection::Publications => {
            authoring::select(
                catalog,
                identity,
                scope,
                address.collection(),
                None,
                address.after(),
            )
            .await?
        }
        MapKnowledgeCollection::Locations | MapKnowledgeCollection::Facilities => {
            analytics.knowledge_page(&scope.tenant_key(), address)?
        }
        MapKnowledgeCollection::Releases => {
            let after = match address.after() {
                Some(MapKnowledgeMember::Release { release, .. }) => Some(release),
                _ => None,
            };
            let page = catalog.releases_page(scope, None, after).await?;
            let more = page.next_cursor.is_some();
            let mut rows = page
                .items
                .into_iter()
                .map(release)
                .collect::<Result<Vec<_>>>()?;
            // Catalog pages already have a lookahead cursor; preserve it without a second read.
            let next_cursor = if more {
                rows.last()
                    .map(|r| MapKnowledgeCursor::after(r.address.clone()))
            } else {
                None
            };
            return Ok(MapKnowledgePage {
                items: rows
                    .drain(..)
                    .map(|r| MapKnowledgeLink {
                        uri: r.address.to_uri(),
                        title: summaries::link_title(&r.title),
                    })
                    .collect(),
                next_cursor,
            });
        }
    };
    Ok(page(rows))
}

pub async fn read(
    catalog: &MapCatalog,
    analytics: &MapAnalytics,
    identity: &GatewayInternalIdentity,
    scope: &MapAccessContext,
    address: &MapKnowledgeMember,
) -> Result<Option<ObservedMap>> {
    Ok(match address {
        MapKnowledgeMember::Layer { .. }
        | MapKnowledgeMember::Feature { .. }
        | MapKnowledgeMember::Publication { .. } => {
            let mut rows = authoring::select(
                catalog,
                identity,
                scope,
                address.collection(),
                Some(address),
                None,
            )
            .await?;
            ensure!(rows.len() <= 1, "duplicate Map member identity");
            rows.pop()
        }
        MapKnowledgeMember::Location { location } => analytics
            .location(&scope.tenant_key(), location)?
            .map(geography::location)
            .transpose()?,
        MapKnowledgeMember::Facility { facility } => analytics
            .facility(&scope.tenant_key(), facility)?
            .map(geography::facility)
            .transpose()?,
        MapKnowledgeMember::Release {
            dataset,
            release: id,
        } => catalog
            .release_in_dataset(scope, dataset, id)
            .await?
            .map(release)
            .transpose()?,
    })
}

fn release(value: DatasetRelease) -> Result<ObservedMap> {
    value.validate()?;
    ObservedMap::new(
        MapKnowledgeMember::Release {
            dataset: value.dataset_id.clone(),
            release: value.release_id.clone(),
        },
        value.version_label.clone(),
        &value,
        None,
        Some(value.updated_at),
        None,
    )
}

#[cfg(test)]
mod tests;
