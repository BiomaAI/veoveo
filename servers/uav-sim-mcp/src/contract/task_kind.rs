//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum UavTaskKind {
    #[vocabulary(rename = "run_scenario")]
    RunScenario,
    #[vocabulary(rename = "execute_vehicle_mission_plan")]
    ExecuteMission,
    #[vocabulary(rename = "capture_dataset")]
    CaptureDataset,
}
