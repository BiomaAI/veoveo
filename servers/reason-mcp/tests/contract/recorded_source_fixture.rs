//! Current typed receiving product for the shared recorded-video source identity.
use veoveo_artifact_contract::{ArtifactMetadata, ComplianceMetadata};
use veoveo_reason_mcp::contract::*;

pub(super) fn results() -> ReasoningResults {
    let snapshot: RecordingSourceSnapshot = serde_json::from_str(include_str!(
        "../../../../platform/recordings/video/testdata/source-snapshot.json"
    ))
    .unwrap();
    let range = IndexRange::new(10, 20).unwrap();
    ReasoningResultsBuilder {
        schema: REASONING_RESULTS_SCHEMA.into(),
        pipeline_id: "traffic-events".parse().unwrap(),
        model_id: "world-model".parse().unwrap(),
        recording_uri: veoveo_recording_mcp::contract::RecordingUri::new(snapshot.recording_id),
        entity_path: "/camera/front".into(),
        timeline: "sensor_time".into(),
        timeline_kind: VideoTimelineKind::DurationNanoseconds,
        requested_range: range,
        source_snapshot: snapshot,
        task: ReasoningTask::DetectEvents {
            prompt: "Vehicles entering the intersection".into(),
        },
        answer: ReasoningAnswer::Events {
            events: vec![ReasonedEvent {
                range,
                label: "entry".into(),
                description: "Vehicle enters.".into(),
                track_ids: vec![7],
            }],
        },
        observed_frames: 10,
        elapsed_ms: 10,
        prompt_revision: "traffic-v1".into(),
        model_digest: Some(format!("sha256:{}", "a".repeat(64))),
        decode: DecodePolicy::Greedy,
        confidence_basis: ConfidenceBasis::ModelReported,
    }
    .build()
    .unwrap()
}

pub(super) fn output() -> AnalyzeRecordingOutput {
    let analysis: AnalysisId = "01983da0-0000-7000-8000-000000000001".parse().unwrap();
    let results = results();
    let digest = results.source_snapshot.digest_sha256().unwrap();
    let artifact = |id: &str, mime: &str, provenance| ArtifactMetadata {
        byte_len: 12,
        mime_type: Some(mime.into()),
        filename: None,
        artifact_uri: veoveo_artifact_contract::ArtifactUri::presented(
            &veoveo_types::ResourceScheme::parse("reason").unwrap(),
            id.parse().unwrap(),
        ),
        download_url: None,
        created_at: "2026-09-28T00:00:00Z".parse().unwrap(),
        release_state: Default::default(),
        compliance: ComplianceMetadata::default(),
        metadata: serde_json::to_value(ReasonArtifactMetadata { provenance }).unwrap(),
    };
    let result = artifact(
        "01983da0-0000-7000-8000-000000000001",
        "application/vnd.veoveo.reason-results+json",
        ReasonArtifactProvenance::Results {
            analysis_id: analysis,
            recording_id: results.recording_uri.id(),
            pipeline_id: results.pipeline_id.clone(),
            model_id: results.model_id.clone(),
            prompt_revision: results.prompt_revision.clone(),
            task_kind: ReasoningKind::DetectEvents,
            source_snapshot_sha256: digest.clone(),
        },
    );
    let annotations = artifact(
        "01983da0-0000-7000-8000-000000000003",
        "application/vnd.rerun.rrd",
        ReasonArtifactProvenance::AnnotationLayer {
            analysis_id: analysis,
            recording_id: results.recording_uri.id(),
            results_artifact_uri: result.artifact_uri.clone(),
            source_snapshot_sha256: digest.clone(),
        },
    );
    let output = AnalyzeRecordingOutput::new(
        analysis,
        FindingData::from_results(&results).unwrap(),
        ReasoningSummary {
            observed_frames: 10,
            event_count: 1,
            elapsed_ms: 10,
            decode_start_index: 0,
            requested_start_index: 10,
            requested_end_index: 20,
        },
        result,
        annotations,
    )
    .unwrap();
    assert_eq!(output.finding.source_snapshot_sha256(), &digest);
    output
}

#[test]
fn actual_recorded_source_producer_matches_current_digest_fixture() {
    let current = serde_json::to_value(output()).unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/analysis-output-v1.json");
    if std::env::var_os("UPDATE_REASON_SOURCE_FIXTURES").is_some() {
        std::fs::write(
            path.with_file_name("reason-results-v2.json"),
            serde_json::to_string_pretty(&results()).unwrap() + "\n",
        )
        .unwrap();
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&current).unwrap() + "\n",
        )
        .unwrap();
    }
    let captured: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(current, captured);
    let restored: AnalyzeRecordingOutput = serde_json::from_value(captured).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), current);
    let mut old = current.clone();
    old["finding"]["sourceSnapshotSha256"] = serde_json::json!(
        "sha256:b39375df89fabab90acb1982bea5a06b8c42758f6c02e9c2d0b952701afe95c1"
    );
    assert!(serde_json::from_value::<AnalyzeRecordingOutput>(old).is_err());
}

#[test]
fn changed_reason_result_tag_has_one_current_literal_and_scalar_role() {
    let schema = schemars::schema_for!(ReasoningResults);
    let node =
        schemars::Schema::try_from(schema.as_value()["properties"]["schema"].clone()).unwrap();
    assert_eq!(node.as_value()["const"], REASONING_RESULTS_SCHEMA);
    assert_eq!(
        veoveo_types::naming_profile(&node, veoveo_types::NamingSchemaContext::new(&node))
            .unwrap()
            .unwrap()
            .role(),
        &veoveo_types::NamingRole::Scalar {
            profile: veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag)
        }
    );
}

#[test]
fn actual_current_reason_tags_pass_documented_naming_receiver() {
    let results = schemars::schema_for!(ReasoningResults);
    let result_tag =
        schemars::Schema::try_from(results.as_value()["properties"]["schema"].clone()).unwrap();
    for tag in [
        result_tag,
        schemars::schema_for!(AnalysisOutputSchema),
        schemars::schema_for!(GroundingSchema),
    ] {
        veoveo_mcp_conformance::naming::NamingInspection::default()
            .schema(
                "current Reason format",
                &tag,
                None,
                veoveo_mcp_conformance::SchemaEvidenceOrigin::SourceOnly,
            )
            .unwrap();
        let mut unannotated = tag;
        unannotated
            .as_object_mut()
            .unwrap()
            .remove(veoveo_types::naming::NAMING_PROFILE_KEY);
        assert!(
            veoveo_mcp_conformance::naming::NamingInspection::default()
                .schema(
                    "unannotated Reason format",
                    &unannotated,
                    None,
                    veoveo_mcp_conformance::SchemaEvidenceOrigin::SourceOnly
                )
                .is_err()
        );
    }
}
