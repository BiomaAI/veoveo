use serde_json::json;
use veoveo_time_mcp::{
    CancelTemporalEventRequest, ClockPolicyError, ClockPolicyField, ClockQualityPolicy,
    CreateSourceRequest, TimeVersion, TimeVersionError, TimeWriteGuard,
};

fn policy() -> serde_json::Value {
    json!({"maximumErrorNanoseconds":1, "maximumStratum":1,
        "minimumSourceDiversity":1, "maximumHoldoverSeconds":1})
}

#[test]
fn clock_policy_builder_and_decoder_preserve_valid_wire_fields() {
    let value = ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(1)
        .maximum_stratum(1)
        .minimum_source_diversity(1)
        .maximum_holdover_seconds(1)
        .build()
        .unwrap();
    assert_eq!(serde_json::to_value(&value).unwrap(), policy());
    assert_eq!(
        serde_json::from_value::<ClockQualityPolicy>(policy()).unwrap(),
        value
    );
    assert_eq!(value.maximum_error_nanoseconds(), 1);
    assert_eq!(value.maximum_stratum(), 1);
    assert_eq!(value.minimum_source_diversity(), 1);
    assert_eq!(value.maximum_holdover_seconds(), 1);
    assert_eq!(
        ClockQualityPolicy::builder().build(),
        Err(ClockPolicyError::Missing(
            ClockPolicyField::MaximumErrorNanoseconds
        ))
    );
    assert_eq!(
        ClockQualityPolicy::builder()
            .maximum_error_nanoseconds(1)
            .maximum_stratum(0)
            .minimum_source_diversity(1)
            .maximum_holdover_seconds(1)
            .build(),
        Err(ClockPolicyError::OutOfRange(
            ClockPolicyField::MaximumStratum
        ))
    );
}

#[test]
fn clock_schema_and_decoder_admit_the_same_numeric_bounds() {
    let schema = serde_json::to_value(schemars::schema_for!(ClockQualityPolicy)).unwrap();
    for (field, max) in [
        ("maximumErrorNanoseconds", i64::MAX as u64),
        ("maximumStratum", 15),
        ("minimumSourceDiversity", u32::MAX as u64),
        ("maximumHoldoverSeconds", i64::MAX as u64),
    ] {
        assert_eq!(schema["properties"][field]["minimum"], json!(1), "{field}");
        assert_eq!(
            schema["properties"][field]["maximum"],
            json!(max),
            "{field}"
        );
        for (candidate, valid) in [
            (json!(1), true),
            (json!(max), true),
            (json!(0), false),
            (json!(-1), false),
            (json!(max + 1), false),
            (json!(u64::MAX), false),
            (json!(1.5), false),
            (json!("1"), false),
        ] {
            let mut wire = policy();
            wire[field] = candidate;
            assert_eq!(
                serde_json::from_value::<ClockQualityPolicy>(wire).is_ok(),
                valid,
                "{field}"
            );
        }
        let mut missing = policy();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ClockQualityPolicy>(missing).is_err());
    }
}

#[test]
fn version_guards_keep_zero_for_absence_and_stop_at_the_storage_limit() {
    assert_eq!(TimeWriteGuard::new(0).unwrap(), TimeWriteGuard::Absent);
    assert_eq!(
        TimeWriteGuard::Absent.next_version().unwrap(),
        TimeVersion::FIRST
    );
    assert_eq!(
        serde_json::to_value(TimeWriteGuard::Absent).unwrap(),
        json!(0)
    );
    assert_eq!(TimeVersion::FIRST.checked_next().unwrap().get(), 2);
    let max = TimeVersion::new(i64::MAX as u64).unwrap();
    assert_eq!(max.checked_next(), Err(TimeVersionError::Exhausted));
    assert_eq!(
        TimeWriteGuard::Existing(max).next_version(),
        Err(TimeVersionError::Exhausted)
    );
    for wire in [
        json!(-1),
        json!(i64::MAX as u64 + 1),
        json!(u64::MAX),
        json!(1.5),
        json!("1"),
    ] {
        assert!(serde_json::from_value::<TimeVersion>(wire.clone()).is_err());
        assert!(serde_json::from_value::<TimeWriteGuard>(wire).is_err());
    }
    assert!(serde_json::from_value::<TimeVersion>(json!(0)).is_err());
    for value in [1, i64::MAX as u64] {
        assert_eq!(
            serde_json::to_value(TimeWriteGuard::new(value).unwrap()).unwrap(),
            json!(value)
        );
        assert_eq!(
            serde_json::from_value::<TimeVersion>(json!(value))
                .unwrap()
                .get(),
            value
        );
    }
    for (schema, min) in [
        (
            serde_json::to_value(schemars::schema_for!(TimeVersion)).unwrap(),
            1,
        ),
        (
            serde_json::to_value(schemars::schema_for!(TimeWriteGuard)).unwrap(),
            0,
        ),
    ] {
        assert_eq!(schema["minimum"], json!(min));
        assert_eq!(schema["maximum"], json!(i64::MAX));
    }
}

#[test]
fn source_creation_keeps_its_zero_sentinel_while_updates_require_a_version() {
    let request: CreateSourceRequest = serde_json::from_value(json!({
        "source":{"sourceId":"time-source-example", "name":"IANA", "datasetKind":"tzdb",
            "url":"https://example.test/tzdb", "expectedContentType":"application/gzip",
            "enabled":true, "recordVersion":0}, "idempotencyKey":"create"
    }))
    .unwrap();
    assert_eq!(
        request.source.record_version,
        veoveo_time_mcp::SourceCreationVersion
    );
    for (value, valid) in [
        (0, false),
        (1, true),
        (i64::MAX as u64, true),
        (u64::MAX, false),
    ] {
        let request = serde_json::from_value::<CancelTemporalEventRequest>(json!({
            "eventId":"event-example", "expectedRecordVersion":value
        }));
        assert_eq!(request.is_ok(), valid);
    }
}
