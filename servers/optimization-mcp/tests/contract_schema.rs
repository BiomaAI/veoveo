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
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contract-schemas.json");
    if std::env::var_os("UPDATE_CONTRACT_SCHEMAS").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&schemas).unwrap() + "\n",
        )
        .unwrap();
    }
    assert_eq!(
        schemas,
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap()
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

#[test]
fn solver_source_variants_and_nested_artifact_models_are_closed() {
    let resource = serde_json::json!({"source":"resource","uri":OptimizationProblemUri::new(ProblemId::new()).unwrap()});
    let artifact = serde_json::json!({"source":"artifact","model":{"uri":veoveo_artifact_contract::ArtifactId::new().plane_uri(),"format":"optimization_json_v1"}});
    for input in [resource, artifact] {
        assert!(serde_json::from_value::<ConvexProblemSource>(input.clone()).is_ok());
        assert!(serde_json::from_value::<MilpProblemSource>(input.clone()).is_ok());
        let mut extra = input;
        extra["undeclared"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ConvexProblemSource>(extra.clone()).is_err());
        assert!(serde_json::from_value::<MilpProblemSource>(extra).is_err());
    }
    let extra = serde_json::json!({"source":"artifact","model":{"uri":veoveo_artifact_contract::ArtifactId::new().plane_uri(),"format":"optimization_json_v1","undeclared":true}});
    assert!(serde_json::from_value::<ConvexProblemSource>(extra).is_err());
}

#[test]
fn nonnegative_number_schema_matches_admission() {
    use schemars::JsonSchema;
    assert_eq!(NonNegativeF64::schema_name(), f64::schema_name());
    assert_eq!(NonNegativeF64::inline_schema(), f64::inline_schema());
    assert_ne!(NonNegativeF64::schema_id(), f64::schema_id());
    let schema = serde_json::to_value(schemars::schema_for!(NonNegativeF64)).unwrap();
    assert_eq!(schema["type"], "number");
    assert_eq!(schema["format"], "double");
    assert_eq!(schema["minimum"], 0.0);
    assert!(serde_json::from_str::<NonNegativeF64>("0").is_ok());
    assert!(serde_json::from_str::<NonNegativeF64>("-1").is_err());
}
