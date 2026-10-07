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
    json!({"sourceId":"time-source-fixture", "name":"fixture", "datasetKind":"tzdb",
        "url":"https://example.test/data", "expectedContentType":"application/gzip",
        "enabled":true, "recordVersion":1})
}

fn instant() -> Value {
    json!({"taiSecondsSince1970":0, "nanosecond":0, "uncertaintyNanoseconds":0,
        "authority":{"tzdbReleaseId":"time-release-tzdb", "leapSecondsReleaseId":"time-release-leaps"}})
}

#[test]
fn lifecycle_metadata_admits_only_positive_storage_versions() {
    check::<TimeSource>(source(), "recordVersion");
    check::<AuthorityRelease>(
        json!({"releaseId":"time-release-fixture", "sourceId":"time-source-fixture",
        "datasetKind":"tzdb", "versionLabel":"fixture", "sourceUrl":"https://example.test/data",
        "sourceDigestSha256":"a".repeat(64), "artifactPath":"/tmp/fixture", "state":"staged",
        "retrievedAt":"2026-01-01T00:00:00Z", "validatedAt":"2026-01-01T00:00:00Z", "recordVersion":1}),
        "recordVersion",
    );
    check::<TimeAcquisition>(
        json!({"acquisitionId":"time-acquisition-fixture", "sourceId":"time-source-fixture",
        "expectedSourceDigestSha256":null, "status":"queued", "phase":"queued", "stagedReleaseId":null,
        "message":"", "createdAt":"2026-01-01T00:00:00Z", "updatedAt":"2026-01-01T00:00:00Z", "recordVersion":1}),
        "recordVersion",
    );
    check::<TemporalEvent>(
        json!({"eventId":"event-fixture", "name":"fixture", "due":instant(),
        "state":"scheduled", "recordVersion":1}),
        "recordVersion",
    );
}

#[test]
fn immutable_calendar_and_epoch_versions_share_the_checked_type() {
    check::<OperationalCalendar>(
        json!({"calendarId":"calendar-fixture", "version":1,
        "name":"fixture", "zoneId":"UTC", "windows":[], "excludedDates":[]}),
        "version",
    );
    check::<MissionEpoch>(
        json!({"epochId":"epoch-fixture", "name":"fixture", "instant":instant(), "version":1}),
        "version",
    );
}

#[test]
fn source_creation_keeps_zero_and_cannot_decode_persisted_metadata() {
    let mut wire = source();
    wire["recordVersion"] = 0.into();
    let value: NewTimeSource = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(value.record_version, SourceCreationVersion);
    assert_eq!(serde_json::to_value(value).unwrap(), wire);
    assert!(serde_json::from_value::<TimeSource>(wire.clone()).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(NewTimeSource)).unwrap();
    assert_eq!(schema["properties"]["recordVersion"]["const"], 0);
    for version in [
        json!(1),
        json!(-1),
        json!(u64::MAX),
        json!("0"),
        Value::Null,
    ] {
        wire["recordVersion"] = version;
        assert!(serde_json::from_value::<NewTimeSource>(wire.clone()).is_err());
        assert!(
            serde_json::from_value::<CreateSourceRequest>(
                json!({"source":wire, "idempotencyKey":"fixture"})
            )
            .is_err()
        );
    }
    wire.as_object_mut().unwrap().remove("recordVersion");
    assert!(serde_json::from_value::<NewTimeSource>(wire).is_err());
}
