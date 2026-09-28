//! Concrete domain addresses retain their parent ID after wire admission.
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriParts, UriSegment,
};

use super::{OptimizationContractError, ProblemId, RunId, SolutionId, SolverProfileId};

macro_rules! address {
    ($name:ident, $id:ty, $root:literal, $parse:path) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name {
            wire: String,
            id: $id,
        }

        impl $name {
            pub fn new(id: $id) -> Result<Self, OptimizationContractError> {
                let wire = ResourceUriBuilder::new($root)
                    .expect("declared Optimization root")
                    .segment(UriSegment::new(id.to_string()).map_err(|_| Self::error())?)
                    .build()
                    .map_err(|_| Self::error())?
                    .to_string();
                Ok(Self { wire, id })
            }

            pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationContractError> {
                let value = value.as_ref();
                let parts = ResourceUriParts::parse(value).map_err(|_| Self::error())?;
                let path = parts.path_segments().collect::<Vec<_>>();
                if parts.has_query() || path.len() != 1 {
                    return Err(Self::error());
                }
                let address = Self::new($parse(path[0].as_ref())?)?;
                if address.as_str() != value {
                    return Err(Self::error());
                }
                Ok(address)
            }

            pub fn id(&self) -> &$id {
                &self.id
            }
            pub fn as_str(&self) -> &str {
                &self.wire
            }
            fn error() -> OptimizationContractError {
                OptimizationContractError::InvalidUri(stringify!($name))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.wire)
            }
        }
        impl TryFrom<String> for $name {
            type Error = OptimizationContractError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.wire
            }
        }
        impl ResourceAddress for $name {
            type Error = OptimizationContractError;
            fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
                ResourceUri::new(&self.wire).map_err(|_| Self::error())
            }
            fn parse(value: &ResourceUri) -> Result<Self, Self::Error> {
                Self::parse(value.as_str())
            }
        }
    };
}

address!(
    OptimizationProblemUri,
    ProblemId,
    "optimization://problem",
    ProblemId::parse
);
address!(
    OptimizationRunUri,
    RunId,
    "optimization://run",
    RunId::parse
);
address!(
    OptimizationSolutionUri,
    SolutionId,
    "optimization://solution",
    SolutionId::parse
);
address!(
    OptimizationProfileUri,
    SolverProfileId,
    "optimization://profile",
    SolverProfileId::new
);

/// Builders accept the corresponding parent identity.
/// ```compile_fail
/// use veoveo_optimization_mcp::contract::{OptimizationProblemUri, RunId};
/// OptimizationProblemUri::new(RunId::new());
/// ```
/// ```compile_fail
/// use veoveo_optimization_mcp::contract::OptimizationSolutionUri;
/// OptimizationSolutionUri::new("solution-id");
/// ```
const _: () = ();
