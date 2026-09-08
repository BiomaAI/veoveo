//! Pure atomic deployment ownership and pre-mutation selection.
//!
//! This module does not execute tools or prove that a Kubernetes write happened.
//! Renderers provide complete selected units; the lock supplies unselected inventories.

mod bindings;
mod digest;
mod history;
mod plan;
mod profile;
mod types;
mod validation;

pub(crate) use bindings::validate_artifact_bindings;
pub use bindings::validate_profile_component_bindings;
pub use digest::{atomic_unit_content_digest, atomic_unit_digest};
pub use history::validate_helm_inventory;
pub use plan::{component_mutation_plan, select_components};
pub use profile::{
    ComponentOwner, InstallationInput, ProfileComponent, required_installation_inputs,
    selected_source_releases, validate_profile_components,
};
pub use types::*;
pub use validation::{lock_component, validate_component_catalog};

/// Internal, non-secret preflight evidence format.
pub const COMPONENT_MUTATION_PLAN_SCHEMA: &str = "veoveo.io/component-mutation-plan/v2";

/// Reserved source identity for installation-owned inputs and operations.
pub const INSTALLATION_SOURCE_NAME: &str = "installation";
