//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum SumoTaskKind {
        RunBatch => "run_batch",
        GenerateNetwork => "generate_network",
        ComputeRoutes => "compute_routes",
        OptimizeSignals => "optimize_signals",
    }
}
