use serde_json::{Value, json};
use veoveo_time_mcp::{
    AuthorityBinding, AuthorityReleaseId, SubsecondNanoseconds, TimeCoordinateError,
    TimeExpression, TimeInstant,
};

fn authority() -> AuthorityBinding {
    AuthorityBinding::new(
        AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
        AuthorityReleaseId::parse("time-release-leaps").unwrap(),
    )
    .unwrap()
}

#[test]
fn subsecond_schema_and_numeric_admission_agree() {
    let schema = serde_json::to_value(schemars::schema_for!(SubsecondNanoseconds)).unwrap();
    assert_eq!(schema["type"], "integer");
    assert_eq!(schema["minimum"], 0);
    assert_eq!(schema["maximum"], 999_999_999);
    for nanos in [0, 1, 500_000_000, 999_999_999] {
        let value = SubsecondNanoseconds::new(nanos).unwrap();
        assert_eq!(value.get(), nanos);
        assert_eq!(serde_json::to_value(value).unwrap(), json!(nanos));
        assert_eq!(
            serde_json::from_value::<SubsecondNanoseconds>(json!(nanos)).unwrap(),
            value
        );
    }
    assert_eq!(SubsecondNanoseconds::default(), SubsecondNanoseconds::ZERO);
    for nanos in [1_000_000_000, u32::MAX] {
        assert_eq!(
            SubsecondNanoseconds::new(nanos),
            Err(TimeCoordinateError::InvalidNanosecond)
        );
    }
    for value in [
        json!(-1),
        json!(1_000_000_000),
        json!(u64::MAX),
        json!(0.5),
        json!("0"),
        Value::Null,
    ] {
        assert!(serde_json::from_value::<SubsecondNanoseconds>(value).is_err());
    }
}

#[test]
fn instant_and_expression_wires_keep_numeric_fractions_and_zero_defaults() {
    let mut instant = json!({"taiSecondsSince1970":-1, "nanosecond":999_999_999,
        "uncertaintyNanoseconds":u64::MAX, "authority":authority()});
    let decoded: TimeInstant = serde_json::from_value(instant.clone()).unwrap();
    assert_eq!(decoded.total_nanoseconds(), -1);
    assert_eq!(serde_json::to_value(decoded).unwrap(), instant);
    let schema = serde_json::to_value(schemars::schema_for!(TimeInstant)).unwrap();
    assert_eq!(schema["properties"]["nanosecond"]["maximum"], 999_999_999);
    for format in ["unix", "tai"] {
        let seconds = if format == "unix" {
            "seconds"
        } else {
            "secondsSince1970"
        };
        let wire = json!({"format":format, seconds:0});
        let decoded: TimeExpression = serde_json::from_value(wire.clone()).unwrap();
        let mut expected = wire;
        expected["nanosecond"] = 0.into();
        assert_eq!(serde_json::to_value(decoded).unwrap(), expected);
        for invalid in [
            json!(-1),
            json!(1_000_000_000),
            json!(u32::MAX),
            json!(0.5),
            json!("0"),
            Value::Null,
        ] {
            instant["nanosecond"] = invalid.clone();
            expected["nanosecond"] = invalid;
            assert!(serde_json::from_value::<TimeInstant>(instant.clone()).is_err());
            assert!(serde_json::from_value::<TimeExpression>(expected.clone()).is_err());
        }
    }
    instant.as_object_mut().unwrap().remove("nanosecond");
    assert!(serde_json::from_value::<TimeInstant>(instant).is_err());
}

#[test]
fn total_coordinates_round_trip_negative_fractions_and_both_signed_endpoints() {
    let minimum = i128::from(i64::MIN) * 1_000_000_000;
    let maximum = i128::from(i64::MAX) * 1_000_000_000 + 999_999_999;
    for total in [
        minimum,
        minimum + 1,
        -1_000_000_001,
        -1_000_000_000,
        -1,
        0,
        1,
        999_999_999,
        1_000_000_000,
        maximum,
    ] {
        let value = TimeInstant::from_total_nanoseconds(total, 42, authority()).unwrap();
        assert_eq!(value.total_nanoseconds(), total);
        assert_eq!(value.uncertainty_nanoseconds, 42);
        assert_eq!(value.authority, authority());
    }
    let negative = TimeInstant::from_total_nanoseconds(-1, 0, authority()).unwrap();
    assert_eq!(negative.tai_seconds_since_1970, -1);
    assert_eq!(negative.nanosecond, SubsecondNanoseconds::MAX);
    for total in [i128::MIN, minimum - 1, maximum + 1, i128::MAX] {
        assert_eq!(
            TimeInstant::from_total_nanoseconds(total, 0, authority()),
            Err(TimeCoordinateError::SecondsOverflow)
        );
    }
}

#[test]
fn checked_authority_wire_and_instant_reject_unknown_fields() {
    let binding = serde_json::to_value(authority()).unwrap();
    let mut extra = binding.clone();
    extra["undeclared"] = json!(true);
    assert!(serde_json::from_value::<AuthorityBinding>(extra).is_err());
    let input = json!({"taiSecondsSince1970":0,"nanosecond":0,"uncertaintyNanoseconds":0,"authority":binding});
    assert!(serde_json::from_value::<TimeInstant>(input.clone()).is_ok());
    for path in ["", "/authority"] {
        let mut extra = input.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("undeclared".to_owned(), json!(true));
        assert!(
            serde_json::from_value::<TimeInstant>(extra).is_err(),
            "{path}"
        );
    }
}
