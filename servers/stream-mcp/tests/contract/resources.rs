use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use veoveo_stream_mcp::{contract::*, uris};

const ID: &str = "01983da0-0000-7000-8000-000000000001";

#[test]
fn catalog_and_execution_id_profiles_are_enforced_on_decode() {
    for id in ["camera", "camera-2", "7", &"x".repeat(128)] {
        assert_eq!(PipelineId::parse(id).unwrap().as_str(), id);
        assert_eq!(ModelId::parse(id).unwrap().as_str(), id);
    }
    for id in [
        "",
        "-camera",
        "Camera",
        " camera",
        "camera/x",
        "a?b",
        "a%2Fb",
        &"x".repeat(129),
    ] {
        assert!(PipelineId::parse(id).is_err());
        assert!(serde_json::from_value::<ModelId>(serde_json::json!(id)).is_err());
    }
    assert_eq!(RunId::parse(ID).unwrap().to_string(), ID);
    assert_eq!(SessionId::parse(ID).unwrap().to_string(), ID);
    for id in [
        "run",
        "01983da0-0000-4000-8000-000000000001",
        "01983da0-0000-7000-0000-000000000001",
        "01983DA0-0000-7000-8000-000000000001",
    ] {
        assert!(RunId::parse(id).is_err());
        assert!(SessionId::parse(id).is_err());
    }
}

#[test]
fn every_resource_round_trips_through_its_owner() {
    let run = RunId::parse(ID).unwrap();
    let session = SessionId::parse(ID).unwrap();
    let date = "2026-09-28T12:00:00Z".parse().unwrap();
    let resources = [
        StreamResource::Docs,
        StreamResource::Document(StreamDocument::Design),
        StreamResource::Document(StreamDocument::Agents),
        StreamResource::Contract,
        StreamResource::LiveApp,
        StreamResource::Pipelines,
        StreamResource::Models,
        StreamResource::Pipeline(uris::pipeline_uri(&"camera".parse().unwrap())),
        StreamResource::Model(uris::model_uri(&"detector".parse().unwrap())),
        StreamResource::Runs(None),
        StreamResource::Runs(Some(RunCursor::new(date, run))),
        StreamResource::Run(uris::run_uri(run)),
        StreamResource::RunResults(uris::results_uri(run)),
        StreamResource::Sessions(None),
        StreamResource::Sessions(Some(SessionCursor::new(session))),
        StreamResource::Session(uris::session_uri(session)),
        StreamResource::SessionResults(uris::session_results_uri(session)),
        StreamResource::SessionPreview(uris::session_preview_uri(session)),
        StreamResource::Artifact(ID.parse().unwrap()),
    ];
    for resource in resources {
        let wire = serde_json::to_value(&resource).unwrap();
        assert_eq!(
            serde_json::from_value::<StreamResource>(wire).unwrap(),
            resource
        );
    }
    assert_eq!(*uris::run_uri(run).id(), run);
    assert_eq!(*uris::session_uri(session).id(), session);
    assert!(RunUri::parse(uris::results_uri(run).to_string()).is_err());
    assert!(SessionResultsUri::parse(uris::session_preview_uri(session).to_string()).is_err());
}

#[test]
fn resources_reject_aliases_and_noncanonical_components() {
    for uri in [
        "stream://pipeline/Camera",
        "stream://pipeline/camera/",
        "stream://pipeline/%63amera",
        "stream://model/a%2Fb",
        "stream://model/camera?x=1",
        "stream://models#fragment",
        "stream://docs/unknown",
        "stream://runs/",
        "stream://runs?",
        "stream://runs?offset=1",
        "stream://runs?cursor=",
        "stream://runs?cursor=one&cursor=two",
        "stream://sessions?cursor=bad",
        "ui://stream/live.html?x=1",
    ] {
        assert!(StreamResource::parse(uri).is_err(), "{uri}");
    }
}

#[test]
fn cursors_validate_version_collection_position_and_unknown_fields() {
    let run = RunId::parse(ID).unwrap();
    let session = SessionId::parse(ID).unwrap();
    let cursor = RunCursor::new("2026-09-28T12:00:00Z".parse().unwrap(), run);
    assert_eq!(RunCursor::parse(cursor.as_str()).unwrap(), cursor);
    let session_cursor = SessionCursor::new(session);
    assert_eq!(
        SessionCursor::parse(session_cursor.as_str())
            .unwrap()
            .session_id(),
        session
    );
    assert!(RunCursor::parse(session_cursor.as_str()).is_err());
    assert!(SessionCursor::parse(cursor.as_str()).is_err());
    let value: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor.as_str()).unwrap()).unwrap();
    for (field, mutation) in [
        ("version", serde_json::json!(2)),
        ("collection", serde_json::json!("stream://sessions")),
        (
            "position",
            serde_json::json!({"created_at": "invalid", "task_id": ID}),
        ),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut invalid = value.clone();
        invalid[field] = mutation;
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&invalid).unwrap());
        assert!(RunCursor::parse(encoded).is_err());
    }
    assert!(RunCursor::parse("x".repeat(1025)).is_err());
    assert!(SessionCursor::parse("x".repeat(1025)).is_err());
}

#[test]
fn declared_templates_expand_to_the_owning_builders() {
    use veoveo_types::ResourceTemplateUri;
    let run = RunId::parse(ID).unwrap();
    let session = SessionId::parse(ID).unwrap();
    let runs = RunCursor::new("2026-09-28T12:00:00Z".parse().unwrap(), run);
    let sessions = SessionCursor::new(session);
    for (template, parameter, value, expected) in [
        (
            uris::DOC_TEMPLATE,
            "doc_id",
            "design".to_owned(),
            uris::doc_uri(StreamDocument::Design).to_string(),
        ),
        (
            uris::PIPELINE_TEMPLATE,
            "pipeline_id",
            "camera".into(),
            uris::pipeline_uri(&"camera".parse().unwrap()).to_string(),
        ),
        (
            uris::MODEL_TEMPLATE,
            "model_id",
            "detector".into(),
            uris::model_uri(&"detector".parse().unwrap()).to_string(),
        ),
        (
            uris::RUN_TEMPLATE,
            "run_id",
            ID.into(),
            uris::run_uri(run).to_string(),
        ),
        (
            uris::RUN_RESULTS_TEMPLATE,
            "run_id",
            ID.into(),
            uris::results_uri(run).to_string(),
        ),
        (
            uris::SESSION_TEMPLATE,
            "session_id",
            ID.into(),
            uris::session_uri(session).to_string(),
        ),
        (
            uris::SESSION_RESULTS_TEMPLATE,
            "session_id",
            ID.into(),
            uris::session_results_uri(session).to_string(),
        ),
        (
            uris::SESSION_PREVIEW_TEMPLATE,
            "session_id",
            ID.into(),
            uris::session_preview_uri(session).to_string(),
        ),
        (
            uris::ARTIFACT_TEMPLATE,
            "artifact_id",
            ID.into(),
            uris::artifact_uri(ID.parse().unwrap()).to_string(),
        ),
        (
            uris::RUNS_PAGE_TEMPLATE,
            "cursor",
            runs.as_str().into(),
            uris::runs_uri(Some(runs.clone())).to_string(),
        ),
        (
            uris::SESSIONS_PAGE_TEMPLATE,
            "cursor",
            sessions.as_str().into(),
            uris::sessions_uri(Some(sessions.clone())).to_string(),
        ),
    ] {
        let expanded = ResourceTemplateUri::new(template)
            .unwrap()
            .expand_scalars(&[(parameter.to_owned(), value)].into())
            .unwrap();
        assert_eq!(expanded.as_str(), expected);
        StreamResource::parse(expanded.as_str()).unwrap();
    }
    for (template, root) in [
        (uris::RUNS_PAGE_TEMPLATE, uris::RUNS_URI),
        (uris::SESSIONS_PAGE_TEMPLATE, uris::SESSIONS_URI),
    ] {
        assert_eq!(
            ResourceTemplateUri::new(template)
                .unwrap()
                .expand_scalars(&Default::default())
                .unwrap()
                .as_str(),
            root
        );
    }
}

#[test]
fn stream_declares_no_additional_domain_scopes() {
    use veoveo_types::ScopeName;
    for name in ["operator:use", "stream:read", "future:server:read"] {
        assert!(StreamScope::try_from(&ScopeName::new(name).unwrap()).is_err());
    }
}

#[test]
fn only_run_resources_implement_the_shared_task_address_contract() {
    use veoveo_types::{ResourceAddress, TaskResourceAddress};
    let id: RunId = ID.parse().unwrap();
    for resource in [RunResource::run(id), RunResource::results(id)] {
        assert_eq!(resource.task_id(), id.task_id());
        let uri = resource.to_uri().unwrap();
        assert_eq!(RunResource::parse(uri.as_str()).unwrap(), resource);
        let wire = serde_json::to_value(&resource).unwrap();
        assert_eq!(wire, uri.as_str());
        assert_eq!(
            serde_json::from_value::<RunResource>(wire).unwrap(),
            resource
        );
    }
    for uri in [
        "stream://runs".to_owned(),
        "stream://pipelines".to_owned(),
        uris::session_uri(ID.parse().unwrap()).to_string(),
        uris::session_results_uri(ID.parse().unwrap()).to_string(),
        uris::session_preview_uri(ID.parse().unwrap()).to_string(),
        uris::artifact_uri(ID.parse().unwrap()).to_string(),
    ] {
        assert!(RunResource::parse(&uri).is_err(), "{uri}");
    }
}

#[test]
fn nominal_address_route_selection_precedes_sibling_identifier_admission() {
    assert!(matches!(
        ModelUri::parse("stream://pipeline/INVALID"),
        Err(StreamContractError::InvalidResource)
    ));
    assert!(matches!(
        ModelUri::parse("stream://model/INVALID"),
        Err(StreamContractError::InvalidId(_))
    ));
}
