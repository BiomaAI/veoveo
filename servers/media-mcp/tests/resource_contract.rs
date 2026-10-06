use serde_json::json;
use veoveo_artifact_contract::ArtifactId;
use veoveo_media_mcp::contract::*;
use veoveo_types::{ResourceAddress, ScopeName, TaskId};

#[test]
fn complete_resource_vocabulary_round_trips() {
    let task = TaskId::new();
    let prediction = MediaPredictionId::new("provider/part ?#&=%+ 雨").unwrap();
    let usage_cursor = MediaUsageCursor::new(task).unwrap();
    let prediction_cursor = MediaPredictionCursor::new(prediction.clone()).unwrap();
    for resource in [
        MediaResource::Models(veoveo_media_mcp::contract::MediaModelIndexUri::new(
            None, None, None, None,
        )),
        MediaResource::Model(MediaModelUri::new(
            "openai/gpt-image-2/edit".parse().unwrap(),
        )),
        MediaResource::Predictions(MediaPredictionIndexUri::new(None)),
        MediaResource::Predictions(MediaPredictionIndexUri::new(Some(&prediction_cursor))),
        MediaResource::Prediction(MediaPredictionUri::new(prediction.clone())),
        MediaResource::Generation(MediaGenerationUri::new(prediction)),
        MediaResource::Docs,
        MediaResource::Document(MediaDocument::Agents),
        MediaResource::Document(MediaDocument::Design),
        MediaResource::Contract,
        MediaResource::StudioApp,
        MediaResource::Usage(MediaUsageIndexUri::new(None)),
        MediaResource::Usage(MediaUsageIndexUri::new(Some(&usage_cursor))),
        MediaResource::TaskUsage(MediaTaskUsageUri::new(task).unwrap()),
        MediaResource::Artifact(MediaArtifactUri::new(ArtifactId::new())),
    ] {
        let uri = resource.to_uri().unwrap();
        assert_eq!(MediaResource::parse(uri.as_str()).unwrap(), resource);
        assert_eq!(
            <MediaResource as ResourceAddress>::parse(&uri).unwrap(),
            resource
        );
        assert_eq!(
            serde_json::from_value::<MediaResource>(json!(uri.as_str())).unwrap(),
            resource
        );
        assert_eq!(serde_json::to_value(resource).unwrap(), json!(uri.as_str()));
    }
}

#[test]
fn route_admission_rejects_wrong_parents_aliases_and_unsupported_components() {
    let artifact = ArtifactId::new();
    for uri in [
        "media://docs/missing".to_owned(),
        "media://docs/design/".to_owned(),
        "media://docs/%64esign".to_owned(),
        "media://models/extra".to_owned(),
        "media://models?x=1".to_owned(),
        "media://models?".to_owned(),
        "media://contract#fragment".to_owned(),
        "media://prediction/job/result/extra".to_owned(),
        "media://prediction/job/result?x=1".to_owned(),
        "ui://media/studio.html?x=1".to_owned(),
        "ui://other/studio.html".to_owned(),
        format!("timeseries://artifact/{artifact}"),
        format!("media://artifact/{artifact}/extra"),
        format!("media://artifact/{artifact}?x=1"),
    ] {
        assert!(MediaResource::parse(&uri).is_err(), "{uri}");
        assert!(serde_json::from_value::<MediaResource>(json!(uri)).is_err());
    }
    for scope in ["media:read", "media:admin", "map:feature:read"] {
        assert!(MediaScope::try_from(&ScopeName::parse(scope).unwrap()).is_err());
    }
}

#[test]
fn model_identity_owns_segmented_routes_and_request_admission() {
    for model in [
        "model",
        "openai/gpt-image-2/edit",
        "provider/model.v2_test",
        &"x".repeat(512),
    ] {
        let id: MediaModelId = model.parse().unwrap();
        let uri = MediaModelUri::new(id.clone());
        assert_eq!(MediaModelUri::parse(uri.as_str()).unwrap().model_id(), &id);
        let args: RunArgs =
            serde_json::from_value(json!({"model": model, "input": {"prompt":"image"}})).unwrap();
        assert_eq!(args.model, id);
        assert_eq!(serde_json::to_value(args).unwrap()["model"], model);
        let request = ModelSchemaArgs { model: id };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({"model": model})
        );
    }
    for model in [
        "",
        ".",
        "..",
        "a/../b",
        "a/./b",
        "a//b",
        "a/",
        "/a",
        "a?x=1",
        "a#b",
        "a%b",
        "a b",
        "a\\b",
        "a\nb",
        &"x".repeat(513),
    ] {
        assert!(model.parse::<MediaModelId>().is_err(), "{model}");
        assert!(serde_json::from_value::<RunArgs>(json!({"model": model, "input": {}})).is_err());
    }
    for uri in [
        "media://model",
        "media://model/",
        "media://model/a//b",
        "media://model/%61",
        "media://model/a%2Fb",
        "media://model/a/../b",
        "media://model/a?x=1",
        "media://model/a#b",
        "other://model/a",
    ] {
        assert!(MediaModelUri::parse(uri).is_err(), "{uri}");
    }
}

#[test]
fn artifact_tool_admits_only_the_media_occurrence_address() {
    let id = ArtifactId::new();
    let uri = MediaArtifactUri::new(id);
    let args: ArtifactArgs = serde_json::from_value(json!({"artifact_uri": uri})).unwrap();
    assert_eq!(args.artifact_uri.artifact_id(), id);
    assert_eq!(
        serde_json::to_value(args).unwrap(),
        json!({"artifact_uri": uri})
    );
    for uri in [
        id.plane_uri().to_string(),
        format!("speech://artifact/{id}"),
        "media://artifact/bad".to_owned(),
    ] {
        assert!(serde_json::from_value::<ArtifactArgs>(json!({"artifact_uri": uri})).is_err());
    }
}

#[test]
fn tool_envelopes_are_closed_while_model_inputs_stay_provider_owned() {
    let input = json!({"model":"openai/gpt-image-2/edit","input":{"prompt":"scene","provider_option":{"future":true}}});
    let request: RunArgs = serde_json::from_value(input.clone()).unwrap();
    assert_eq!(request.input["provider_option"]["future"], true);
    let mut extra = input;
    extra["undeclared"] = json!(true);
    assert!(
        serde_json::from_value::<RunArgs>(extra)
            .unwrap_err()
            .to_string()
            .contains("undeclared")
    );
    let schema = serde_json::to_value(schemars::schema_for!(RunArgs)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_ne!(schema["properties"]["input"]["additionalProperties"], false);
    let query = json!({"query":"image","type":"text-to-image","limit":10});
    assert!(serde_json::from_value::<ModelsArgs>(query.clone()).is_ok());
    let mut extra = query;
    extra["undeclared"] = json!(true);
    assert!(serde_json::from_value::<ModelsArgs>(extra).is_err());
    assert!(
        serde_json::from_value::<ModelSchemaArgs>(
            json!({"model":"openai/gpt-image-2/edit","undeclared":true})
        )
        .is_err()
    );
}
