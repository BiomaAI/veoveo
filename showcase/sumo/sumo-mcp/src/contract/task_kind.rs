//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum SumoTaskKind {
    #[vocabulary(rename = "run_batch")]
    RunBatch,
    #[vocabulary(rename = "generate_network")]
    GenerateNetwork,
    #[vocabulary(rename = "compute_routes")]
    ComputeRoutes,
    #[vocabulary(rename = "optimize_signals")]
    OptimizeSignals,
}
