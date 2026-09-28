use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use veoveo_time_mcp::*;

fn check<T: DeserializeOwned + Serialize + JsonSchema>(mut wire: Value, field: &str) {
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    assert_eq!(schema["properties"][field]["minimum"], 1);
    assert_eq!(schema["properties"][field]["maximum"], i64::MAX);
    for version in [1, i64::MAX as u64] {
        wire[field] = version.into();
        let value: T = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), wire);
    }
    for invalid in [
        json!(0),
        json!(-1),
        json!(i64::MAX as u64 + 1),
        json!(u64::MAX),
        json!(1.5),
        json!("1"),
        Value::Null,
    ] {
        wire[field] = invalid;
        assert!(serde_json::from_value::<T>(wire.clone()).is_err());
    }
    wire.as_object_mut().unwrap().remove(field);
    assert!(serde_json::from_value::<T>(wire).is_err());
}

fn source() -> Value {
    json!({"source_id":"time-source-fixture", "name":"fixture", "dataset_kind":"tzdb",
        "url":"https://example.test/data", "expected_content_type":"application/gzip",
        "enabled":true, "record_version":1})
}

fn instant() -> Value {
    json!({"tai_seconds_since_1970":0, "nanosecond":0, "uncertainty_nanoseconds":0,
        "authority":{"tzdb_release_id":"time-release-tzdb", "leap_seconds_release_id":"time-release-leaps"}})
}

#[test]
fn lifecycle_metadata_admits_only_positive_storage_versions() {
    check::<TimeSource>(source(), "record_version");
    check::<AuthorityRelease>(
        json!({"release_id":"time-release-fixture", "source_id":"time-source-fixture",
        "dataset_kind":"tzdb", "version_label":"fixture", "source_url":"https://example.test/data",
        "source_digest_sha256":"a".repeat(64), "artifact_path":"/tmp/fixture", "state":"staged",
        "retrieved_at":"2026-01-01T00:00:00Z", "validated_at":"2026-01-01T00:00:00Z", "record_version":1}),
        "record_version",
    );
    check::<TimeAcquisition>(
        json!({"acquisition_id":"time-acquisition-fixture", "source_id":"time-source-fixture",
        "expected_source_digest_sha256":null, "status":"queued", "phase":"queued", "staged_release_id":null,
        "message":"", "created_at":"2026-01-01T00:00:00Z", "updated_at":"2026-01-01T00:00:00Z", "record_version":1}),
        "record_version",
    );
    check::<TemporalEvent>(
        json!({"event_id":"event-fixture", "name":"fixture", "due":instant(),
        "state":"scheduled", "record_version":1}),
        "record_version",
    );
}

#[test]
fn immutable_calendar_and_epoch_versions_share_the_checked_type() {
    check::<OperationalCalendar>(
        json!({"calendar_id":"calendar-fixture", "version":1,
        "name":"fixture", "zone_id":"UTC", "windows":[], "excluded_dates":[]}),
        "version",
    );
    check::<MissionEpoch>(
        json!({"epoch_id":"epoch-fixture", "name":"fixture", "instant":instant(), "version":1}),
        "version",
    );
}

#[test]
fn source_creation_keeps_zero_and_cannot_decode_persisted_metadata() {
    let mut wire = source();
    wire["record_version"] = 0.into();
    let value: NewTimeSource = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(value.record_version, SourceCreationVersion);
    assert_eq!(serde_json::to_value(value).unwrap(), wire);
    assert!(serde_json::from_value::<TimeSource>(wire.clone()).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(NewTimeSource)).unwrap();
    assert_eq!(schema["properties"]["record_version"]["const"], 0);
    for version in [
        json!(1),
        json!(-1),
        json!(u64::MAX),
        json!("0"),
        Value::Null,
    ] {
        wire["record_version"] = version;
        assert!(serde_json::from_value::<NewTimeSource>(wire.clone()).is_err());
        assert!(
            serde_json::from_value::<CreateSourceRequest>(
                json!({"source":wire, "idempotency_key":"fixture"})
            )
            .is_err()
        );
    }
    wire.as_object_mut().unwrap().remove("record_version");
    assert!(serde_json::from_value::<NewTimeSource>(wire).is_err());
}
