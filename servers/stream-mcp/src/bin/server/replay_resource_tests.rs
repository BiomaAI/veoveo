//! Admission by the actual selected replay resource receiver, without an Artifact server.
use super::*;
use veoveo_stream_mcp::contract::*;

fn replay_fixture() -> (
    RunView,
    RecordingVideoSelection,
    veoveo_artifact_contract::ArtifactMetadata,
    Vec<u8>,
) {
    let results: AnalysisResults =
        serde_json::from_str(include_str!("../../../testdata/replay-results-v2.json")).unwrap();
    let selected = RecordingVideoSelection::new(
        results.recording_uri.clone(),
        results.entity_path.clone(),
        results.timeline.clone(),
        results.requested_range,
    )
    .unwrap();
    let id: RunId = "01983da0-0000-7000-8000-000000000001".parse().unwrap();
    let mut output = serde_json::from_str::<RunRecordingOutput>(include_str!(
        "../../../testdata/run-output.json"
    ))
    .unwrap()
    .into_builder();
    output.summary = AnalysisSummary {
        processed_frames: results.processed_frames,
        detection_count: results
            .frames
            .iter()
            .map(|frame| frame.detections.len() as u64)
            .sum(),
        elapsed_ms: results.elapsed_ms,
        decode_start_index: 0,
        requested_start_index: results.requested_range.start,
        requested_end_index: results.requested_range.end,
    };
    let digest = results.source_snapshot.digest_sha256().unwrap();
    output.results_artifact.metadata = serde_json::to_value(StreamArtifactMetadata {
        provenance: StreamArtifactProvenance::Results {
            run_id: id,
            recording_id: selected.recording_uri.id(),
            pipeline_id: results.pipeline_id.clone(),
            model_id: results.model_id.clone(),
            source_snapshot_sha256: digest.clone(),
        },
    })
    .unwrap();
    output.annotations_artifact.metadata = serde_json::to_value(StreamArtifactMetadata {
        provenance: StreamArtifactProvenance::AnnotationLayer {
            run_id: id,
            recording_id: selected.recording_uri.id(),
            results_artifact_uri: output.results_artifact.artifact_uri.clone(),
            source_snapshot_sha256: digest,
        },
    })
    .unwrap();
    let artifact = output.results_artifact.clone();
    let run = RunView::new(
        id,
        results.pipeline_id.clone(),
        RunDetails {
            status: veoveo_task_contract::TaskStatus::Succeeded,
            progress: 1.0,
            recording_uri: selected.recording_uri.clone(),
            entity_path: selected.entity_path.clone(),
            timeline: selected.timeline.clone(),
            created_at: "2026-09-28T00:00:00Z".into(),
            updated_at: "2026-09-28T00:01:00Z".into(),
        },
    )
    .with_output(Some(output.build().unwrap()))
    .unwrap();
    (
        run,
        selected,
        artifact,
        serde_json::to_vec(&results).unwrap(),
    )
}

#[test]
fn actual_replay_receiver_requires_current_body_and_selected_snapshot_agreement() {
    let (run, selected, artifact, bytes) = replay_fixture();
    let accepted = decode_replay(&run, &selected, &artifact, &bytes).unwrap();
    assert_eq!(serde_json::to_vec(&accepted).unwrap(), bytes);
    let current: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (pointer, replacement) in [
        ("/schema", serde_json::json!("veoveo.stream-results/v1")),
        ("/pipelineId", serde_json::json!("another")),
        ("/modelId", serde_json::json!("another")),
        (
            "/sourceSnapshot/capturedAt",
            serde_json::json!("2026-09-28T01:00:00Z"),
        ),
        ("/elapsedMs", serde_json::json!(6)),
        ("/entityPath", serde_json::json!("/another")),
        ("/requestedRange/end", serde_json::json!(31)),
    ] {
        let mut wire = current.clone();
        *wire.pointer_mut(pointer).unwrap() = replacement;
        let error = decode_replay(
            &run,
            &selected,
            &artifact,
            &serde_json::to_vec(&wire).unwrap(),
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "stored Stream replay does not match its selected run and Artifact"
        );
    }
    for mode in 0..3 {
        let mut wire = current.clone();
        let object = wire.as_object_mut().unwrap();
        let value = if mode == 2 {
            serde_json::json!("retired-conflict")
        } else {
            object["pipelineId"].clone()
        };
        if mode == 0 {
            object.remove("pipelineId");
        }
        object.insert("pipeline_id".into(), value);
        assert!(
            decode_replay(
                &run,
                &selected,
                &artifact,
                &serde_json::to_vec(&wire).unwrap()
            )
            .is_err()
        );
    }
    let mut changed = artifact.clone();
    changed.artifact_uri = "stream://artifact/01983da0-0000-7000-8000-000000000004"
        .parse()
        .unwrap();
    assert!(decode_replay(&run, &selected, &changed, &bytes).is_err());
    let mut changed = artifact.clone();
    changed.metadata["provenance"]["run_id"] = serde_json::json!("retired-conflict");
    assert!(decode_replay(&run, &selected, &changed, &bytes).is_err());
    let mut changed = selected.into_builder();
    changed.range = IndexRange::new(0, 31).unwrap();
    assert!(decode_replay(&run, &changed.build().unwrap(), &artifact, &bytes).is_err());
}
