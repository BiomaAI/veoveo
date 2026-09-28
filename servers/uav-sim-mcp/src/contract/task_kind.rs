//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum UavTaskKind {
        RunScenario => "run_scenario",
        ExecuteMission => "execute_vehicle_mission_plan",
        CaptureDataset => "capture_dataset",
    }
}
