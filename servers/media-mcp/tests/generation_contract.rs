#[path = "support/generation.rs"]
mod fixture;

use serde_json::json;
use veoveo_media_mcp::contract::{
    GenerationPredictionSummary, MediaGenerationResult, MediaGenerationUri, MediaPredictionId,
};
use veoveo_types::{ResourceAddress, TaskId};
#[test]
fn generation_schemas_preserve_the_published_profile() {
    let actual = serde_json::json!({"summary":schemars::schema_for!(GenerationPredictionSummary),"output":schemars::schema_for!(MediaGenerationResult)});
    assert_eq!(
        actual,
        serde_json::from_str::<serde_json::Value>(include_str!("fixtures/generation-schemas.json"))
            .unwrap()
    );
}

#[test]
fn generation_address_round_trips_encoded_prediction_identity() {
    for value in [
        "provider-123",
        "slash/space ?#&%+",
        "imagen/árbol",
        "quote'\"",
        "%2f",
    ] {
        let id = MediaPredictionId::new(value).unwrap();
        let uri = MediaGenerationUri::new(id.clone());
        assert_eq!(MediaGenerationUri::parse(uri.as_str()).unwrap(), uri);
        assert_eq!(uri.prediction_id(), &id);
        assert_eq!(uri.prediction_uri().id(), &id);
        assert_eq!(uri.to_uri().unwrap().as_str(), uri.as_str());
        assert_eq!(
            serde_json::from_value::<MediaGenerationUri>(json!(uri)).unwrap(),
            uri
        );
    }
    for value in [
        "media://prediction/test",
        "media://prediction/test/result/extra",
        "media://prediction/test/result?cursor=x",
        "media://prediction/test/result?",
        "media://prediction/test/result#fragment",
        "media://prediction/test/%72esult",
        "media://prediction/%74est/result",
        "media://prediction/../result",
        "media://prediction/a/b/result",
        "media://predictions/test/result",
        "other://prediction/test/result",
    ] {
        assert!(
            MediaGenerationUri::parse(value).is_err(),
            "accepted {value}"
        );
    }
}

#[test]
fn canonical_result_round_trip_and_empty_output_set() {
    let task = TaskId::new();
    let result = fixture::generation(task, MediaPredictionId::new("job").unwrap());
    let wire = serde_json::to_value(&result).unwrap();
    assert_eq!(wire["schema"], MediaGenerationResult::SCHEMA);
    assert_eq!(wire["result_uri"], "media://prediction/job/result");
    assert_eq!(result.task_id(), task);
    assert_eq!(result.artifacts().len(), 2);
    assert_eq!(
        serde_json::from_value::<MediaGenerationResult>(wire).unwrap(),
        result
    );
    let mut prediction = result.prediction().clone();
    prediction.output_count = 0;
    let empty = MediaGenerationResult::new(task, prediction, vec![]).unwrap();
    assert!(empty.artifacts().is_empty());
    assert_eq!(empty.result_uri(), result.result_uri());
    let schema = serde_json::to_value(schemars::schema_for!(MediaGenerationResult)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("result_uri"))
    );
}

#[test]
fn canonical_result_rejects_inconsistent_identity_and_output_attribution() {
    let result = fixture::generation(TaskId::new(), MediaPredictionId::new("job").unwrap());
    let wire = serde_json::to_value(&result).unwrap();
    for (path, replacement) in [
        ("/schema", json!("veoveo.ai/media-generation/v2")),
        ("/result_uri", json!("media://prediction/other/result")),
        ("/task_id", json!(TaskId::new())),
        ("/task_id", json!("0195dabe-7777-4abc-8def-000000000001")),
        ("/prediction/id", json!("other")),
        ("/prediction/model_id", json!("different/model")),
        ("/prediction/status", json!("processing")),
        ("/prediction/output_count", json!(1)),
        (
            "/artifacts/0/artifact_uri",
            json!("artifact://0195dabe-8888-7abc-8def-000000000001"),
        ),
        ("/artifacts/0/metadata/task_id", json!(TaskId::new())),
        ("/artifacts/0/metadata/job_id", json!("other")),
        ("/artifacts/0/metadata/model_id", json!("different/model")),
        ("/artifacts/0/metadata/output_index", json!(1)),
    ] {
        let mut bad = wire.clone();
        *bad.pointer_mut(path).unwrap() = replacement;
        assert!(
            serde_json::from_value::<MediaGenerationResult>(bad).is_err(),
            "accepted {path}"
        );
    }
    let mut bad = wire.clone();
    bad["artifacts"][1]["artifact_id"] = bad["artifacts"][0]["artifact_id"].clone();
    bad["artifacts"][1]["artifact_uri"] = bad["artifacts"][0]["artifact_uri"].clone();
    assert!(serde_json::from_value::<MediaGenerationResult>(bad).is_err());
    let mut bad = wire.clone();
    bad["artifacts"].as_array_mut().unwrap().swap(0, 1);
    assert!(serde_json::from_value::<MediaGenerationResult>(bad).is_err());
    let mut bad = wire;
    bad["artifacts"][0]["download_url"] = json!("https://provider.test/output");
    assert!(serde_json::from_value::<MediaGenerationResult>(bad).is_err());
    let mut artifacts = result.artifacts().to_vec();
    artifacts[0].metadata["output_index"] = json!(3);
    assert!(
        MediaGenerationResult::new(result.task_id(), result.prediction().clone(), artifacts)
            .is_err()
    );
}

#[test]
fn canonical_result_rejects_unknown_profile_fields() {
    let wire = serde_json::to_value(fixture::generation(
        TaskId::new(),
        MediaPredictionId::new("job").unwrap(),
    ))
    .unwrap();
    for path in ["", "/artifacts/0/metadata"] {
        let mut bad = wire.clone();
        bad.pointer_mut(path).unwrap()["unexpected"] = json!(true);
        assert!(serde_json::from_value::<MediaGenerationResult>(bad).is_err());
    }
    let mut bad = wire;
    bad.as_object_mut().unwrap().remove("schema");
    assert!(serde_json::from_value::<MediaGenerationResult>(bad).is_err());
}

#[test]
fn generation_result_requires_the_current_complete_profile() {
    let result = fixture::generation(TaskId::new(), MediaPredictionId::new("job").unwrap());
    let current = serde_json::to_value(&result).unwrap();
    assert_eq!(
        serde_json::from_value::<MediaGenerationResult>(current.clone()).unwrap(),
        result
    );
    let unversioned = json!({"prediction": result.prediction(), "artifacts":result.artifacts()});
    assert!(serde_json::from_value::<MediaGenerationResult>(unversioned).is_err());
    for schema in ["veoveo.ai/media-generation/v2", "unexpected"] {
        let mut bad = current.clone();
        bad["schema"] = json!(schema);
        assert!(serde_json::from_value::<MediaGenerationResult>(bad).is_err());
    }
}
