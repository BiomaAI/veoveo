use anyhow::Result;

// Composition-owned source is delivered into this focused client without linking
// the Gateway service composition. Each helper keeps its original module scope.
#[path = "../../../../platform/gateway/composition/src/smoke_support/auth/context.rs"]
pub(crate) mod auth;
#[path = "../../../../platform/gateway/composition/src/smoke_support/gpu.rs"]
mod gpu;
#[path = "../../../../platform/gateway/composition/src/smoke_support/installation.rs"]
mod installation;
pub(crate) use gpu::{NvidiaGpuUuid, parse_single_nvidia_smi_gpu};
pub(crate) use installation::InstalledTarget;
pub(crate) use veoveo_testing_support::{assert_executable, contains, run_checked};
