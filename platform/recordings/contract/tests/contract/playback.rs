use serde_json::{Value, json};
use veoveo_recording_contract::{
    PLAYBACK_MANIFEST_SCHEMA, PlaybackArchiveUri, PlaybackManifest, PlaybackManifestBuilder,
    PlaybackManifestSchema, RecordingDatasetId, RecordingId, RecordingReadGrantId,
    RecordingRedapOrigin, RecordingState,
};

pub(super) fn manifest() -> Value {
    let dataset = RecordingDatasetId::new();
    let recording = RecordingId::new();
    let origin = RecordingRedapOrigin::from_http("https://archive.example").unwrap();
    json!({
        "schema": PLAYBACK_MANIFEST_SCHEMA, "datasetId": dataset,
        "recordingSegmentId": recording, "applicationId": "producer",
        "recordingKey": "capture", "state": "sealed",
        "startedAt": "2026-09-29T00:00:00Z", "endedAt": "2026-09-29T00:01:00Z",
        "catalogRevision": "r1",
        "access": { "grantId": RecordingReadGrantId::new(), "redapToken": "opaque-token",
            "expiresAt": "2026-09-29T00:05:00Z" },
        "archive": { "uri": PlaybackArchiveUri::new(&origin, dataset, recording), "datasetId": dataset,
            "recordingSegmentId": recording, "catalogRevision": "r1", "rrdVersion": "0.38.1",
            "optimizationProfile": "object-store", "byteLen": 128, "layerCount": 1 },
        "live": null,
        "blueprint": { "blueprintId": "producer-blueprint", "revision": 1,
            "sha256": "a".repeat(64), "byteLen": 64, "mapProvider": "none" }
    })
}

#[test]
fn checked_playback_construction_and_json_share_the_wire_model() {
    let wire = manifest();
    let builder: PlaybackManifestBuilder = serde_json::from_value(wire.clone()).unwrap();
    let admitted = builder.build().unwrap();
    assert_eq!(admitted.schema, PlaybackManifestSchema::V11);
    assert_eq!(admitted.state, RecordingState::Sealed);
    assert_eq!(
        admitted.blueprint.as_ref().unwrap().sha256.hex(),
        "a".repeat(64)
    );
    assert_eq!(serde_json::to_value(&admitted).unwrap(), wire);
    let decoded: PlaybackManifest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);
    let schema = serde_json::to_value(schemars::schema_for!(PlaybackManifest)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["RecordingState"]["enum"],
        json!([
            "live",
            "ready",
            "sealing",
            "sealed",
            "interrupted",
            "failed"
        ])
    );
}

#[test]
fn manifest_admission_rejects_wrong_parents_shapes_and_values() {
    let wire = manifest();
    let origin = RecordingRedapOrigin::from_http("https://archive.example").unwrap();
    let dataset: RecordingDatasetId = serde_json::from_value(wire["datasetId"].clone()).unwrap();
    let recording: RecordingId =
        serde_json::from_value(wire["recordingSegmentId"].clone()).unwrap();
    for (pointer, invalid) in [
        ("/schema", json!("veoveo.ai/recording-playback/v10")),
        ("/state", json!("recording")),
        ("/applicationId", json!(" ")),
        ("/recordingKey", json!("bad\nkey")),
        ("/catalogRevision", json!("")),
        ("/access/redapToken", json!("")),
        ("/access/expiresAt", json!("not-a-date")),
        ("/startedAt", json!("not-a-date")),
        ("/endedAt", json!("2026-09-28T23:00:00Z")),
        ("/archive/datasetId", json!(RecordingDatasetId::new())),
        ("/archive/recordingSegmentId", json!(RecordingId::new())),
        ("/archive/catalogRevision", json!("different")),
        ("/archive/uri", json!("")),
        (
            "/archive/uri",
            json!(PlaybackArchiveUri::new(
                &origin,
                RecordingDatasetId::new(),
                recording
            )),
        ),
        (
            "/archive/uri",
            json!(PlaybackArchiveUri::new(
                &origin,
                dataset,
                RecordingId::new()
            )),
        ),
        ("/archive/layerCount", json!(0)),
        ("/archive/byteLen", json!(0)),
        ("/blueprint/blueprintId", json!("")),
        ("/blueprint/revision", json!(0)),
        ("/blueprint/byteLen", json!(0)),
        ("/blueprint/sha256", json!("a".repeat(63))),
        ("/blueprint/sha256", json!("A".repeat(64))),
        (
            "/blueprint/sha256",
            json!(format!("sha256:{}", "a".repeat(64))),
        ),
    ] {
        let mut invalid_wire = wire.clone();
        *invalid_wire.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            serde_json::from_value::<PlaybackManifest>(invalid_wire.clone()).is_err(),
            "{pointer}"
        );
        // Values that reach a typed Rust builder must still fail relationship admission.
        if let Ok(builder) = serde_json::from_value::<PlaybackManifestBuilder>(invalid_wire) {
            assert!(builder.build().is_err(), "{pointer}");
        }
    }
    for pointer in ["", "/access", "/archive", "/blueprint"] {
        let mut unknown = wire.clone();
        unknown.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
        assert!(serde_json::from_value::<PlaybackManifest>(unknown).is_err());
    }
}

#[test]
fn lifecycle_requires_the_recording_scoped_live_channel() {
    let mut live = manifest();
    live["state"] = json!("live");
    live["endedAt"] = Value::Null;
    live["live"] = json!({
        "historySeconds": 1,
        "videoPrerollSeconds": 2, "transport": "rerun_rrd_channel_v2"
    });
    assert!(serde_json::from_value::<PlaybackManifest>(live.clone()).is_err());
    live["archive"] = Value::Null;
    assert!(serde_json::from_value::<PlaybackManifest>(live.clone()).is_ok());
    for (pointer, invalid) in [
        ("/live", Value::Null),
        ("/state", json!("sealed")),
        ("/endedAt", json!("2026-09-29T00:01:00Z")),
        ("/live/historySeconds", json!(0)),
        ("/live/videoPrerollSeconds", json!(0)),
    ] {
        let mut invalid_wire = live.clone();
        *invalid_wire.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            serde_json::from_value::<PlaybackManifest>(invalid_wire).is_err(),
            "{pointer}"
        );
    }
    for state in ["ready", "sealing", "sealed", "interrupted", "failed"] {
        let mut terminal = live.clone();
        terminal["state"] = json!(state);
        assert!(serde_json::from_value::<PlaybackManifest>(terminal.clone()).is_err());
        terminal["live"] = Value::Null;
        assert!(serde_json::from_value::<PlaybackManifest>(terminal).is_ok());
    }
    // Capture-layer details belong to the server's current-layer selection, not
    // a recording receiver. Stale versioned payloads must not be accepted.
    for field in ["layerId", "layerName", "ordinal", "current_byte_len"] {
        let mut unknown = live.clone();
        unknown["live"][field] = json!(0);
        assert!(serde_json::from_value::<PlaybackManifest>(unknown).is_err());
    }
}

/// Current owner-produced timestamp bytes and their actual Chrono ordering profile.
#[test]
fn chronology_browser_fixture_comes_from_checked_playback_and_chrono() {
    use chrono::{DateTime, Utc};
    use serde::Serialize;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct TimestampPosition {
        whole_second: String,
        nanosecond: u32,
    }
    fn position(value: DateTime<Utc>) -> TimestampPosition {
        TimestampPosition {
            whole_second: value.timestamp().to_string(),
            nanosecond: value.timestamp_subsec_nanos(),
        }
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: &'static str,
        input_wire: Value,
        produced_wire: Option<PlaybackManifest>,
        admitted: bool,
        started: Option<TimestampPosition>,
        ended: Option<TimestampPosition>,
        expires: Option<TimestampPosition>,
    }
    let dataset = RecordingDatasetId::parse(super::DATASET).unwrap();
    let recording = RecordingId::parse(super::RECORDING).unwrap();
    let origin = RecordingRedapOrigin::from_http("https://archive.example").unwrap();
    let mut base: PlaybackManifestBuilder = serde_json::from_value(manifest()).unwrap();
    base.dataset_id = dataset;
    base.recording_segment_id = recording;
    base.access.grant_id =
        RecordingReadGrantId::parse("0197f78e-f2f0-7a6e-8a5d-f41c691e4473").unwrap();
    let archive = base.archive.as_mut().unwrap();
    archive.dataset_id = dataset;
    archive.recording_segment_id = recording;
    archive.uri = PlaybackArchiveUri::new(&origin, dataset, recording);
    let base = serde_json::to_value(base.build().unwrap()).unwrap();
    let mut cases = Vec::new();
    for (name, started, ended, expires, expected) in [
        (
            "equal",
            "2026-10-03T00:00:00Z",
            "2026-10-03T00:00:00Z",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "nanoseconds",
            "2026-10-03T00:00:00.000000001Z",
            "2026-10-03T00:00:00.000000002Z",
            "2099-10-03T00:00:00.000000003Z",
            true,
        ),
        (
            "reversed_within_millisecond",
            "2026-10-03T00:00:00.000000002Z",
            "2026-10-03T00:00:00.000000001Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "fraction_more_than_nine",
            "2026-10-03T00:00:00.123456789123Z",
            "2026-10-03T00:00:00.123456789999Z",
            "2099-10-03T00:00:00.000000000001Z",
            true,
        ),
        (
            "positive_offset",
            "2026-10-03T05:30:00.000000001+05:30",
            "2026-10-03T00:00:00.000000002Z",
            "2099-10-03T01:30:00+01:30",
            true,
        ),
        (
            "negative_offset",
            "2026-10-02T20:30:00.000000001-03:30",
            "2026-10-03T00:00:00.000000002Z",
            "2099-10-02T20:30:00-03:30",
            true,
        ),
        (
            "leap_second",
            "2016-12-31T23:59:60.000000001Z",
            "2016-12-31T23:59:60.000000002Z",
            "2016-12-31T23:59:60.999999999Z",
            true,
        ),
        (
            "leap_before_following_second",
            "2016-12-31T23:59:60.999999999Z",
            "2017-01-01T00:00:00Z",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "following_second_before_leap",
            "2017-01-01T00:00:00Z",
            "2016-12-31T23:59:60.999999999Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "offset_leap",
            "2017-01-01T05:29:60.123456789+05:30",
            "2017-01-01T00:00:00Z",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "year_zero_leap_day",
            "0000-02-29T00:00:00Z",
            "0000-03-01T00:00:00Z",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "negative_year",
            "-0001-12-31T23:59:59.999999999Z",
            "0000-01-01T00:00:00Z",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "expanded_year",
            "+10000-01-01T00:00:00.000000001Z",
            "+10000-01-01T00:00:00.000000002Z",
            "+10001-01-01T00:00:00Z",
            true,
        ),
        (
            "minimum_year",
            "-262143-01-01T00:00:00Z",
            "-262143-01-01T00:00:00.000000001Z",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "maximum_year",
            "+262142-12-31T23:59:59.999999998Z",
            "+262142-12-31T23:59:59.999999999Z",
            "+262142-12-31T23:59:59Z",
            true,
        ),
        (
            "minimum_offset_inside",
            "-262143-01-01T01:00:00+01:00",
            "-262143-01-01T01:00:00.000000001+01:00",
            "2099-10-03T00:00:00Z",
            true,
        ),
        (
            "maximum_offset_inside",
            "+262142-12-31T22:59:59.999999998-01:00",
            "+262142-12-31T22:59:59.999999999-01:00",
            "+262142-12-31T23:59:59Z",
            true,
        ),
        (
            "minimum_offset_outside",
            "-262143-01-01T00:00:00+01:00",
            "-262143-01-01T01:00:00Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "maximum_offset_outside",
            "+262142-12-31T23:59:59-01:00",
            "+262142-12-31T23:59:59Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "invalid_calendar",
            "2026-02-30T00:00:00Z",
            "2026-03-01T00:00:00Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "invalid_century_leap",
            "1900-02-29T00:00:00Z",
            "1900-03-01T00:00:00Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "invalid_hour",
            "2026-10-03T24:00:00Z",
            "2026-10-04T00:00:00Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "invalid_offset",
            "2026-10-03T00:00:00+24:00",
            "2026-10-04T00:00:00Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "invalid_second",
            "2026-10-03T00:00:61Z",
            "2026-10-04T00:00:00Z",
            "2099-10-03T00:00:00Z",
            false,
        ),
        (
            "invalid_expiry",
            "2026-10-03T00:00:00Z",
            "2026-10-03T00:01:00Z",
            "2026-02-30T00:00:00Z",
            false,
        ),
    ] {
        let mut input = base.clone();
        input["startedAt"] = json!(started);
        input["endedAt"] = json!(ended);
        input["access"]["expiresAt"] = json!(expires);
        let produced = serde_json::from_value::<PlaybackManifest>(input.clone());
        assert_eq!(produced.is_ok(), expected, "{name}");
        let produced = produced.ok();
        cases.push(Case {
            name,
            input_wire: input,
            admitted: expected,
            started: produced.as_ref().map(|m| position(m.started_at)),
            ended: produced.as_ref().and_then(|m| m.ended_at.map(position)),
            expires: produced.as_ref().map(|m| position(m.access.expires_at)),
            produced_wire: produced,
        });
    }
    // Both forms are admitted by the actual Option receiver. A schema callback
    // must not silently turn the omitted form into a required field.
    let schema = serde_json::to_value(schemars::schema_for!(PlaybackManifest)).unwrap();
    assert!(
        !schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("endedAt"))
    );
    let validator = jsonschema::validator_for(&schema).unwrap();
    for (name, omitted) in [("live_ended_omitted", true), ("live_ended_null", false)] {
        let mut input = base.clone();
        input["state"] = json!("live");
        input["archive"] = Value::Null;
        input["live"] =
            json!({"historySeconds":1,"videoPrerollSeconds":2,"transport":"rerun_rrd_channel_v2"});
        if omitted {
            input.as_object_mut().unwrap().remove("endedAt");
        } else {
            input["endedAt"] = Value::Null;
        }
        assert!(
            validator.is_valid(&input),
            "{name}: current schema must admit omission and null"
        );
        let admitted: PlaybackManifest = serde_json::from_value(input.clone()).unwrap();
        assert!(admitted.ended_at.is_none());
        assert!(validator.is_valid(&serde_json::to_value(&admitted).unwrap()));
        cases.push(Case {
            name,
            input_wire: input,
            admitted: true,
            started: Some(position(admitted.started_at)),
            ended: None,
            expires: Some(position(admitted.access.expires_at)),
            produced_wire: Some(admitted),
        });
    }
    let produced = serde_json::to_value(cases).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../apps/console/web/testdata/recording-chronology.json");
    if std::env::var_os("UPDATE_RECORDING_CHRONOLOGY_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&produced).unwrap()),
        )
        .unwrap();
    }
    let captured: Value = serde_json::from_slice(
        &std::fs::read(&path).expect("capture current Recording chronology fixture first"),
    )
    .unwrap();
    assert_eq!(captured, produced);
}
