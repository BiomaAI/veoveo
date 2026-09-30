//! Artifact owns its resource families; foundation owns URI component encoding.
use std::sync::LazyLock;
use veoveo_artifact_contract::ArtifactId;
use veoveo_types::{
    ResourceAddress, ResourceScheme, ResourceUri, ResourceUriBuilder, ResourceUriError,
    ResourceUriParts, UriAuthority, UriSegment,
};

static SCHEME: LazyLock<ResourceScheme> =
    LazyLock::new(|| ResourceScheme::new("artifact").expect("declared scheme"));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactDocument {
    Agents,
    Design,
}
impl ArtifactDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
}
impl TryFrom<&str> for ArtifactDocument {
    type Error = ResourceUriError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(ResourceUriError::DisallowedComponent),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactResource {
    LibraryApp,
    Index,
    Docs,
    Document(ArtifactDocument),
    Contract,
    Occurrence(ArtifactId),
    Metadata(ArtifactId),
    Grants(ArtifactId),
}

impl ArtifactResource {
    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        let parts = ResourceUriParts::parse(value)?;
        if value == super::LIBRARY_APP_URI {
            return Ok(Self::LibraryApp);
        }
        if parts.scheme() != "artifact" || parts.has_query() || value.contains('%') {
            return Err(ResourceUriError::DisallowedComponent);
        }
        let segments = parts.path_segments().collect::<Vec<_>>();
        let path = segments
            .iter()
            .map(|segment| segment.as_ref())
            .collect::<Vec<_>>();
        let id =
            |value| ArtifactId::parse(value).map_err(|_| ResourceUriError::DisallowedComponent);
        match (parts.authority(), path.as_slice()) {
            ("index", []) => Ok(Self::Index),
            ("docs", []) => Ok(Self::Docs),
            ("docs", [doc]) => Ok(Self::Document(ArtifactDocument::try_from(*doc)?)),
            ("contract", []) => Ok(Self::Contract),
            ("metadata", [value]) => Ok(Self::Metadata(id(*value)?)),
            ("grants", [value]) => Ok(Self::Grants(id(*value)?)),
            (value, []) => Ok(Self::Occurrence(id(value)?)),
            _ => Err(ResourceUriError::DisallowedComponent),
        }
    }

    pub fn to_uri(self) -> ResourceUri {
        if self == Self::LibraryApp {
            return ResourceUriBuilder::new("ui://artifact")
                .expect("declared App root")
                .segment(UriSegment::new("library.html").expect("declared App document"))
                .build()
                .expect("declared App URI");
        }
        let (authority, segment) = match self {
            Self::LibraryApp => unreachable!("App address handled above"),
            Self::Index => ("index".into(), None),
            Self::Docs => ("docs".into(), None),
            Self::Contract => ("contract".into(), None),
            Self::Document(doc) => ("docs".into(), Some(doc.as_str().to_owned())),
            Self::Metadata(id) => ("metadata".into(), Some(id.to_string())),
            Self::Grants(id) => ("grants".into(), Some(id.to_string())),
            Self::Occurrence(id) => (id.to_string(), None),
        };
        let mut builder = ResourceUriBuilder::from_components(
            &SCHEME,
            UriAuthority::new(authority).expect("declared name or UUID"),
        )
        .expect("Artifact authority");
        if let Some(segment) = segment {
            builder = builder.segment(UriSegment::new(segment).expect("document name or UUID"));
        }
        builder.build().expect("declared Artifact resource")
    }
}
impl ResourceAddress for ArtifactResource {
    type Error = ResourceUriError;
    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok((*self).to_uri())
    }
}

pub fn doc_uri(doc: ArtifactDocument) -> ResourceUri {
    ArtifactResource::Document(doc).to_uri()
}
pub fn metadata_uri(id: ArtifactId) -> ResourceUri {
    ArtifactResource::Metadata(id).to_uri()
}
pub fn grants_uri(id: ArtifactId) -> ResourceUri {
    ArtifactResource::Grants(id).to_uri()
}
pub fn parse_doc_uri(uri: &str) -> Option<ArtifactDocument> {
    match ArtifactResource::parse(uri).ok()? {
        ArtifactResource::Document(doc) => Some(doc),
        _ => None,
    }
}
pub fn parse_metadata_uri(uri: &str) -> Option<ArtifactId> {
    match ArtifactResource::parse(uri).ok()? {
        ArtifactResource::Metadata(id) => Some(id),
        _ => None,
    }
}
pub fn parse_grants_uri(uri: &str) -> Option<ArtifactId> {
    match ArtifactResource::parse(uri).ok()? {
        ArtifactResource::Grants(id) => Some(id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_families_round_trip_and_reject_ambiguous_components() {
        let id = ArtifactId::new();
        for resource in [
            ArtifactResource::LibraryApp,
            ArtifactResource::Index,
            ArtifactResource::Docs,
            ArtifactResource::Contract,
            ArtifactResource::Document(ArtifactDocument::Agents),
            ArtifactResource::Document(ArtifactDocument::Design),
            ArtifactResource::Occurrence(id),
            ArtifactResource::Metadata(id),
            ArtifactResource::Grants(id),
        ] {
            assert_eq!(
                ArtifactResource::parse(resource.to_uri().as_str()).unwrap(),
                resource
            );
            assert_eq!(
                <ArtifactResource as ResourceAddress>::parse(&resource.to_uri()).unwrap(),
                resource
            );
            for suffix in ["/", "/extra", "?", "?a=1&a=2", "#fragment"] {
                assert!(
                    ArtifactResource::parse(&format!("{}{suffix}", resource.to_uri())).is_err()
                );
            }
        }
        for uri in [
            "artifact://docs/%61gents",
            "artifact://docs/agents%2fdesign",
            "artifact://docs/unknown",
            "artifact://metadata/not-an-id",
            "artifact://grants/../index",
            "artifact://user@index",
            "artifact://index:42",
            "artifact://index/\n",
            "media://index",
        ] {
            assert!(ArtifactResource::parse(uri).is_err(), "{uri:?}");
        }
        assert_eq!(parse_metadata_uri(grants_uri(id).as_str()), None);
        assert_eq!(parse_grants_uri(metadata_uri(id).as_str()), None);
    }
}
