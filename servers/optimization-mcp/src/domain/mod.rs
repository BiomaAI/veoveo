//! cuOpt-native Optimization domain contract.
//!
//! The modules in this namespace define routing, convex optimization, and
//! MILP as separate problem families with independent resource identities.

mod common;
mod model;
mod profile;
mod routing;
mod solution;

pub use common::*;
pub use model::*;
pub use profile::*;
pub use routing::*;
pub use solution::*;

pub const OPTIMIZATION_CONTRACT_VERSION: &str = "veoveo.io/optimization/v1";
pub const ROUTING_PROBLEM_VERSION: &str = "veoveo.io/routing-problem/v1";
pub const CONVEX_PROBLEM_VERSION: &str = "veoveo.io/convex-problem/v1";
pub const MILP_PROBLEM_VERSION: &str = "veoveo.io/milp-problem/v1";
pub const TRAVEL_MODEL_ARTIFACT_VERSION: &str = "veoveo.io/travel-model-artifact/v1";
pub const EXECUTOR_PROTOCOL_VERSION: &str = "veoveo.io/cuopt-executor/v1";
pub const CUOPT_STABLE_VERSION: &str = "26.06";
pub const CUOPT_CONTAINER_DIGEST: &str =
    "sha256:0faac7182b32f5be747e30f081427e80e8dfeccd8f49613814d3d217167fe1ad";

pub const MAX_INLINE_MATRIX_CELLS: usize = 16_384;
pub const MAX_INLINE_MODEL_NONZEROS: usize = 16_384;
pub const MAX_ROUTE_CASES: usize = 64;
pub const MAX_CAPACITY_DIMENSIONS: usize = 64;
pub const MAX_OBJECTIVES: usize = 6;
