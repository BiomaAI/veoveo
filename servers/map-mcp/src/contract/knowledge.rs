//! Map-owned collection names, member addresses and collection-bound keysets.
use super::{
    DatasetReleaseId, FacilityId, FeatureLayerId, LayerPublicationId, LocationId, MapDatasetId,
    MapFeatureId, MapScope,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MapKnowledgeCollection {
    Layers,
    Features,
    Publications,
    Locations,
    Facilities,
    Releases,
}
impl MapKnowledgeCollection {
    pub const ALL: [Self; 6] = [
        Self::Layers,
        Self::Features,
        Self::Publications,
        Self::Locations,
        Self::Facilities,
        Self::Releases,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Layers => "layers",
            Self::Features => "features",
            Self::Publications => "publications",
            Self::Locations => "locations",
            Self::Facilities => "facilities",
            Self::Releases => "releases",
        }
    }
    pub fn scope(self) -> MapScope {
        match self {
            Self::Layers | Self::Features | Self::Publications => MapScope::FeatureRead,
            Self::Locations | Self::Facilities | Self::Releases => MapScope::DatasetRead,
        }
    }
    pub fn member_template(self) -> &'static str {
        match self {
            Self::Layers => "map://feature-layer/{layer_id}/knowledge",
            Self::Features => "map://feature-layer/{layer_id}/feature/{feature_id}/knowledge",
            Self::Publications => {
                "map://feature-layer/{layer_id}/publication/{publication_id}/knowledge"
            }
            Self::Locations => "map://location/{location_id}/knowledge",
            Self::Facilities => "map://facility/{facility_id}/knowledge",
            Self::Releases => "map://dataset/{dataset_id}/release/{release_id}/knowledge",
        }
    }
    pub fn page_template(self) -> &'static str {
        match self {
            Self::Layers => "map://knowledge/layers{?cursor}",
            Self::Features => "map://knowledge/features{?cursor}",
            Self::Publications => "map://knowledge/publications{?cursor}",
            Self::Locations => "map://knowledge/locations{?cursor}",
            Self::Facilities => "map://knowledge/facilities{?cursor}",
            Self::Releases => "map://knowledge/releases{?cursor}",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields,
    rename_all_fields = "camelCase"
)]
pub enum MapKnowledgeMember {
    Layer {
        layer: FeatureLayerId,
    },
    Feature {
        layer: FeatureLayerId,
        feature: MapFeatureId,
    },
    Publication {
        layer: FeatureLayerId,
        publication: LayerPublicationId,
    },
    Location {
        location: LocationId,
    },
    Facility {
        facility: FacilityId,
    },
    Release {
        dataset: MapDatasetId,
        release: DatasetReleaseId,
    },
}
impl MapKnowledgeMember {
    pub fn collection(&self) -> MapKnowledgeCollection {
        match self {
            Self::Layer { .. } => MapKnowledgeCollection::Layers,
            Self::Feature { .. } => MapKnowledgeCollection::Features,
            Self::Publication { .. } => MapKnowledgeCollection::Publications,
            Self::Location { .. } => MapKnowledgeCollection::Locations,
            Self::Facility { .. } => MapKnowledgeCollection::Facilities,
            Self::Release { .. } => MapKnowledgeCollection::Releases,
        }
    }
    pub fn source_uri(&self) -> ResourceUri {
        let (root, path): (&str, Vec<&str>) = match self {
            Self::Layer { layer } => ("map://feature-layer", vec![layer.as_str()]),
            Self::Feature { layer, feature } => (
                "map://feature-layer",
                vec![layer.as_str(), "feature", feature.as_str()],
            ),
            Self::Publication { layer, publication } => (
                "map://feature-layer",
                vec![layer.as_str(), "publication", publication.as_str()],
            ),
            Self::Location { location } => ("map://location", vec![location.as_str()]),
            Self::Facility { facility } => ("map://facility", vec![facility.as_str()]),
            Self::Release { dataset, release } => (
                "map://dataset",
                vec![dataset.as_str(), "release", release.as_str()],
            ),
        };
        let mut builder = ResourceUriBuilder::new(root).expect("Map root");
        for part in path {
            builder = builder.segment(UriSegment::new(part).expect("typed Map component"));
        }
        builder.build().expect("typed Map address")
    }
    pub fn to_uri(&self) -> ResourceUri {
        ResourceUriBuilder::new(self.source_uri().as_str())
            .expect("typed Map source")
            .segment(UriSegment::new("knowledge").expect("summary suffix"))
            .build()
            .expect("Map knowledge summary")
    }
    pub fn parse(value: &str) -> Result<Self, MapKnowledgeAddressError> {
        let parts = ResourceUriParts::parse(value).map_err(|_| MapKnowledgeAddressError)?;
        if parts.scheme() != "map" || parts.has_query() {
            return Err(MapKnowledgeAddressError);
        }
        let mut path: Vec<_> = parts.path_segments().map(|v| v.into_owned()).collect();
        if path.pop().as_deref() != Some("knowledge") {
            return Err(MapKnowledgeAddressError);
        }
        let member = match (parts.authority(), path.as_slice()) {
            ("feature-layer", [layer]) => Self::Layer { layer: id(layer)? },
            ("feature-layer", [layer, marker, feature]) if marker == "feature" => Self::Feature {
                layer: id(layer)?,
                feature: id(feature)?,
            },
            ("feature-layer", [layer, marker, publication]) if marker == "publication" => {
                Self::Publication {
                    layer: id(layer)?,
                    publication: id(publication)?,
                }
            }
            ("location", [location]) => Self::Location {
                location: id(location)?,
            },
            ("facility", [facility]) => Self::Facility {
                facility: id(facility)?,
            },
            ("dataset", [dataset, marker, release]) if marker == "release" => Self::Release {
                dataset: id(dataset)?,
                release: id(release)?,
            },
            _ => return Err(MapKnowledgeAddressError),
        };
        if member.to_uri().as_str() != value {
            return Err(MapKnowledgeAddressError);
        }
        Ok(member)
    }
}
impl ResourceAddress for MapKnowledgeMember {
    type Error = MapKnowledgeAddressError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.to_uri())
    }
}
fn id<T: std::str::FromStr>(value: &str) -> Result<T, MapKnowledgeAddressError> {
    value.parse().map_err(|_| MapKnowledgeAddressError)
}
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("invalid Map knowledge address or collection cursor")]
pub struct MapKnowledgeAddressError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MapKnowledgeCursor(MapKnowledgeMember);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct CursorWire {
    version: u8,
    after: MapKnowledgeMember,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct MapKnowledgeCursorCodec;
impl veoveo_types::CursorCodec for MapKnowledgeCursorCodec {
    type Position = MapKnowledgeMember;
    type Error = MapKnowledgeAddressError;
    fn check(&self, _member: &MapKnowledgeMember) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, member: &MapKnowledgeMember) -> Result<String, Self::Error> {
        Ok(hex::encode(
            serde_json::to_vec(&CursorWire {
                version: 1,
                after: member.clone(),
            })
            .expect("typed cursor"),
        ))
    }
    fn decode(&self, wire: &str) -> Result<MapKnowledgeMember, Self::Error> {
        if wire.is_empty() || wire.len() > 2048 {
            return Err(MapKnowledgeAddressError);
        }
        let bytes = hex::decode(wire).map_err(|_| MapKnowledgeAddressError)?;
        let decoded: CursorWire =
            serde_json::from_slice(&bytes).map_err(|_| MapKnowledgeAddressError)?;
        if decoded.version != 1 || self.encode(&decoded.after)? != wire {
            return Err(MapKnowledgeAddressError);
        }
        Ok(decoded.after)
    }
}
impl MapKnowledgeCursor {
    pub fn after(member: MapKnowledgeMember) -> Self {
        Self(member)
    }
    pub fn member(&self) -> &MapKnowledgeMember {
        &self.0
    }
}
impl From<MapKnowledgeCursor> for String {
    fn from(value: MapKnowledgeCursor) -> Self {
        use veoveo_types::CursorCodec;
        MapKnowledgeCursorCodec
            .encode(&value.0)
            .expect("typed cursor")
    }
}
impl TryFrom<String> for MapKnowledgeCursor {
    type Error = MapKnowledgeAddressError;
    fn try_from(wire: String) -> Result<Self, Self::Error> {
        use veoveo_types::CursorCodec;
        let position = MapKnowledgeCursorCodec.decode(&wire)?;
        MapKnowledgeCursorCodec.check(&position)?;
        Ok(Self(position))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapKnowledgePageUri {
    collection: MapKnowledgeCollection,
    cursor: Option<MapKnowledgeCursor>,
}
impl MapKnowledgePageUri {
    pub fn new(collection: MapKnowledgeCollection) -> Self {
        Self {
            collection,
            cursor: None,
        }
    }
    pub fn with_cursor(
        mut self,
        cursor: MapKnowledgeCursor,
    ) -> Result<Self, MapKnowledgeAddressError> {
        if cursor.member().collection() != self.collection {
            return Err(MapKnowledgeAddressError);
        }
        self.cursor = Some(cursor);
        Ok(self)
    }
    pub fn collection(&self) -> MapKnowledgeCollection {
        self.collection
    }
    pub fn after(&self) -> Option<&MapKnowledgeMember> {
        self.cursor.as_ref().map(MapKnowledgeCursor::member)
    }
    pub fn to_uri(&self) -> ResourceUri {
        let mut builder = ResourceUriBuilder::new("map://knowledge")
            .expect("Map root")
            .segment(UriSegment::new(self.collection.name()).expect("Map collection"));
        if let Some(cursor) = &self.cursor {
            builder = builder
                .query_pair("cursor", &String::from(cursor.clone()))
                .expect("typed cursor");
        }
        builder.build().expect("Map page")
    }
    pub fn parse(value: &str) -> Result<Self, MapKnowledgeAddressError> {
        let parts = ResourceUriParts::parse(value).map_err(|_| MapKnowledgeAddressError)?;
        if parts.scheme() != "map" || parts.authority() != "knowledge" {
            return Err(MapKnowledgeAddressError);
        }
        let path: Vec<_> = parts.path_segments().collect();
        let collection = MapKnowledgeCollection::ALL
            .into_iter()
            .find(|c| path.len() == 1 && path[0] == c.name())
            .ok_or(MapKnowledgeAddressError)?;
        let mut page = Self::new(collection);
        for (name, value) in parts.query_parameters() {
            if name != "cursor" || page.cursor.is_some() {
                return Err(MapKnowledgeAddressError);
            }
            page = page.with_cursor(MapKnowledgeCursor::try_from(value.to_string())?)?;
        }
        if page.to_uri().as_str() != value {
            return Err(MapKnowledgeAddressError);
        }
        Ok(page)
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapKnowledgeLink {
    pub uri: ResourceUri,
    pub title: String,
}
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapKnowledgePage {
    pub items: Vec<MapKnowledgeLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<MapKnowledgeCursor>,
}

#[cfg(feature = "knowledge")]
impl MapKnowledgeCollection {
    pub fn descriptor(self) -> veoveo_mcp_knowledge_extension::CollectionDescriptor {
        use veoveo_mcp_knowledge_extension::{
            AccessModel, ChangeSignal, CollectionDescriptor, Freshness, IndexingMode,
        };
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
