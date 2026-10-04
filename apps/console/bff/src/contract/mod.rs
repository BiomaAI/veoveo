//! Source-owner DTOs shared by browser producers and generated clients.
pub mod apps;
pub mod cluster;
pub use apps::{AppCatalog, AppDescriptor, AppToolDescriptor};
pub use cluster::{
    ClusterIngress, ClusterOrchestrator, ClusterPod, ClusterService, ClusterSnapshot,
    ClusterStorage, ClusterWorkload, ClusterWorkloadKind,
};
