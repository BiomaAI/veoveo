//! Complete hosted route vocabulary, independent of MCP and service dependencies.
use super::TimeseriesArtifactUri;
use super::{TimeseriesTaskUsageUri, TimeseriesUsageIndexUri};
use crate::uris;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeseriesDocument {
    Agents,
    Design,
}
impl TimeseriesDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
    pub fn parse(value: &str) -> Result<Self, TimeseriesResourceError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(TimeseriesResourceError),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum TimeseriesResource {
    Docs,
    Document(TimeseriesDocument),
    Contract,
    ForecastApp,
    Usage(TimeseriesUsageIndexUri),
    TaskUsage(TimeseriesTaskUsageUri),
    Artifact(TimeseriesArtifactUri),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeseriesResourceError;
impl fmt::Display for TimeseriesResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid Timeseries resource URI")
    }
}
impl std::error::Error for TimeseriesResourceError {}

impl TimeseriesResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, TimeseriesResourceError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| TimeseriesResourceError)?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let segments = path.iter().map(|part| part.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), segments.as_slice()) {
            ("timeseries", "usage", []) => Self::Usage(
                TimeseriesUsageIndexUri::parse(value).map_err(|_| TimeseriesResourceError)?,
            ),
            ("timeseries", "usage", ["task", _]) => Self::TaskUsage(
                TimeseriesTaskUsageUri::parse(value).map_err(|_| TimeseriesResourceError)?,
            ),
            _ if parts.has_query() => return Err(TimeseriesResourceError),
            ("timeseries", "artifact", [_]) => Self::Artifact(
                TimeseriesArtifactUri::parse(value).map_err(|_| TimeseriesResourceError)?,
            ),
            ("timeseries", "docs", []) => Self::Docs,
            ("timeseries", "docs", [id]) => Self::Document(TimeseriesDocument::parse(id)?),
            ("timeseries", "contract", []) => Self::Contract,
            ("ui", "timeseries", ["forecast.html"]) => Self::ForecastApp,
            _ => return Err(TimeseriesResourceError),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(TimeseriesResourceError);
        }
        Ok(resource)
    }
}
impl ResourceAddress for TimeseriesResource {
    type Error = TimeseriesResourceError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        match self {
            Self::Usage(uri) => uri.to_uri().map_err(|_| TimeseriesResourceError),
            Self::TaskUsage(uri) => uri.to_uri().map_err(|_| TimeseriesResourceError),
            Self::Artifact(id) => id.to_uri().map_err(|_| TimeseriesResourceError),
            Self::Document(doc) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| TimeseriesResourceError)?
                .segment(UriSegment::new(doc.as_str()).map_err(|_| TimeseriesResourceError)?)
                .build()
                .map_err(|_| TimeseriesResourceError),
            Self::Docs => ResourceUri::new(uris::DOCS_URI).map_err(|_| TimeseriesResourceError),
            Self::Contract => {
                ResourceUri::new(uris::CONTRACT_URI).map_err(|_| TimeseriesResourceError)
            }
            Self::ForecastApp => {
                ResourceUri::new(uris::FORECAST_APP_URI).map_err(|_| TimeseriesResourceError)
            }
        }
    }
}
impl TryFrom<String> for TimeseriesResource {
    type Error = TimeseriesResourceError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<TimeseriesResource> for String {
    fn from(value: TimeseriesResource) -> Self {
        value
            .to_uri()
            .expect("admitted Timeseries address")
            .to_string()
    }
}
