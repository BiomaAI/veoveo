//! The server library exposes the same domain types used by ingest producers.
use veoveo_recording_mcp::{
    contract::{RecordingId, RecordingUri},
    uris,
};

#[test]
fn contract_feature_exposes_the_shared_recording_types() {
    let id = RecordingId::new();
    let uri: veoveo_recording_contract::RecordingUri = RecordingUri::new(id);
    assert_eq!(uri.id(), id);
    assert_eq!(uri, uris::recording_uri(id));
    let _: veoveo_recording_contract::RecordingResource =
        veoveo_recording_mcp::contract::RecordingResource::Recording(uri);
}

#[cfg(feature = "runtime")]
#[test]
fn current_recording_schemas_pass_documented_naming_receiver() {
    use veoveo_mcp_conformance::{SchemaEvidenceOrigin, naming::NamingInspection};
    use veoveo_recording_mcp::contract::*;
    for schema in [
        schemars::schema_for!(RecordingManifest),
        schemars::schema_for!(PlaybackManifest),
        schemars::schema_for!(RecordingCatalogGrant),
        schemars::schema_for!(RecordingProjectionHandle),
        schemars::schema_for!(CreateRecordingProjectionRequest),
        schemars::schema_for!(RecordingProperties),
    ] {
        NamingInspection::default()
            .schema(
                "current Recording owner",
                &schema,
                None,
                SchemaEvidenceOrigin::SourceOnly,
            )
            .unwrap();
    }
    for schema in [
        schemars::schema_for!(RecordingManifestSchema),
        schemars::schema_for!(PlaybackManifestSchema),
        schemars::schema_for!(RecordingCatalogGrantSchema),
        schemars::schema_for!(RecordingProjectionHandleSchema),
    ] {
        NamingInspection::default()
            .schema(
                "current Recording tag",
                &schema,
                None,
                SchemaEvidenceOrigin::SourceOnly,
            )
            .unwrap();
        let mut unannotated = schema;
        unannotated
            .as_object_mut()
            .unwrap()
            .remove(veoveo_types::NAMING_PROFILE_KEY);
        assert!(
            NamingInspection::default()
                .schema(
                    "unannotated Recording tag",
                    &unannotated,
                    None,
                    SchemaEvidenceOrigin::SourceOnly
                )
                .is_err()
        );
    }
}

#[test]
fn current_controlled_projection_inputs_come_from_owner_builders() {
    use veoveo_recording_mcp::contract::*;
    let cases = [
        (
            "RecordingProjectionSampling.range",
            RecordingProjectionSampling::Range { start: 1, end: 3 },
            RecordingProjectionSparseFill::None,
            "/sampling/kind",
            vec![
                "/timeline",
                "/datasetId",
                "/entityPaths",
                "/sampling/start",
                "/sampling/end",
            ],
        ),
        (
            "RecordingProjectionSampling.latest_at",
            RecordingProjectionSampling::LatestAt { at: 1 },
            RecordingProjectionSparseFill::None,
            "/sampling/kind",
            vec!["/timeline", "/datasetId", "/entityPaths", "/sampling/at"],
        ),
        (
            "RecordingProjectionSampling.sample_grid",
            RecordingProjectionSampling::SampleGrid { values: vec![1, 2] },
            RecordingProjectionSparseFill::None,
            "/sampling/kind",
            vec![
                "/timeline",
                "/datasetId",
                "/entityPaths",
                "/sampling/values",
            ],
        ),
        (
            "RecordingProjectionSparseFill.none",
            RecordingProjectionSampling::Range { start: 1, end: 3 },
            RecordingProjectionSparseFill::None,
            "/sparseFill",
            vec!["/recordingId"],
        ),
        (
            "RecordingProjectionSparseFill.latest_at_global",
            RecordingProjectionSampling::Range { start: 1, end: 3 },
            RecordingProjectionSparseFill::LatestAtGlobal,
            "/sparseFill",
            vec!["/recordingId"],
        ),
    ];
    let actual: Vec<_> = cases
        .into_iter()
        .map(|(branch, sampling, sparse_fill, tag, required)| {
            let query = RecordingProjectionQueryBuilder {
                entity_paths: vec!["/camera/front".into()],
                component_ids: vec!["rerun.components.ImageFormat".into()],
                timeline: "sensor_time".into(),
                sampling,
                sparse_fill,
                maximum_entities: 1,
                maximum_columns: 1,
                maximum_samples: 4,
                maximum_rows: 4,
                maximum_bytes: 1024,
            }
            .build()
            .unwrap();
            let arguments = CreateRecordingProjectionRequestBuilder {
                dataset_id: RecordingDatasetId::parse("01983da0-0000-7000-8000-000000000000")
                    .unwrap(),
                recording_id: RecordingId::parse("01983da0-0000-7000-8000-000000000000").unwrap(),
                query,
                deadline_ms: 1000,
                idempotency_key: "input-fixture".into(),
                units: Default::default(),
                coordinate_frame_refs: Vec::new(),
            }
            .build()
            .unwrap();
            serde_json::json!({
                "branch": branch, "tool": "create_recording_projection", "arguments": arguments,
                "tag": tag, "required": required, "defaults": {}, "open_objects": [],
                "object_errors": {"/units": "invalid type"}
            })
        })
        .collect();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/controlled-inputs.json");
    if let Some(capture) = std::env::var_os("CAPTURE_RECORDING_FORMAT_FIXTURE") {
        use std::io::Write;
        let capture = std::path::PathBuf::from(capture);
        assert!(
            capture.is_absolute(),
            "capture requires an absolute scratch path"
        );
        assert!(
            capture
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(std::env::temp_dir().canonicalize().unwrap()),
            "capture parent must be an existing scratch directory"
        );
        let bytes = serde_json::to_string_pretty(&actual).unwrap() + "\n";
        assert!(bytes.len() <= 64 * 1024, "capture exceeds 64 KiB");
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(capture)
            .unwrap();
        output.write_all(bytes.as_bytes()).unwrap();
        output.sync_all().unwrap();
        return;
    }
    let retained: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(retained, serde_json::to_value(actual).unwrap());
}
