use veoveo_reason_mcp::{contract::*, uris};
use veoveo_types::{ResourceAddress, ResourceTemplateUri, TaskResourceAddress};

const ID: &str = "01983da0-0000-7000-8000-000000000001";

#[test]
fn task_resource_contract_owns_only_analysis_and_result_relationships() {
    let id = AnalysisId::parse(ID).unwrap();
    for resource in [
        AnalysisResource::analysis(id),
        AnalysisResource::results(id),
    ] {
        assert_eq!(resource.task_id(), id.task_id());
        let uri = resource.to_uri().unwrap();
        assert_eq!(AnalysisResource::parse(&uri).unwrap(), resource);
        assert_eq!(
            serde_json::from_value::<AnalysisResource>(uri.as_str().into()).unwrap(),
            resource
        );
    }
    for uri in [
        uris::ANALYSES_URI,
        uris::MODELS_URI,
        "reason://pipeline/a",
        "reason://analysis/task-1",
    ] {
        assert!(AnalysisResource::parse(uri).is_err());
    }
}

#[test]
fn domain_identifiers_preserve_the_catalog_and_native_task_profiles() {
    for id in ["a", "0", "world-model", "a--", &"a".repeat(128)] {
        let pipeline = PipelineId::parse(id).unwrap();
        assert_eq!(serde_json::to_value(pipeline).unwrap(), id);
        assert_eq!(ModelId::parse(id).unwrap().to_string(), id);
    }
    for id in [
        "",
        "-a",
        "A",
        ".",
        "..",
        "a/b",
        "a?b",
        "a%2fb",
        "café",
        &"a".repeat(129),
    ] {
        assert!(PipelineId::parse(id).is_err(), "{id}");
        assert!(serde_json::from_value::<ModelId>(id.into()).is_err());
    }
    assert_eq!(AnalysisId::parse(ID).unwrap().to_string(), ID);
    for id in [
        "task-1",
        "01983da0-0000-4000-8000-000000000001",
        "01983da0-0000-7000-0000-000000000001",
        "01983DA0-0000-7000-8000-000000000001",
        "01983da000007000800000000000000001",
    ] {
        assert!(AnalysisId::parse(id).is_err(), "{id}");
    }
}

#[test]
fn every_domain_route_round_trips_and_templates_match_builders() {
    let id = AnalysisId::parse(ID).unwrap();
    let pipeline = PipelineId::parse("traffic-events").unwrap();
    let model = ModelId::parse("world-model").unwrap();
    let cursor = AnalysisCursor::new("2026-09-28T00:00:00Z".parse().unwrap(), id);
    let routes = [
        ReasonResource::Docs,
        ReasonResource::Contract,
        ReasonResource::AnalysesApp,
        ReasonResource::Document(ReasonDocument::Agents),
        ReasonResource::Document(ReasonDocument::Design),
        ReasonResource::Pipelines,
        ReasonResource::Models,
        ReasonResource::Analyses(None),
        ReasonResource::Pipeline(uris::pipeline_uri(&pipeline)),
        ReasonResource::Model(uris::model_uri(&model)),
        ReasonResource::Analysis(uris::analysis_uri(id)),
        ReasonResource::Results(uris::results_uri(id)),
        ReasonResource::Analyses(Some(cursor.clone())),
        ReasonResource::Artifact(ID.parse().unwrap()),
    ];
    for route in routes {
        let uri = route.to_uri().unwrap();
        assert_eq!(ReasonResource::parse(uri.as_str()).unwrap(), route);
        let encoded = serde_json::to_value(&route).unwrap();
        assert_eq!(encoded, uri.as_str());
        assert_eq!(
            serde_json::from_value::<ReasonResource>(encoded).unwrap(),
            route
        );
    }
    for (template, key, value, expected) in [
        (
            uris::DOC_TEMPLATE,
            "doc_id",
            "agents",
            uris::doc_uri(ReasonDocument::Agents).to_string(),
        ),
        (
            uris::PIPELINE_TEMPLATE,
            "pipeline_id",
            "traffic-events",
            uris::pipeline_uri(&pipeline).to_string(),
        ),
        (
            uris::MODEL_TEMPLATE,
            "model_id",
            "world-model",
            uris::model_uri(&model).to_string(),
        ),
        (
            uris::ANALYSIS_TEMPLATE,
            "analysis_id",
            ID,
            uris::analysis_uri(id).to_string(),
        ),
        (
            uris::RESULTS_TEMPLATE,
            "analysis_id",
            ID,
            uris::results_uri(id).to_string(),
        ),
        (
            uris::ARTIFACT_TEMPLATE,
            "artifact_id",
            ID,
            uris::artifact_uri(ID.parse().unwrap()).to_string(),
        ),
        (
            uris::ANALYSES_PAGE_TEMPLATE,
            "cursor",
            cursor.as_str(),
            uris::analyses_uri(Some(cursor.clone())).to_string(),
        ),
    ] {
        let parameters = [(key.to_string(), value.to_string())].into();
        let expanded = ResourceTemplateUri::new(template)
            .unwrap()
            .expand_scalars(&parameters)
            .unwrap();
        assert_eq!(expanded.as_str(), expected);
        ReasonResource::parse(expanded.as_str()).unwrap();
    }
    let empty = ResourceTemplateUri::new(uris::ANALYSES_PAGE_TEMPLATE)
        .unwrap()
        .expand_scalars(&Default::default())
        .unwrap();
    assert_eq!(empty.as_str(), uris::ANALYSES_URI);
}

#[test]
fn routes_reject_aliases_wrong_parents_and_unsupported_parameters() {
    for uri in [
        "reason://pipeline/a/b",
        "reason://pipeline/a?extra=1",
        "reason://pipeline/%61",
        "reason://pipeline/../model/a",
        "reason://pipeline/a#fragment",
        "reason://pipeline/a/",
        "reason://pipeline/A",
        "reason://pipeline/a%2Fb",
        "reason://model/a?cursor=bad",
        "reason://docs/unknown",
        "reason://docs/agents?extra=1",
        "reason://docs/agents/more",
        "reason://analyses?",
        "reason://analyses?cursor=",
        "reason://analyses?offset=1",
        "reason://analyses?cursor=one&cursor=two",
        "reason://analyses?cursor=bad",
        "ui://reason/other.html",
        "ui://reason/analyses.html?x=1",
        "reason://analysis/task-1",
        "reason://analysis/01983da0-0000-4000-8000-000000000001",
        "reason://analysis/01983da0-0000-7000-8000-000000000001/wrong-child",
    ] {
        let error = ReasonResource::parse(uri).unwrap_err();
        assert!(!error.to_string().contains(uri));
    }
    assert!(ModelUri::parse("reason://pipeline/a").is_err());
    assert!(PipelineUri::parse("reason://model/a").is_err());
    assert!(
        AnalysisUri::parse(uris::results_uri(AnalysisId::parse(ID).unwrap()).to_string()).is_err()
    );
}

#[test]
fn analyses_cursor_preserves_version_one_bytes_and_exact_collection_identity() {
    let expected = include_str!("../../testdata/analyses-cursor.txt").trim();
    let id = AnalysisId::parse(ID).unwrap();
    let cursor = AnalysisCursor::new("2026-09-28T00:00:00Z".parse().unwrap(), id);
    assert_eq!(cursor.as_str(), expected);
    assert_eq!(AnalysisCursor::parse(expected).unwrap(), cursor);
    assert_eq!(serde_json::to_value(&cursor).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<AnalysisCursor>(expected.into()).unwrap(),
        cursor
    );
    for invalid in ["", "a", "%%%%", &"a".repeat(1025)] {
        assert!(AnalysisCursor::parse(invalid).is_err());
    }
    // These are published v1 bodies for another collection and an unknown v2 body.
    for body in [
        include_str!("../../testdata/analyses-cursor-wrong-collection.txt"),
        include_str!("../../testdata/analyses-cursor-wrong-version.txt"),
    ] {
        assert!(AnalysisCursor::parse(body.trim()).is_err());
    }
    assert_eq!(
        ReasonResource::parse(uris::analyses_uri(Some(cursor)).as_str())
            .unwrap()
            .subscription_analysis(),
        None
    );
    assert_eq!(
        ReasonResource::Analysis(uris::analysis_uri(id)).subscription_analysis(),
        Some(id)
    );
    assert_eq!(
        ReasonResource::Results(uris::results_uri(id)).subscription_analysis(),
        Some(id)
    );
}
