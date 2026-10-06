use veoveo_stream_mcp::contract::*;

fn results() -> AnalysisResults {
    serde_json::from_str(include_str!("../../testdata/replay-results-v1.json")).unwrap()
}

#[test]
fn replay_profile_and_intrinsic_checks_are_owned_by_the_contract() {
    let value = results();
    assert_eq!(value.schema, StreamResultsSchema::V1);
    value.validate().unwrap();
    let mut wire = serde_json::to_value(value).unwrap();
    wire["schema"] = "unsupported/v2".into();
    assert!(serde_json::from_value::<AnalysisResults>(wire).is_err());
    let mut invalid = serde_json::to_value(results()).unwrap();
    invalid["recording_uri"] = "recording://recordings/private?token=secret".into();
    assert!(serde_json::from_value::<AnalysisResults>(invalid).is_err());
    for field in [
        "source_snapshot",
        "recording_uri",
        "timeline_kind",
        "processed_frames",
    ] {
        let mut wire = serde_json::to_value(results()).unwrap();
        wire.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<AnalysisResults>(wire).is_err(),
            "{field}"
        );
    }
}

#[test]
fn replay_validation_rejects_inconsistent_frames_and_detection_values() {
    let mut invalid = results().into_builder();
    invalid.recording_uri = "recording://recordings/01983da0-0000-7000-8000-000000000001"
        .parse()
        .unwrap();
    assert_eq!(invalid.validate(), Err(StreamResultsError::SourceRecording));
    let mut invalid = results().into_builder();
    invalid.processed_frames = 0;
    assert_eq!(invalid.validate(), Err(StreamResultsError::FrameCount));
    let mut invalid = results().into_builder();
    invalid.frames[1].index = invalid.frames[0].index;
    assert_eq!(invalid.validate(), Err(StreamResultsError::FrameOrder));
    let mut invalid = results().into_builder();
    invalid.frames[0].index = -1;
    assert_eq!(invalid.validate(), Err(StreamResultsError::FrameRange));
    let invalid = results().into_builder();
    assert!(IndexRange::new(invalid.requested_range.start, -1).is_err());
    let detection = results().frames[0].detections[0].clone().into_builder();
    for (confidence, expected) in [
        (Some(f32::NAN), false),
        (Some(1.1), false),
        (Some(0.0), true),
        (None, true),
    ] {
        let mut candidate = detection.clone();
        candidate.confidence = confidence;
        assert_eq!(candidate.validate().is_ok(), expected);
        candidate.confidence = None;
        candidate.tracker_confidence = confidence;
        assert_eq!(candidate.validate().is_ok(), expected);
    }
    let mut invalid = detection.clone();
    invalid.label = " ".into();
    assert_eq!(invalid.validate(), Err(StreamResultsError::Label));
    invalid.label = "x".repeat(257);
    assert_eq!(invalid.validate(), Err(StreamResultsError::Label));
    let mut invalid = detection.clone();
    invalid.class_id = u32::from(u16::MAX) + 1;
    assert_eq!(invalid.validate(), Err(StreamResultsError::ClassId));
    for width in [0.0, -1.0, f32::INFINITY] {
        let mut invalid = detection.clone();
        invalid.bounds.width = width;
        assert_eq!(invalid.validate(), Err(StreamResultsError::Bounds));
    }
}

#[test]
fn stream_artifact_addresses_use_shared_occurrences_and_reject_other_presentations() {
    let id = "01983da0-0000-7000-8000-000000000001";
    let uri = StreamArtifactUri::new(id.parse().unwrap());
    assert_eq!(uri.to_string(), format!("stream://artifact/{id}"));
    assert_eq!(uri.artifact_id().to_string(), id);
    assert_eq!(uri.to_uri().as_str(), uri.as_str());
    let wire = serde_json::to_value(&uri).unwrap();
    assert_eq!(
        serde_json::from_value::<StreamArtifactUri>(wire).unwrap(),
        uri
    );
    // The shared Artifact profile admits and preserves this UUID spelling.
    let upper = "stream://artifact/01983DA0-0000-7000-8000-000000000001";
    assert_eq!(StreamArtifactUri::parse(upper).unwrap().as_str(), upper);
    for value in [
        format!("artifact://{id}"),
        format!("reason://artifact/{id}"),
        format!("stream://artifact/{id}?token=private"),
        format!("stream://artifact/{id}#fragment"),
        format!("stream://artifact/{id}/child"),
        format!("stream://artifact/%30{id}"),
        "stream://artifact/01983da0-0000-4000-8000-000000000001".into(),
    ] {
        let error = StreamArtifactUri::parse(&value).unwrap_err();
        assert!(!error.to_string().contains(&value));
    }
}

#[test]
fn ordinary_replay_decoding_rejects_portable_relationship_failures() {
    let wire = serde_json::to_value(results()).unwrap();
    for (pointer, value) in [
        (
            "/recording_uri",
            serde_json::json!("recording://recordings/01983da0-0000-7000-8000-000000000001"),
        ),
        ("/entity_path", serde_json::json!("relative")),
        ("/processed_frames", serde_json::json!(0)),
        ("/frames/1/index", serde_json::json!(0)),
        ("/frames/0/index", serde_json::json!(-1)),
        ("/frames/0/detections/0/confidence", serde_json::json!(1.1)),
        ("/frames/0/detections/0/bounds/width", serde_json::json!(0)),
    ] {
        let mut bad = wire.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<AnalysisResults>(bad).is_err(),
            "{pointer}"
        );
    }
    let mut sparse = results().into_builder();
    sparse.frames.clear();
    sparse.processed_frames = 0;
    sparse.requested_range = IndexRange::new(-10, 10).unwrap();
    let admitted = sparse.build().unwrap();
    assert!(
        serde_json::from_value::<AnalysisResults>(serde_json::to_value(admitted).unwrap()).is_ok()
    );
}
