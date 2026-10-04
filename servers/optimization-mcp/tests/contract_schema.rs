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

#[test]
fn exact_address_routes_select_the_parent_before_identifier_admission() {
    // A wrong parent is an address error even when its apparent ID is malformed.
    assert!(matches!(
        OptimizationProblemUri::parse("optimization://run/not-an-id"),
        Err(OptimizationContractError::InvalidUri(
            "OptimizationProblemUri"
        ))
    ));
    assert!(matches!(
        OptimizationProblemUri::parse("optimization://problem/not-an-id"),
        Err(OptimizationContractError::InvalidIdentifier(_))
    ));
    let address = OptimizationProblemUri::new(ProblemId::new()).unwrap();
    assert_eq!(
        address.resource_components_uri().unwrap().as_str(),
        address.as_str()
    );
    assert_eq!(
        OptimizationProblemUri::RESOURCE_ROUTES[0]
            .discovery_template()
            .unwrap(),
        OptimizationProblemUri::RESOURCE_TEMPLATE
    );
}
