//! Deterministic translation from the public provider-neutral contract to the
//! private cuOpt executor protocol.

mod mathematical;
mod routing;

pub use mathematical::{compile_convex_problem, compile_milp_problem};
pub use routing::{compile_routing_initial_solution, compile_routing_problem};

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum CompileError {
    #[error("invalid optimization problem: {0}")]
    InvalidProblem(String),
    #[error("{0} must be materialized before executor compilation")]
    UnmaterializedInput(&'static str),
    #[error("{field} value {value} cannot be represented by cuOpt {target}")]
    NumericRange {
        field: &'static str,
        value: String,
        target: &'static str,
    },
}
