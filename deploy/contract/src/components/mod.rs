//! Pure atomic deployment ownership and pre-mutation selection.
//!
//! This module does not execute tools or prove that a Kubernetes write happened.
//! Renderers provide complete selected units; the lock supplies unselected inventories.

mod plan;
mod types;
mod validation;

pub use plan::{component_mutation_plan, select_components};
pub use types::*;
pub use validation::{atomic_unit_digest, lock_component, validate_component_catalog};

/// Internal, non-secret preflight evidence format.
pub const COMPONENT_MUTATION_PLAN_SCHEMA: &str = "veoveo.io/component-mutation-plan/v1";
