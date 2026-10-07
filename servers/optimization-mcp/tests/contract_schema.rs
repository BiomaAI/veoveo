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
    "TravelModelArtifact": schemars::schema_for!(veoveo_map_mcp::TravelModelArtifact),
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
    let artifact = serde_json::json!({"source":"artifact","model":{"uri":veoveo_artifact_contract::ArtifactId::new().plane_uri(),"format":"optimization_json_v2"}});
    for input in [resource, artifact] {
        assert!(serde_json::from_value::<ConvexProblemSource>(input.clone()).is_ok());
        assert!(serde_json::from_value::<MilpProblemSource>(input.clone()).is_ok());
        let mut extra = input;
        extra["undeclared"] = serde_json::json!(true);
        assert!(serde_json::from_value::<ConvexProblemSource>(extra.clone()).is_err());
        assert!(serde_json::from_value::<MilpProblemSource>(extra).is_err());
    }
    let extra = serde_json::json!({"source":"artifact","model":{"uri":veoveo_artifact_contract::ArtifactId::new().plane_uri(),"format":"optimization_json_v2","undeclared":true}});
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

#[test]
fn every_solver_fixture_publishes_current_members_and_refuses_retired_or_mixed() {
    use serde_json::Value;
    fn roundtrip(tool: &str, wire: Value) -> Result<Value, serde_json::Error> {
        match tool {
            "solve_convex" => {
                serde_json::to_value(serde_json::from_value::<SolveConvexRequest>(wire)?)
            }
            "solve_milp" => serde_json::to_value(serde_json::from_value::<SolveMilpRequest>(wire)?),
            "optimize_routes" => {
                serde_json::to_value(serde_json::from_value::<OptimizeRoutesRequest>(wire)?)
            }
            _ => panic!("unexpected existing owner tool fixture"),
        }
    }
    fn members(value: &Value, path: &str, out: &mut Vec<(String, String, String)>) {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    if key.chars().any(char::is_uppercase) {
                        let retired = key
                            .chars()
                            .flat_map(|c| {
                                if c.is_uppercase() {
                                    vec!['_', c.to_ascii_lowercase()]
                                } else {
                                    vec![c]
                                }
                            })
                            .collect();
                        out.push((path.to_owned(), key.clone(), retired));
                    }
                    members(child, &format!("{path}/{key}"), out);
                }
            }
            Value::Array(array) => {
                for (index, child) in array.iter().enumerate() {
                    members(child, &format!("{path}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let fixture: Value =
        serde_json::from_str(include_str!("../testdata/controlled-inputs.json")).unwrap();
    let cases = fixture
        .as_array()
        .or_else(|| fixture.get("cases").and_then(Value::as_array))
        .unwrap();
    assert_eq!(cases.len(), 34);
    let mut controls = 0;
    for case in cases {
        let tool = case["tool"].as_str().unwrap();
        let current = roundtrip(tool, case["arguments"].clone()).unwrap();
        assert_eq!(roundtrip(tool, current.clone()).unwrap(), current);
        let mut positions = Vec::new();
        members(&current, "", &mut positions);
        for (path, key, retired) in positions {
            for mixed in [false, true] {
                let mut bad = current.clone();
                let object = bad.pointer_mut(&path).unwrap().as_object_mut().unwrap();
                let value = if mixed {
                    object[&key].clone()
                } else {
                    object.remove(&key).unwrap()
                };
                object.insert(retired.clone(), value);
                assert!(
                    roundtrip(tool, bad).is_err(),
                    "{tool}: {path}/{key}, mixed={mixed}"
                );
                controls += 1;
            }
        }
    }
    assert!(controls > 34);
}

#[test]
fn artifact_current_marker_and_problem_revision_refuse_retired_versions() {
    let artifact = serde_json::json!({"source":"artifact","model":{"uri":veoveo_artifact_contract::ArtifactId::new().plane_uri(),"format":"optimization_json_v2"}});
    assert!(serde_json::from_value::<ConvexProblemSource>(artifact.clone()).is_ok());
    let mut retired = artifact;
    retired["model"]["format"] = "optimization_json_v1".into();
    assert!(serde_json::from_value::<ConvexProblemSource>(retired.clone()).is_err());
    assert!(serde_json::from_value::<MilpProblemSource>(retired).is_err());
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/controlled-inputs.json")).unwrap();
    let cases = fixture
        .as_array()
        .or_else(|| fixture.get("cases").and_then(serde_json::Value::as_array))
        .unwrap();
    for (tool, current, retired) in [
        (
            "solve_convex",
            CONVEX_PROBLEM_VERSION,
            "veoveo.ai/convex-problem/v1",
        ),
        (
            "solve_milp",
            MILP_PROBLEM_VERSION,
            "veoveo.ai/milp-problem/v1",
        ),
        (
            "optimize_routes",
            ROUTING_PROBLEM_VERSION,
            "veoveo.ai/routing-problem/v1",
        ),
    ] {
        let case = cases
            .iter()
            .find(|case| case["tool"] == tool && case["arguments"]["problem"]["source"] == "inline")
            .unwrap();
        let mut value = case["arguments"].clone();
        assert_eq!(value["problem"]["problem"]["version"], current);
        value["problem"]["problem"]["version"] = retired.into();
        match tool {
            "solve_convex" => assert!(serde_json::from_value::<SolveConvexRequest>(value).is_err()),
            "solve_milp" => assert!(serde_json::from_value::<SolveMilpRequest>(value).is_err()),
            _ => assert!(serde_json::from_value::<OptimizeRoutesRequest>(value).is_err()),
        }
    }
}
