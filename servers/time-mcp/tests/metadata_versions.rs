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

fn active_selection() -> Value {
    json!({"pointerVersion":1,"release":{
        "releaseId":"time-release-fixture", "sourceId":"time-source-fixture",
        "datasetKind":"tzdb", "versionLabel":"fixture", "sourceUrl":"https://example.test/data",
        "sourceDigestSha256":"a".repeat(64), "artifactPath":"/tmp/fixture", "state":"active",
        "retrievedAt":"2026-01-01T00:00:00Z", "validatedAt":"2026-01-01T00:00:00Z", "recordVersion":2
    }})
}
#[test]
fn active_selection_exposes_pointer_guard_and_rejects_unadmitted_wire() {
    let wire = active_selection();
    let selection: ActiveAuthoritySelection = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(selection.pointer_version.get(), 1);
    assert_eq!(selection.release.record_version.get(), 2);
    assert_eq!(
        selection.write_guard(),
        TimeWriteGuard::Existing(TimeVersion::FIRST)
    );
    assert_eq!(serde_json::to_value(&selection).unwrap(), wire);
    check::<ActiveAuthoritySelection>(wire.clone(), "pointerVersion");
    let schema = serde_json::to_value(schemars::schema_for!(ActiveAuthoritySelection)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&wire));
    for invalid in [
        {
            let mut value = wire.clone();
            value["pointerVersion"] = json!(0);
            value
        },
        {
            let mut value = wire.clone();
            value["release"]["state"] = json!("staged");
            value
        },
        {
            let mut value = wire.clone();
            value["release"]["state"] = json!("retired");
            value
        },
        {
            let mut value = wire.clone();
            value["unexpected"] = json!(true);
            value
        },
        wire["release"].clone(),
    ] {
        assert!(!validator.is_valid(&invalid));
        assert!(serde_json::from_value::<ActiveAuthoritySelection>(invalid).is_err());
    }
    let mut staged: AuthorityReleaseValue = selection.release.clone().into();
    staged.state = AuthorityReleaseState::Staged;
    assert!(
        ActiveAuthoritySelectionValue {
            pointer_version: TimeVersion::FIRST,
            release: staged.build().unwrap()
        }
        .build()
        .is_err()
    );
    let page = AdminPage {
        items: vec![selection],
        next_cursor: None,
    };
    let wire = serde_json::to_value(page).unwrap();
    let admitted: AdminPage<ActiveAuthoritySelection> =
        serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(admitted.items.len(), 1);
    let validator = jsonschema::validator_for(
        &serde_json::to_value(schemars::schema_for!(AdminPage<ActiveAuthoritySelection>)).unwrap(),
    )
    .unwrap();
    assert!(validator.is_valid(&wire));
    let old = json!({"items":[wire["items"][0]["release"].clone()],"nextCursor":null});
    assert!(!validator.is_valid(&old));
    assert!(serde_json::from_value::<AdminPage<ActiveAuthoritySelection>>(old).is_err());
}
