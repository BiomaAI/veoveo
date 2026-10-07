use serde_json::json;
use veoveo_reason_mcp::{
    contract::*,
    grounding::{MAX_GROUNDING_DETECTIONS, extract_grounding, grounded_track_ids},
};
use veoveo_stream_mcp::contract::AnalysisResults;

fn producer() -> AnalysisResults {
    serde_json::from_str(include_str!(
        "../../../stream-mcp/testdata/replay-results-v2.json"
    ))
    .unwrap()
}

fn selection() -> RecordingVideoSelection {
    let result = producer();
    veoveo_recording_video::contract::RecordingVideoSelectionBuilder {
        recording_uri: result.recording_uri.clone(),
        entity_path: result.entity_path.clone(),
        timeline: result.timeline.clone(),
        range: IndexRange::new(10, 20).unwrap(),
    }
    .build()
    .unwrap()
}

fn source() -> StreamArtifactUri {
    StreamArtifactUri::new("01983da0-0000-7000-8000-000000000001".parse().unwrap())
}

#[test]
fn stream_producer_contract_flows_into_reason_without_runtime_dependencies() {
    let produced = producer();
    produced.validate().unwrap();
    let grounding = extract_grounding(
        &source(),
        &selection(),
        &serde_json::to_vec(&produced).unwrap(),
    )
    .unwrap();
    assert_eq!(grounding.schema, GroundingSchema::V2);
    assert_eq!(grounding.source_artifact_uri, source());
    assert_eq!(
        grounding.frames.iter().map(|f| f.index).collect::<Vec<_>>(),
        [10, 20]
    );
    assert_eq!(grounded_track_ids(&grounding), [7, 8].into_iter().collect());
    let wire = serde_json::to_value(&grounding).unwrap();
    assert_eq!(wire["schema"], "veoveo.ai/reason-grounding/v2");
    assert!(
        wire["frames"][0]["detections"][0]
            .get("confidence")
            .is_none()
    );
    let restored: GroundingDetections = serde_json::from_value(wire).unwrap();
    assert_eq!(
        grounded_track_ids(&restored),
        grounded_track_ids(&grounding)
    );
}

#[test]
fn grounding_rejects_other_recordings_entities_timelines_and_uncovered_ranges() {
    let bytes = serde_json::to_vec(&producer()).unwrap();
    for field in ["recording", "entity", "timeline", "start", "end"] {
        let mut selected = selection().into_builder();
        match field {
            "recording" => {
                selected.recording_uri =
                    "recording://recordings/01983da0-0000-7000-8000-000000000003"
                        .parse()
                        .unwrap()
            }
            "entity" => selected.entity_path = "/camera/back".into(),
            "timeline" => selected.timeline = "other_time".into(),
            "start" => selected.range = IndexRange::new(-1, selected.range.end).unwrap(),
            "end" => selected.range = IndexRange::new(selected.range.start, 31).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            extract_grounding(&source(), &selected.build().unwrap(), &bytes).is_err(),
            "{field}"
        );
    }
}

#[test]
fn grounding_cannot_accept_a_partial_or_malformed_stream_document() {
    let wire = serde_json::to_value(producer()).unwrap();
    for field in ["sourceSnapshot", "pipelineId", "requestedRange"] {
        let mut invalid = wire.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            extract_grounding(
                &source(),
                &selection(),
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err()
        );
    }
    for (path, value) in [
        ("/schema", json!("unsupported/v2")),
        (
            "/sourceSnapshot/recordingId",
            json!("01983da0-0000-7000-8000-000000000001"),
        ),
        ("/frames/1/index", json!(0)),
        ("/frames/1/detections/0/label", json!(" ")),
        ("/frames/1/detections/0/confidence", json!(1.5)),
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            extract_grounding(
                &source(),
                &selection(),
                &serde_json::to_vec(&invalid).unwrap()
            )
            .is_err(),
            "{path}"
        );
    }
}

#[test]
fn grounding_budget_is_checked_before_admitting_track_citations() {
    let mut result = producer().into_builder();
    result.frames.truncate(1);
    let detection = result.frames[0].detections[0].clone();
    result.frames[0].detections = vec![detection; MAX_GROUNDING_DETECTIONS + 1];
    let error = extract_grounding(
        &source(),
        &selection(),
        &serde_json::to_vec(&result).unwrap(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("exceeds 100000 detections"));
}

#[test]
fn request_and_retained_subset_use_the_stream_owned_artifact_address() {
    let reference: GroundingReference =
        serde_json::from_value(json!({"resultsArtifactUri": source()})).unwrap();
    assert_eq!(reference.results_artifact_uri, source());
    for uri in [
        "artifact://test",
        "https://example.com/results.json",
        "reason://artifact/01983da0-0000-7000-8000-000000000001",
    ] {
        assert!(
            serde_json::from_value::<GroundingReference>(json!({"resultsArtifactUri": uri}))
                .is_err()
        );
        assert!(serde_json::from_value::<GroundingDetections>(json!({"schema":"veoveo.ai/reason-grounding/v2", "sourceArtifactUri":uri, "frames":[]})).is_err());
    }
    assert!(
        serde_json::from_value::<GroundingDetections>(
            json!({"schema":"unknown/v2", "sourceArtifactUri":source(), "frames":[]})
        )
        .is_err()
    );
}
