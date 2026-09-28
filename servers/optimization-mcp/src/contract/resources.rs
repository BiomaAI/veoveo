//! Domain routes compose the foundational URI builder without MCP dependencies.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{
    OptimizationCollectionUri, OptimizationContractError, OptimizationProblemUri,
    OptimizationProfileUri, OptimizationRunUri, OptimizationSolutionUri, OptimizationTaskUsageUri,
    OptimizationUsageIndexUri, RunId, SolutionId, uris,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationDocument {
    Agents,
    Design,
}

impl OptimizationDocument {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Design => "design",
        }
    }
    pub fn parse(value: &str) -> Result<Self, OptimizationContractError> {
        match value {
            "agents" => Ok(Self::Agents),
            "design" => Ok(Self::Design),
            _ => Err(invalid()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
pub enum OptimizationResource {
    Capabilities,
    Profiles,
    Profile(OptimizationProfileUri),
    Collection(OptimizationCollectionUri),
    Problem(OptimizationProblemUri),
    Run(OptimizationRunUri),
    RunIncumbents(RunId),
    Solution(OptimizationSolutionUri),
    SolutionRoutes(SolutionId),
    SolutionVariables(SolutionId),
    SolutionVerification(SolutionId),
    Usage(OptimizationUsageIndexUri),
    TaskUsage(OptimizationTaskUsageUri),
    Artifact(ArtifactId),
    Docs,
    Document(OptimizationDocument),
    Contract,
    RoutesApp,
    ModelsApp,
}

impl OptimizationResource {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationContractError> {
        let value = value.as_ref();
        let parts = ResourceUriParts::parse(value).map_err(|_| invalid())?;
        let path = parts.path_segments().collect::<Vec<_>>();
        let segments = path.iter().map(|part| part.as_ref()).collect::<Vec<_>>();
        let resource = match (parts.scheme(), parts.authority(), segments.as_slice()) {
            ("optimization", "problems" | "runs" | "solutions", []) => {
                Self::Collection(OptimizationCollectionUri::parse(value).map_err(|_| invalid())?)
            }
            ("optimization", "usage", []) => {
                Self::Usage(OptimizationUsageIndexUri::parse(value).map_err(|_| invalid())?)
            }
            ("optimization", "usage", ["task", _]) => {
                Self::TaskUsage(OptimizationTaskUsageUri::parse(value).map_err(|_| invalid())?)
            }
            _ if parts.has_query() => return Err(invalid()),
            ("optimization", "capabilities", []) => Self::Capabilities,
            ("optimization", "profiles", []) => Self::Profiles,
            ("optimization", "profile", [_]) => {
                Self::Profile(OptimizationProfileUri::parse(value)?)
            }
            ("optimization", "problem", [_]) => {
                Self::Problem(OptimizationProblemUri::parse(value)?)
            }
            ("optimization", "run", [_]) => Self::Run(OptimizationRunUri::parse(value)?),
            ("optimization", "run", [id, "incumbents"]) => Self::RunIncumbents(RunId::parse(*id)?),
            ("optimization", "solution", [_]) => {
                Self::Solution(OptimizationSolutionUri::parse(value)?)
            }
            ("optimization", "solution", [id, "routes"]) => {
                Self::SolutionRoutes(SolutionId::parse(*id)?)
            }
            ("optimization", "solution", [id, "variables"]) => {
                Self::SolutionVariables(SolutionId::parse(*id)?)
            }
            ("optimization", "solution", [id, "verification"]) => {
                Self::SolutionVerification(SolutionId::parse(*id)?)
            }
            ("optimization", "artifact", [_]) => Self::Artifact(
                ArtifactUri::parse(value)
                    .map_err(|_| invalid())?
                    .artifact_id(),
            ),
            ("optimization", "docs", []) => Self::Docs,
            ("optimization", "docs", [id]) => Self::Document(OptimizationDocument::parse(id)?),
            ("optimization", "contract", []) => Self::Contract,
            ("ui", "optimization", ["routes.html"]) => Self::RoutesApp,
            ("ui", "optimization", ["models.html"]) => Self::ModelsApp,
            _ => return Err(invalid()),
        };
        if resource.to_uri()?.as_str() != value {
            return Err(invalid());
        }
        Ok(resource)
    }
}

impl ResourceAddress for OptimizationResource {
    type Error = OptimizationContractError;
    fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        match self {
            Self::Profile(uri) => uri.to_uri(),
            Self::Problem(uri) => uri.to_uri(),
            Self::Run(uri) => uri.to_uri(),
            Self::Solution(uri) => uri.to_uri(),
            Self::Collection(uri) => uri.to_uri().map_err(|_| invalid()),
            Self::Usage(uri) => uri.to_uri().map_err(|_| invalid()),
            Self::TaskUsage(uri) => uri.to_uri().map_err(|_| invalid()),
            Self::Artifact(id) => ArtifactUri::presented(&uris::SCHEME, *id)
                .to_uri()
                .map_err(|_| invalid()),
            Self::RunIncumbents(id) => child("optimization://run", id.as_str(), "incumbents"),
            Self::SolutionRoutes(id) => child("optimization://solution", id.as_str(), "routes"),
            Self::SolutionVariables(id) => {
                child("optimization://solution", id.as_str(), "variables")
            }
            Self::SolutionVerification(id) => {
                child("optimization://solution", id.as_str(), "verification")
            }
            Self::Document(id) => ResourceUriBuilder::new(uris::DOCS_URI)
                .map_err(|_| invalid())?
                .segment(UriSegment::new(id.as_str()).map_err(|_| invalid())?)
                .build()
                .map_err(|_| invalid()),
            Self::Capabilities => literal(uris::CAPABILITIES_URI),
            Self::Profiles => literal(uris::PROFILES_URI),
            Self::Docs => literal(uris::DOCS_URI),
            Self::Contract => literal(uris::CONTRACT_URI),
            Self::RoutesApp => literal(uris::ROUTES_APP_URI),
            Self::ModelsApp => literal(uris::MODELS_APP_URI),
        }
    }
}

impl TryFrom<String> for OptimizationResource {
    type Error = OptimizationContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}
impl From<OptimizationResource> for String {
    fn from(value: OptimizationResource) -> Self {
        value
            .to_uri()
            .expect("admitted Optimization resource")
            .to_string()
    }
}

fn invalid() -> OptimizationContractError {
    OptimizationContractError::InvalidUri("Optimization resource")
}
fn literal(value: &str) -> Result<ResourceUri, OptimizationContractError> {
    ResourceUri::new(value).map_err(|_| invalid())
}
fn child(root: &str, id: &str, child: &str) -> Result<ResourceUri, OptimizationContractError> {
    ResourceUriBuilder::new(root)
        .map_err(|_| invalid())?
        .segment(UriSegment::new(id).map_err(|_| invalid())?)
        .segment(UriSegment::new(child).map_err(|_| invalid())?)
        .build()
        .map_err(|_| invalid())
}
