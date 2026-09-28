use serde_json::json;
use veoveo_reason_mcp::contract::*;

fn legacy() -> serde_json::Value {
    serde_json::from_str(include_str!("../../testdata/analysis-output-v0.json")).unwrap()
}

#[test]
fn explicit_retained_decoder_admits_both_profiles_into_one_canonical_result() {
    let v0 = legacy();
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(v0.clone()).is_err());
    let retained = RetainedAnalysisOutput::decode(v0.clone()).unwrap();
    assert_eq!(retained.profile(), AnalysisOutputProfile::UnversionedV0);
    let v1 = serde_json::to_value(retained.output()).unwrap();
    assert_eq!(v1["schema"], "veoveo.ai/reason-analysis/v1");
    assert_eq!(v1["result_uri"], v0["results_uri"]);
    assert!(v1.get("results_uri").is_none());
    let decoded = RetainedAnalysisOutput::decode(v1.clone()).unwrap();
    assert_eq!(decoded.profile(), AnalysisOutputProfile::V1);
    assert_eq!(serde_json::to_value(decoded.into_output()).unwrap(), v1);
}

#[test]
fn unknown_or_malformed_declared_profiles_cannot_fall_back_to_legacy() {
    for schema in [
        json!("veoveo.ai/reason-analysis/v1"),
        json!("future-v2"),
        json!(null),
    ] {
        let mut value = legacy();
        value["schema"] = schema;
        assert!(RetainedAnalysisOutput::decode(value).is_err());
    }
    let v1 = serde_json::to_value(
        RetainedAnalysisOutput::decode(legacy())
            .unwrap()
            .into_output(),
    )
    .unwrap();
    for field in [
        "schema",
        "result_uri",
        "analysis_uri",
        "pipeline_uri",
        "summary",
    ] {
        let mut value = v1.clone();
        value.as_object_mut().unwrap().remove(field);
        assert!(RetainedAnalysisOutput::decode(value).is_err(), "{field}");
    }
    let mut conflicting = v1;
    conflicting["results_uri"] = legacy()["results_uri"].clone();
    assert!(RetainedAnalysisOutput::decode(conflicting).is_err());
}

#[test]
fn legacy_admission_preserves_identity_and_artifact_checks() {
    for (path, replacement) in [
        (
            "/results_uri",
            json!("reason://analysis/01983da0-0000-7000-8000-000000000002/results"),
        ),
        (
            "/results_artifact/artifact_id",
            json!("01983da0-0000-7000-8000-000000000002"),
        ),
        ("/analysis_uri", json!("reason://analysis/not-a-task")),
    ] {
        let mut value = legacy();
        *value.pointer_mut(path).unwrap() = replacement;
        assert!(RetainedAnalysisOutput::decode(value).is_err(), "{path}");
    }
}
