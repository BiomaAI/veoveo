//! Private native runtime. Authorization and durable claims belong to Computers.
// Preserve the pinned generated protobuf API and upstream documentation.
// These allowances do not cover handwritten runtime or allocator types.
#[allow(clippy::large_enum_variant, clippy::doc_lazy_continuation)]
pub mod protocol {
    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("openshell");
    pub mod datamodel {
        pub mod v1 {
            tonic::include_proto!("openshell.datamodel.v1");
        }
    }
    pub mod sandbox {
        pub mod v1 {
            tonic::include_proto!("openshell.sandbox.v1");
        }
    }
    pub mod v1 {
        tonic::include_proto!("openshell.v1");
    }
    pub mod terminal {
        pub mod v1 {
            tonic::include_proto!("openshell.terminal.v1");
        }
    }
}
mod allocation;
mod binding;
mod canonical;
mod client;
mod execution;
mod models;
mod policy_continuity;
mod policy_json;
mod recovery;
mod remote_access;
mod storage;
mod terminal;
mod terminal_output;

pub use allocation::{AllocationConfig, HomeAllocator};
pub use binding::Binding;
pub use client::{GATEWAY_VERSION, GatewayConfig, OpenShellRuntime};
pub use execution::{ExecChunk, ExecInput, ExecIntent, ExecResult, OutputStream};
pub use models::{
    DevelopmentTemplate, MAX_CHUNK_BYTES, Observation, Phase, Result, RuntimeFailure, TerminalSize,
};
pub use policy_continuity::{PolicyRestoration, ReplacementPolicy};
pub use policy_json::parse_policy;
pub use recovery::{LifecycleCheckpoint, LifecycleObservation};
pub use remote_access::OpenShellAccess;
pub use storage::{PERSISTENT_BUILD_COMMAND, PERSISTENT_COMMAND, PERSISTENT_HOME, PersistentHome};
pub use terminal::Terminal;
pub use terminal_output::TerminalOutput;

#[cfg(test)]
mod tests;
