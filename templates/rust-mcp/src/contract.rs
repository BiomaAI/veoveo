//! The Glossary contract: identifiers, resource addresses and tool shapes.
//!
//! Every controlled shape is a type. Requests deserialize into these types, so
//! an invalid identifier is rejected before domain code runs.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

pub const SCHEME: &str = "glossary";
pub const DOCS_URI: &str = "glossary://docs";
pub const CONTRACT_URI: &str = "glossary://contract";
pub const TERMS_URI: &str = "glossary://terms";
pub const TERM_ROOT: &str = "glossary://term";
pub const TERM_TEMPLATE: &str = "glossary://term/{term_id}";

/// Glossary adds no scopes; gateway operation policy governs access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, veoveo_types::Vocabulary)]
#[vocabulary(scope)]
pub enum GlossaryScope {}

/// An invalid identifier or resource address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlossaryError;

impl fmt::Display for GlossaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Glossary identifier or address")
    }
}

impl std::error::Error for GlossaryError {}

/// A term identifier: 1 to 64 lowercase ASCII letters, digits and hyphens,
/// starting with a letter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(
    with = "String",
    description = "Term identifier, such as `hosted-server`."
)]
pub struct TermId(String);

impl TermId {
    pub fn new(value: impl Into<String>) -> Result<Self, GlossaryError> {
        let value = value.into();
        let valid = (1..=64).contains(&value.len())
            && value.starts_with(|c: char| c.is_ascii_lowercase())
            && value
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        valid.then_some(Self(value)).ok_or(GlossaryError)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TermId {
    type Error = GlossaryError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TermId> for String {
    fn from(value: TermId) -> Self {
        value.0
    }
}

impl fmt::Display for TermId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The embedded crate documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlossaryDocument {
    Agents,
    Design,
}

impl GlossaryDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }

    pub fn parse(value: &str) -> Result<Self, GlossaryError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(GlossaryError),
        }
    }
}

/// Every address the server serves. The host parses each requested URI into
/// this type before the domain sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlossaryResource {
    Docs,
    Document(GlossaryDocument),
    Contract,
    Terms,
    Term(TermId),
}

impl ResourceAddress for GlossaryResource {
    type Error = GlossaryError;

    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        let parts = ResourceUriParts::parse(uri.as_str()).map_err(|_| GlossaryError)?;
        if parts.has_query() {
            return Err(GlossaryError);
        }
        let segments = parts.path_segments().collect::<Vec<_>>();
        let segments = segments.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        let address = match (parts.scheme(), parts.authority(), segments.as_slice()) {
            (SCHEME, "docs", []) => Self::Docs,
            (SCHEME, "docs", [id]) => Self::Document(GlossaryDocument::parse(id)?),
            (SCHEME, "contract", []) => Self::Contract,
            (SCHEME, "terms", []) => Self::Terms,
            (SCHEME, "term", [id]) => Self::Term(TermId::new(*id)?),
            _ => return Err(GlossaryError),
        };
        // One spelling per address: reject anything that does not round-trip.
        if address.to_uri()? != *uri {
            return Err(GlossaryError);
        }
        Ok(address)
    }

    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let (root, segment) = match self {
            Self::Docs => (DOCS_URI, None),
            Self::Document(doc) => (DOCS_URI, Some(doc.as_str())),
            Self::Contract => (CONTRACT_URI, None),
            Self::Terms => (TERMS_URI, None),
            Self::Term(id) => (TERM_ROOT, Some(id.as_str())),
        };
        let mut builder = ResourceUriBuilder::new(root).map_err(|_| GlossaryError)?;
        if let Some(segment) = segment {
            builder = builder.segment(UriSegment::new(segment).map_err(|_| GlossaryError)?);
        }
        builder.build().map_err(|_| GlossaryError)
    }
}

/// Input of the `define` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DefineRequest {
    /// The term to define.
    pub term: TermId,
}

/// One glossary entry, the output of `define` and the body of a term resource.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Definition {
    pub term: TermId,
    pub title: String,
    pub definition: String,
    /// Related terms, each readable at `glossary://term/{term_id}`.
    pub see_also: Vec<TermId>,
}

/// One row of the term index.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TermSummary {
    pub term: TermId,
    pub title: String,
    pub uri: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(uri: &str) -> Result<GlossaryResource, GlossaryError> {
        GlossaryResource::parse(&ResourceUri::new(uri).map_err(|_| GlossaryError)?)
    }

    #[test]
    fn addresses_round_trip_and_reject_other_spellings() {
        let term = GlossaryResource::Term(TermId::new("hosted-server").unwrap());
        assert_eq!(
            term.to_uri().unwrap().as_str(),
            "glossary://term/hosted-server"
        );
        assert_eq!(parse("glossary://term/hosted-server").unwrap(), term);
        assert_eq!(parse(TERMS_URI).unwrap(), GlossaryResource::Terms);
        for uri in [
            "glossary://term/Hosted-Server",
            "glossary://term/hosted-server/extra",
            "glossary://terms?cursor=1",
            "glossary://docs/readme",
            "other://term/hosted-server",
        ] {
            assert!(parse(uri).is_err(), "{uri}");
        }
    }

    #[test]
    fn term_ids_are_lowercase_slugs() {
        assert!(TermId::new("domain-read").is_ok());
        for bad in ["", "1st", "Upper", "under_score", &"a".repeat(65)] {
            assert!(TermId::new(bad).is_err(), "{bad}");
        }
    }
}
