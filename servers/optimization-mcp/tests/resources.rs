use iri_string::template::simple_context::SimpleContext;
use veoveo_optimization_mcp::contract::*;
use veoveo_types::{ResourceAddress, ResourceTemplateUri, ScopeName, TaskId};

const NATIVE: &str = "0195dabe-7777-7abc-8def-000000000001";

#[test]
fn every_discovery_template_expands_to_a_typed_address() {
    use OptimizationResource as R;
    let problem = ProblemId::parse(format!("problem-{NATIVE}")).unwrap();
    let run = RunId::parse(format!("run-{NATIVE}")).unwrap();
    let solution = SolutionId::parse(format!("solution-{NATIVE}")).unwrap();
    let task: TaskId = NATIVE.parse().unwrap();
    let mut cases = vec![
        (
            uris::DOC_TEMPLATE,
            R::Document(OptimizationDocument::Agents),
            "doc_id",
            "agents".into(),
        ),
        (
            uris::PROFILE_TEMPLATE,
            R::Profile(
                OptimizationProfileUri::new(SolverProfileId::new("route.v2-a_1").unwrap()).unwrap(),
            ),
            "profile_id",
            "route.v2-a_1".into(),
        ),
        (
            uris::PROBLEM_TEMPLATE,
            R::Problem(OptimizationProblemUri::new(problem.clone()).unwrap()),
            "problem_id",
            problem.to_string(),
        ),
        (
            uris::RUN_TEMPLATE,
            R::Run(OptimizationRunUri::new(run.clone()).unwrap()),
            "run_id",
            run.to_string(),
        ),
        (
            uris::RUN_INCUMBENTS_TEMPLATE,
            R::RunIncumbents(run.clone()),
            "run_id",
            run.to_string(),
        ),
        (
            uris::SOLUTION_TEMPLATE,
            R::Solution(OptimizationSolutionUri::new(solution.clone()).unwrap()),
            "solution_id",
            solution.to_string(),
        ),
        (
            uris::SOLUTION_ROUTES_TEMPLATE,
            R::SolutionRoutes(solution.clone()),
            "solution_id",
            solution.to_string(),
        ),
        (
            uris::SOLUTION_VARIABLES_TEMPLATE,
            R::SolutionVariables(solution.clone()),
            "solution_id",
            solution.to_string(),
        ),
        (
            uris::SOLUTION_VERIFICATION_TEMPLATE,
            R::SolutionVerification(solution),
            "solution_id",
            format!("solution-{NATIVE}"),
        ),
        (
            uris::ARTIFACT_TEMPLATE,
            R::Artifact(NATIVE.parse().unwrap()),
            "artifact_id",
            NATIVE.to_owned(),
        ),
        (
            OptimizationTaskUsageUri::TEMPLATE,
            R::TaskUsage(OptimizationTaskUsageUri::new(task).unwrap()),
            "task_id",
            NATIVE.to_owned(),
        ),
    ];
    for (collection, template) in [
        (
            OptimizationCollection::Problems,
            uris::PROBLEMS_PAGE_TEMPLATE,
        ),
        (OptimizationCollection::Runs, uris::RUNS_PAGE_TEMPLATE),
        (
            OptimizationCollection::Solutions,
            uris::SOLUTIONS_PAGE_TEMPLATE,
        ),
    ] {
        let cursor =
            OptimizationIndexCursor::new(collection, "2026-09-28T00:00:00Z".parse().unwrap(), task)
                .unwrap();
        cases.push((
            template,
            R::Collection(
                OptimizationCollectionUri::new(collection, Some(cursor.clone())).unwrap(),
            ),
            "cursor",
            cursor.as_str().to_owned(),
        ));
    }
    let cursor = OptimizationUsageCursor::new(task).unwrap();
    cases.push((
        OptimizationUsageIndexUri::TEMPLATE,
        R::Usage(OptimizationUsageIndexUri::new(Some(&cursor))),
        "cursor",
        cursor.as_str().to_owned(),
    ));
    for (template, address, parameter, value) in cases {
        let template = ResourceTemplateUri::new(template).unwrap();
        let mut context = SimpleContext::new();
        context.insert(parameter, value);
        assert_eq!(
            template.expand(&context).unwrap(),
            address.to_uri().unwrap()
        );
        round_trip(address);
    }
}

#[test]
fn fixed_resources_and_collection_roots_preserve_wire_spelling() {
    use OptimizationResource as R;
    for (address, wire) in [
        (R::Capabilities, uris::CAPABILITIES_URI),
        (R::Profiles, uris::PROFILES_URI),
        (R::Docs, uris::DOCS_URI),
        (R::Contract, uris::CONTRACT_URI),
        (R::RoutesApp, uris::ROUTES_APP_URI),
        (R::ModelsApp, uris::MODELS_APP_URI),
        (
            R::Document(OptimizationDocument::Design),
            "optimization://docs/design",
        ),
        (
            R::Usage(OptimizationUsageIndexUri::new(None)),
            OptimizationUsageIndexUri::ROOT,
        ),
    ] {
        assert_eq!(address.to_uri().unwrap().as_str(), wire);
        round_trip(address);
    }
    for collection in [
        OptimizationCollection::Problems,
        OptimizationCollection::Runs,
        OptimizationCollection::Solutions,
    ] {
        let address = R::Collection(OptimizationCollectionUri::new(collection, None).unwrap());
        assert_eq!(address.to_uri().unwrap().as_str(), collection.root());
        round_trip(address);
    }
}

fn round_trip(address: OptimizationResource) {
    let wire = address.to_uri().unwrap();
    assert_eq!(OptimizationResource::parse(wire.as_str()).unwrap(), address);
    assert_eq!(
        <OptimizationResource as ResourceAddress>::parse(&wire).unwrap(),
        address
    );
    assert_eq!(serde_json::to_value(&address).unwrap(), wire.as_str());
    assert_eq!(
        serde_json::from_value::<OptimizationResource>(serde_json::json!(wire.as_str())).unwrap(),
        address
    );
}

#[test]
fn malformed_and_aliased_addresses_cannot_reach_a_handler() {
    let root = format!("optimization://solution/solution-{NATIVE}");
    let mut invalid = vec![
        "optimization://profile/..".into(),
        "optimization://profile/a:b".into(),
        "optimization://profile/a%3Ab".into(),
        "optimization://profile/a%2Fb".into(),
        "optimization://profile/a%3Fb".into(),
        "optimization://profile/a%23b".into(),
        "optimization://profile/%".into(),
        "optimization://profile/%62alanced".into(),
        "optimization://docs/unknown".into(),
        "optimization://docs/design?ignored=yes".into(),
        "optimization://docs/design#fragment".into(),
        "optimization://profiles/".into(),
        "ui://optimization/routes.html?ignored=yes".into(),
        "optimization://solution/solution-1".into(),
        format!("optimization://solution/run-{NATIVE}"),
        format!("{root}/routes/extra"),
        format!("{root}/incumbents"),
        format!("{root}?cursor=x"),
        format!("{root}?cursor=x&cursor=x"),
        format!("{root}#fragment"),
        root.to_uppercase(),
        format!(
            "optimization://solution/solution-{}",
            NATIVE.replace("8def", "0def")
        ),
    ];
    invalid.push(format!("{root}/../solution-{NATIVE}"));
    for value in invalid {
        assert!(
            OptimizationResource::parse(&value).is_err(),
            "accepted {value}"
        );
        assert!(serde_json::from_value::<OptimizationResource>(serde_json::json!(value)).is_err());
    }
    let run = OptimizationRunUri::new(RunId::parse(format!("run-{NATIVE}")).unwrap()).unwrap();
    assert!(OptimizationSolutionUri::parse(run.as_str()).is_err());
    assert!(OptimizationProblemUri::parse(run.as_str()).is_err());
    assert!(OptimizationProfileUri::parse(run.as_str()).is_err());
}

#[test]
fn output_identity_admission_is_canonical_rfc_uuidv7() {
    for value in [
        NATIVE.to_uppercase(),
        NATIVE.replace('-', ""),
        NATIVE.replace("7abc", "4abc"),
        NATIVE.replace("8def", "0def"),
    ] {
        assert!(ProblemId::parse(format!("problem-{value}")).is_err());
        assert!(RunId::parse(format!("run-{value}")).is_err());
        assert!(SolutionId::parse(format!("solution-{value}")).is_err());
        assert!(VerificationId::parse(format!("verification-{value}")).is_err());
    }
    assert!(SolverProfileId::new(".").is_err());
    assert!(SolverProfileId::new("..").is_err());
    assert!(SolverProfileId::new("a:b").is_err());
    assert!(OptimizationScope::try_from(&ScopeName::new("installation:read").unwrap()).is_err());
}
