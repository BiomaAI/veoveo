use serde_json::{Value, json};
use veoveo_time_mcp::*;
use veoveo_types::Sha256Digest;

fn reference(name: &str, kind: AuthorityDatasetKind) -> TimeAuthorityReference {
    TimeAuthorityReference::new(
        TimeAuthorityReleaseUri::new(&AuthorityReleaseId::new(name).unwrap()),
        kind,
        TimeAuthoritySource::Bootstrap,
        Sha256Digest::from_hex("a".repeat(64)).unwrap(),
        "2026b".into(),
    )
    .unwrap()
}

#[test]
fn reference_construction_preserves_wire_fields_and_derives_identity() {
    for source in [
        TimeAuthoritySource::Bootstrap,
        TimeAuthoritySource::Acquisition {
            source_id: TimeSourceId::new("time-source-fixture").unwrap(),
            acquisition_id: TimeAcquisitionId::new("time-acquisition-fixture").unwrap(),
        },
    ] {
        let id = AuthorityReleaseId::new("time-release-2026b:published").unwrap();
        let uri = TimeAuthorityReleaseUri::new(&id);
        let reference = TimeAuthorityReference::new(
            uri.clone(),
            AuthorityDatasetKind::Tzdb,
            source.clone(),
            Sha256Digest::from_hex("a".repeat(64)).unwrap(),
            " 2026b ".into(),
        )
        .unwrap();
        let wire = json!({"release_uri":uri, "release_id":id, "dataset_kind":"tzdb", "source":source,
            "source_digest":format!("sha256:{}", "a".repeat(64)), "version_label":" 2026b "});
        assert_eq!(serde_json::to_value(&reference).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<TimeAuthorityReference>(wire).unwrap(),
            reference
        );
        assert_eq!(reference.release_uri(), &uri);
        assert_eq!(reference.release_id(), &id);
        assert_eq!(reference.source(), &source);
        assert_eq!(reference.version_label(), " 2026b ");
        let changed = TimeAuthorityReference::new(
            uri,
            AuthorityDatasetKind::Tzdb,
            source,
            Sha256Digest::from_hex("b".repeat(64)).unwrap(),
            " 2026b ".into(),
        )
        .unwrap();
        assert_ne!(
            reference, changed,
            "downstream equality includes content identity"
        );
    }
}

#[test]
fn reference_decoder_rejects_conflicting_identity_and_blank_labels() {
    let value = reference("time-release-fixture", AuthorityDatasetKind::Tzdb);
    let original = serde_json::to_value(&value).unwrap();
    let mut wrong = original.clone();
    wrong["release_id"] = "time-release-sensitive-payload".into();
    let error = serde_json::from_value::<TimeAuthorityReference>(wrong)
        .unwrap_err()
        .to_string();
    assert!(error.contains("URI and identity"));
    assert!(!error.contains("sensitive-payload"));
    for label in ["", " ", "\t\r\n", "\u{2003}"] {
        assert_eq!(
            TimeAuthorityReference::new(
                value.release_uri().clone(),
                value.dataset_kind(),
                value.source().clone(),
                value.source_digest().clone(),
                label.into()
            ),
            Err(TimeAuthorityError::BlankVersionLabel)
        );
        let mut wire = original.clone();
        wire["version_label"] = label.into();
        assert!(serde_json::from_value::<TimeAuthorityReference>(wire).is_err());
    }
    for field in [
        "release_uri",
        "release_id",
        "dataset_kind",
        "source",
        "source_digest",
        "version_label",
    ] {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<TimeAuthorityReference>(missing).is_err(),
            "{field}"
        );
    }
    let mut extra = original;
    extra["extra"] = Value::Bool(true);
    assert!(serde_json::from_value::<TimeAuthorityReference>(extra).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(TimeAuthorityReference)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["version_label"]["minLength"], 1);
}

#[test]
fn effective_pair_checks_roles_and_derives_the_instant_binding() {
    let tzdb = reference("time-release-tzdb", AuthorityDatasetKind::Tzdb);
    let leaps = reference("time-release-leaps", AuthorityDatasetKind::LeapSeconds);
    let pair = EffectiveTimeAuthority::new(tzdb.clone(), leaps.clone()).unwrap();
    assert_eq!(pair.tzdb(), &tzdb);
    assert_eq!(pair.leap_seconds(), &leaps);
    assert_eq!(pair.binding().tzdb_release_id(), tzdb.release_id());
    assert_eq!(pair.binding().leap_seconds_release_id(), leaps.release_id());
    let wire = json!({"tzdb":tzdb, "leap_seconds":leaps});
    assert_eq!(serde_json::to_value(&pair).unwrap(), wire);
    assert_eq!(
        serde_json::from_value::<EffectiveTimeAuthority>(wire).unwrap(),
        pair
    );
    for (first, second, reason) in [
        (leaps.clone(), tzdb.clone(), TimeAuthorityError::DatasetKind),
        (tzdb.clone(), tzdb.clone(), TimeAuthorityError::DatasetKind),
        (
            tzdb.clone(),
            reference("time-release-tzdb", AuthorityDatasetKind::LeapSeconds),
            TimeAuthorityError::DuplicateRelease,
        ),
    ] {
        assert_eq!(
            EffectiveTimeAuthority::new(first.clone(), second.clone()),
            Err(reason)
        );
        assert!(
            serde_json::from_value::<EffectiveTimeAuthority>(
                json!({"tzdb":first,"leap_seconds":second})
            )
            .is_err()
        );
    }
    let schema = serde_json::to_value(schemars::schema_for!(EffectiveTimeAuthority)).unwrap();
    assert_eq!(
        schema["properties"]["tzdb"]["allOf"][1]["properties"]["dataset_kind"]["const"],
        "tzdb"
    );
    assert_eq!(
        schema["properties"]["leap_seconds"]["allOf"][1]["properties"]["dataset_kind"]["const"],
        "leap_seconds"
    );
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn instant_bindings_reject_one_identity_assigned_to_both_families() {
    let id = AuthorityReleaseId::new("time-release-fixture").unwrap();
    assert_eq!(
        AuthorityBinding::new(id.clone(), id.clone()),
        Err(TimeAuthorityError::DuplicateRelease)
    );
    let wire = json!({"tzdb_release_id":id,"leap_seconds_release_id":id});
    assert!(serde_json::from_value::<AuthorityBinding>(wire.clone()).is_err());
    assert!(
        serde_json::from_value::<TimeInstant>(json!({"tai_seconds_since_1970":0,
        "nanosecond":0,"uncertainty_nanoseconds":0,"authority":wire}))
        .is_err()
    );
}

#[test]
fn deterministic_outputs_require_the_effective_authority_contract() {
    let resolve = serde_json::to_value(schemars::schema_for!(ResolveTimeOutput)).unwrap();
    let convert = serde_json::to_value(schemars::schema_for!(ConvertTimeOutput)).unwrap();
    assert!(resolve["properties"]["effective_authority"].is_object());
    assert!(convert["properties"]["canonical"].is_object());
}

fn resolution() -> ResolveTimeOutput {
    let authority = EffectiveTimeAuthority::new(
        reference("time-release-tzdb", AuthorityDatasetKind::Tzdb),
        reference("time-release-leaps", AuthorityDatasetKind::LeapSeconds),
    )
    .unwrap();
    ResolveTimeOutput::new(
        TimeInstant::from_total_nanoseconds(63_072_010_000_000_000, 7, authority.binding())
            .unwrap(),
        authority,
        TimeProjection {
            utc_rfc3339: "1972-01-01T00:00:00Z".into(),
            utc_is_leap_second: false,
            military_dtg: "010000ZJAN72".into(),
            unix_seconds: 63_072_000,
            gps_week: None,
            gps_seconds_of_week: None,
            julian_day_tai: 2_441_317.500_115_740_6,
        },
    )
    .unwrap()
}

#[test]
fn resolved_output_preserves_flat_wire_schema_and_read_only_metadata() {
    let output = resolution();
    let expected = json!({
        "instant": output.instant(), "effective_authority": output.effective_authority(),
        "utc_rfc3339":"1972-01-01T00:00:00Z", "utc_is_leap_second":false,
        "military_dtg":"010000ZJAN72", "unix_seconds":63_072_000,
        "gps_week":null, "gps_seconds_of_week":null, "julian_day_tai":2_441_317.500_115_740_6,
    });
    assert_eq!(serde_json::to_value(&output).unwrap(), expected);
    assert_eq!(
        serde_json::from_value::<ResolveTimeOutput>(expected.clone()).unwrap(),
        output
    );
    assert_eq!(
        output.instant().authority,
        output.effective_authority().binding()
    );
    assert_eq!(output.clone().into_instant(), *output.instant());
    let schema = serde_json::to_value(schemars::schema_for!(ResolveTimeOutput)).unwrap();
    let mut fields: Vec<_> = schema["properties"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    fields.sort();
    let mut expected_fields: Vec<_> = expected.as_object().unwrap().keys().cloned().collect();
    expected_fields.sort();
    assert_eq!(fields, expected_fields);
    let mut required: Vec<_> = schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    required.sort();
    assert_eq!(
        required,
        vec![
            "effective_authority",
            "instant",
            "julian_day_tai",
            "military_dtg",
            "unix_seconds",
            "utc_is_leap_second",
            "utc_rfc3339"
        ]
    );
    for field in required {
        let mut missing = expected.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ResolveTimeOutput>(missing).is_err(),
            "{field}"
        );
    }
    let mut optional = expected;
    for field in ["gps_week", "gps_seconds_of_week"] {
        optional.as_object_mut().unwrap().remove(field);
    }
    optional["extra"] = true.into();
    assert_eq!(
        serde_json::from_value::<ResolveTimeOutput>(optional).unwrap(),
        output
    );
}

#[test]
fn resolved_and_converted_outputs_reject_either_mismatched_authority_family() {
    let output = resolution();
    for family in ["tzdb_release_id", "leap_seconds_release_id"] {
        let mut wire = serde_json::to_value(&output).unwrap();
        wire["instant"]["authority"][family] = "time-release-sensitive-input".into();
        let instant: TimeInstant = serde_json::from_value(wire["instant"].clone()).unwrap();
        assert_eq!(
            ResolveTimeOutput::new(
                instant,
                output.effective_authority().clone(),
                output.projection().clone()
            ),
            Err(ResolutionAuthorityMismatch),
        );
        let error = serde_json::from_value::<ResolveTimeOutput>(wire.clone())
            .unwrap_err()
            .to_string();
        assert_eq!(error, ResolutionAuthorityMismatch.to_string());
        assert!(!error.contains("sensitive-input"));
        assert!(
            serde_json::from_value::<ConvertTimeOutput>(
                json!({"canonical":wire,"zoned":[],"scales":[]})
            )
            .is_err()
        );
    }
}
