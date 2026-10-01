//! Completed-analysis discovery, separate from owner-scoped Task control.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{AnalysisId, ReasonContractError, ids::string_schema};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FindingCollection {
    Analyses,
    Results,
}
impl FindingCollection {
    pub const ALL: [Self; 2] = [Self::Analyses, Self::Results];
    pub fn segment(self) -> &'static str {
        match self {
            Self::Analyses => "analyses",
            Self::Results => "results",
        }
    }
    pub fn page_template(self) -> &'static str {
        match self {
            Self::Analyses => "reason://knowledge/analyses{?cursor}",
            Self::Results => "reason://knowledge/results{?cursor}",
        }
    }
    pub fn member_template(self) -> &'static str {
        match self {
            Self::Analyses => "reason://knowledge/analyses/{analysis_id}",
            Self::Results => "reason://knowledge/results/{analysis_id}",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FindingCursor {
    collection: FindingCollection,
    created_at: DateTime<Utc>,
    analysis: AnalysisId,
}
impl FindingCursor {
    pub fn new(
        collection: FindingCollection,
        created_at: DateTime<Utc>,
        analysis: AnalysisId,
    ) -> Self {
        Self {
            collection,
            created_at,
            analysis,
        }
    }
    pub fn collection(&self) -> FindingCollection {
        self.collection
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn analysis(&self) -> AnalysisId {
        self.analysis
    }
    pub fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&(1_u8, self.collection, self.created_at, self.analysis))
                .expect("fixed cursor"),
        )
    }
    pub fn parse(value: &str) -> Result<Self, ReasonContractError> {
        let bad = || ReasonContractError::InvalidCursor;
        if value.is_empty() || value.len() > 1024 {
            return Err(bad());
        }
        let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| bad())?;
        let (version, collection, created_at, analysis): (
            u8,
            FindingCollection,
            DateTime<Utc>,
            AnalysisId,
        ) = serde_json::from_slice(&bytes).map_err(|_| bad())?;
        let cursor = Self::new(collection, created_at, analysis);
        if version != 1 || cursor.encode() != value {
            return Err(bad());
        }
        Ok(cursor)
    }
}
impl TryFrom<String> for FindingCursor {
    type Error = ReasonContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<FindingCursor> for String {
    fn from(value: FindingCursor) -> Self {
        value.encode()
    }
}
string_schema!(FindingCursor);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum FindingResource {
    Root {
        collection: FindingCollection,
    },
    Page {
        cursor: FindingCursor,
    },
    Member {
        collection: FindingCollection,
        analysis: AnalysisId,
    },
}
impl FindingResource {
    pub fn collection(&self) -> FindingCollection {
        match self {
            Self::Root { collection } | Self::Member { collection, .. } => *collection,
            Self::Page { cursor } => cursor.collection(),
        }
    }
    pub fn root(collection: FindingCollection) -> Self {
        Self::Root { collection }
    }
    pub fn parse(value: &str) -> Result<Self, ReasonContractError> {
        let bad = || ReasonContractError::InvalidResource;
        let parts = ResourceUriParts::parse(value).map_err(|_| bad())?;
        if parts.scheme() != "reason" || parts.authority() != "knowledge" {
            return Err(bad());
        }
        let path = parts.path_segments().collect::<Vec<_>>();
        let collection = match path.first().map(|p| p.as_ref()) {
            Some("analyses") => FindingCollection::Analyses,
            Some("results") => FindingCollection::Results,
            _ => return Err(bad()),
        };
        let address = match path.len() {
            1 => {
                let cursor = match parts
                    .query_parameters()
                    .iter()
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    [] if !parts.has_query() => None,
                    [(key, value)] if key.as_str() == "cursor" => {
                        Some(FindingCursor::parse(value.as_str())?)
                    }
                    _ => return Err(bad()),
                };
                match cursor {
                    Some(cursor) if cursor.collection() == collection => Self::Page { cursor },
                    None => Self::root(collection),
                    _ => return Err(ReasonContractError::InvalidCursor),
                }
            }
            2 if !parts.has_query() => Self::Member {
                collection,
                analysis: AnalysisId::parse(path[1].as_ref())?,
            },
            _ => return Err(bad()),
        };
        if address.to_uri()?.as_str() != value {
            return Err(bad());
        }
        Ok(address)
    }
}
impl ResourceAddress for FindingResource {
    type Error = ReasonContractError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        let bad = |_| ReasonContractError::InvalidResource;
        let mut builder = ResourceUriBuilder::new("reason://knowledge")
            .map_err(bad)?
            .segment(UriSegment::new(self.collection().segment()).map_err(bad)?);
        match self {
            Self::Page { cursor } => {
                builder = builder
                    .query_pair("cursor", &cursor.encode())
                    .map_err(bad)?;
            }
            Self::Root { .. } => {}
            Self::Member { analysis, .. } => {
                builder = builder.segment(UriSegment::new(analysis.to_string()).map_err(bad)?);
            }
        }
        builder.build().map_err(bad)
    }
}
impl TryFrom<String> for FindingResource {
    type Error = ReasonContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<FindingResource> for String {
    fn from(value: FindingResource) -> Self {
        value
            .to_uri()
            .expect("admitted finding address")
            .to_string()
    }
}
string_schema!(FindingResource);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindingIndexEntry {
    pub uri: FindingResource,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FindingPage {
    pub items: Vec<FindingIndexEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<FindingCursor>,
}
