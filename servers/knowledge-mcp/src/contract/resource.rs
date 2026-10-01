use veoveo_knowledge_contract::KnowledgeError;
use veoveo_mcp_knowledge_extension::{CollectionId, DocumentId};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, ServerSlug, UriSegment,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnowledgeResource {
    Sources { after: Option<ServerSlug> },
    Source(ServerSlug),
    Collection(CollectionId),
    Docs { after: Option<DocumentId> },
    Document(DocumentId),
    Contract,
}
impl ResourceAddress for KnowledgeResource {
    type Error = KnowledgeError;
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let (root, segment, after) = match self {
            Self::Sources { after } => (
                "knowledge://sources",
                None,
                after.as_ref().map(ToString::to_string),
            ),
            Self::Source(server) => ("knowledge://source", Some(server.to_string()), None),
            Self::Collection(collection) => {
                ("knowledge://collection", Some(collection.to_string()), None)
            }
            Self::Docs { after } => (
                "knowledge://docs",
                None,
                after.as_ref().map(ToString::to_string),
            ),
            Self::Document(document) => ("knowledge://docs", Some(document.to_string()), None),
            Self::Contract => ("knowledge://contract", None, None),
        };
        let mut builder = ResourceUriBuilder::new(root).map_err(|_| invalid())?;
        if let Some(segment) = segment {
            builder = builder.segment(UriSegment::new(segment).map_err(|_| invalid())?);
        }
        if let Some(after) = after {
            builder = builder
                .query_pair("cursor", &after)
                .map_err(|_| invalid())?;
        }
        builder.build().map_err(|_| invalid())
    }
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        let parts = ResourceUriParts::parse(uri.as_str()).map_err(|_| invalid())?;
        if parts.scheme() != "knowledge" {
            return Err(invalid());
        }
        let path: Vec<_> = parts.path_segments().collect();
        let after = parts.query_parameters().get("cursor");
        if parts.query_parameters().len() > usize::from(after.is_some()) {
            return Err(invalid());
        }
        let resource = match (parts.authority(), path.as_slice()) {
            ("sources", []) => Self::Sources {
                after: after
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|_| invalid())?,
            },
            ("source", [server]) if after.is_none() => {
                Self::Source(server.parse().map_err(|_| invalid())?)
            }
            ("collection", [collection]) if after.is_none() => {
                Self::Collection(collection.parse().map_err(|_| invalid())?)
            }
            ("docs", []) => Self::Docs {
                after: after
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(|_| invalid())?,
            },
            ("docs", [document]) if after.is_none() => {
                Self::Document(document.parse().map_err(|_| invalid())?)
            }
            ("contract", []) if after.is_none() => Self::Contract,
            _ => return Err(invalid()),
        };
        if resource.to_uri()? != *uri {
            return Err(invalid());
        }
        Ok(resource)
    }
}
fn invalid() -> KnowledgeError {
    KnowledgeError("invalid Knowledge resource address")
}
