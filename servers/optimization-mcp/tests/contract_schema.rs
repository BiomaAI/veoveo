use veoveo_optimization_mcp::contract::*;

#[test]
fn public_solver_schemas_preserve_the_published_contract() {
    let schemas = serde_json::json!({
    "OptimizeRoutesRequest": schemars::schema_for!(OptimizeRoutesRequest),
    "OptimizeRouteScenariosRequest": schemars::schema_for!(OptimizeRouteScenariosRequest),
    "SolveConvexRequest": schemars::schema_for!(SolveConvexRequest),
    "SolveMilpRequest": schemars::schema_for!(SolveMilpRequest),
    "VerifySolutionRequest": schemars::schema_for!(VerifySolutionRequest),
    "OptimizationToolOutput": schemars::schema_for!(OptimizationToolOutput),
    "VerifySolutionOutput": schemars::schema_for!(VerifySolutionOutput),
    "SolverProfile": schemars::schema_for!(SolverProfile),
    "TravelModelArtifact": schemars::schema_for!(TravelModelArtifact),
    "OptimizationRunRecord": schemars::schema_for!(OptimizationRunRecord),
    "OptimizationSolution": schemars::schema_for!(OptimizationSolution)
    });
    assert_eq!(
        schemas,
        serde_json::from_str::<serde_json::Value>(include_str!("fixtures/contract-schemas.json"))
            .unwrap()
    );
}
