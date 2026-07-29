use std::num::NonZeroU32;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{NonNegativeF64, OptimizationProfileUri, ProblemFamily, SolverProfileId, UnitInterval};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SolverIntent {
    Interactive,
    Balanced,
    Thorough,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolverProfileDefaults {
    pub routing_deadline_seconds: NonZeroU32,
    pub convex_deadline_seconds: NonZeroU32,
    pub milp_deadline_seconds: NonZeroU32,
    pub convex_optimality_tolerance: NonNegativeF64,
    pub milp_relative_gap: UnitInterval,
    pub milp_absolute_gap: NonNegativeF64,
    pub retain_milp_incumbents: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SolverProfile {
    pub profile_id: SolverProfileId,
    pub profile_uri: OptimizationProfileUri,
    pub title: String,
    pub description: String,
    pub intent: SolverIntent,
    pub supported_families: Vec<ProblemFamily>,
    pub maximum_deadline_seconds: NonZeroU32,
    pub defaults: SolverProfileDefaults,
}
