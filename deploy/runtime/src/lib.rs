//! Shared execution boundary for Veoveo disposable deployment profiles.
//!
//! The contract crate owns validation and schemas. This crate owns source checkouts,
//! rendering and operational tools. Enterprise installations retain their GitOps owner.

mod charts;
mod cluster;
mod compile;
mod configuration;
mod discovery;
mod gpu;
mod helm_bundle;
mod helm_state;
mod images;
mod installed;
mod ownership;
mod process;
mod profile;
mod publication;
mod snapshot;
mod sources;

pub use charts::lock_source_charts;
pub use cluster::{
    profile_cluster_delete, profile_cluster_stop, profile_cluster_up, profile_registry_up,
};
pub use compile::compile_component_lock;
pub use profile::{profile_down, profile_gpu_verify, profile_up, profile_validate};
pub use publication::{ComponentUpdates, update_components};

#[cfg(test)]
mod tests;
