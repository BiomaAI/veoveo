//! Every address Map serves: its resources and its knowledge-source pages and
//! members. The shared host parses each requested URI into this type.

use veoveo_types::{ResourceAddress, ResourceUri};

use super::{MapKnowledgeMember, MapKnowledgePageUri, MapResource, MapResourceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapAddress {
    Resource(MapResource),
    KnowledgePage(MapKnowledgePageUri),
    KnowledgeMember(MapKnowledgeMember),
}

impl MapAddress {
    pub fn parse(value: &str) -> Result<Self, MapResourceError> {
        if let Ok(page) = MapKnowledgePageUri::parse(value) {
            return Ok(Self::KnowledgePage(page));
        }
        if let Ok(member) = MapKnowledgeMember::parse(value) {
            return Ok(Self::KnowledgeMember(member));
        }
        MapResource::parse(value).map(Self::Resource)
    }

    pub fn to_uri(&self) -> ResourceUri {
        match self {
            Self::Resource(resource) => resource.to_uri(),
            Self::KnowledgePage(page) => page.to_uri(),
            Self::KnowledgeMember(member) => member.to_uri(),
        }
    }
}

impl ResourceAddress for MapAddress {
    type Error = MapResourceError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(self.to_uri())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{MapKnowledgeCollection, MapRoot};

    #[test]
    fn knowledge_addresses_and_resources_round_trip() {
        let page =
            MapAddress::KnowledgePage(MapKnowledgePageUri::new(MapKnowledgeCollection::Layers));
        let datasets = MapAddress::Resource(MapResource::Root(MapRoot::Datasets));
        for address in [page, datasets] {
            assert_eq!(
                MapAddress::parse(address.to_uri().as_str()).unwrap(),
                address
            );
        }
        assert!(MapAddress::parse("map://knowledge/unknown").is_err());
        assert!(MapAddress::parse("other://datasets").is_err());
    }
}
