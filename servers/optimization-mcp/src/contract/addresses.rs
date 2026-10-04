//! Concrete domain addresses retain their parent ID after wire admission.
use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri};

use super::{OptimizationContractError, ProblemId, RunId, SolutionId, SolverProfileId};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "optimization://problem/{problem_id}", error = OptimizationContractError, route_error = |_| Self::error(), wire)]
pub struct OptimizationProblemUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "problem_id", error = |error| error)]
    id: ProblemId,
}
impl OptimizationProblemUri {
    pub fn new(id: ProblemId) -> Result<Self, OptimizationContractError> {
        Self::resource_from_parts(id)
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationContractError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| Self::error())?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &ProblemId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    fn error() -> OptimizationContractError {
        OptimizationContractError::InvalidUri("OptimizationProblemUri")
    }
}
impl fmt::Display for OptimizationProblemUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "optimization://run/{run_id}", error = OptimizationContractError, route_error = |_| Self::error(), wire)]
pub struct OptimizationRunUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "run_id", error = |error| error)]
    id: RunId,
}
impl OptimizationRunUri {
    pub fn new(id: RunId) -> Result<Self, OptimizationContractError> {
        Self::resource_from_parts(id)
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationContractError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| Self::error())?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &RunId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    fn error() -> OptimizationContractError {
        OptimizationContractError::InvalidUri("OptimizationRunUri")
    }
}
impl fmt::Display for OptimizationRunUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "optimization://solution/{solution_id}", error = OptimizationContractError, route_error = |_| Self::error(), wire)]
pub struct OptimizationSolutionUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "solution_id", error = |error| error)]
    id: SolutionId,
}
impl OptimizationSolutionUri {
    pub fn new(id: SolutionId) -> Result<Self, OptimizationContractError> {
        Self::resource_from_parts(id)
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationContractError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| Self::error())?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &SolutionId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    fn error() -> OptimizationContractError {
        OptimizationContractError::InvalidUri("OptimizationSolutionUri")
    }
}
impl fmt::Display for OptimizationSolutionUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire)
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template = "optimization://profile/{profile_id}", error = OptimizationContractError, route_error = |_| Self::error(), wire)]
pub struct OptimizationProfileUri {
    #[resource(cache)]
    wire: String,
    #[resource(variable = "profile_id", error = |error| error)]
    id: SolverProfileId,
}
impl OptimizationProfileUri {
    pub fn new(id: SolverProfileId) -> Result<Self, OptimizationContractError> {
        Self::resource_from_parts(id)
    }
    pub fn parse(value: impl AsRef<str>) -> Result<Self, OptimizationContractError> {
        let uri = ResourceUri::new(value.as_ref()).map_err(|_| Self::error())?;
        <Self as ResourceAddress>::parse(&uri)
    }
    pub fn id(&self) -> &SolverProfileId {
        &self.id
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    fn error() -> OptimizationContractError {
        OptimizationContractError::InvalidUri("OptimizationProfileUri")
    }
}
impl fmt::Display for OptimizationProfileUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire)
    }
}

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
