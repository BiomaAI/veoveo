use serde::{Serialize, de::DeserializeOwned};
use veoveo_time_mcp::contract::{
    AuthorityReleaseId, CalendarId, MissionEpochId, ResolveTimeRequest, TemporalEventId,
    TimeAcquisitionId, TimeAuthorityReleaseUri, TimeExpression, TimeScope, TimeSourceId,
};
use veoveo_types::{ResourceAddress, ResourceUri, ScopeDefinition, ScopeName};

#[test]
fn scope_wire_values_match_the_published_vocabulary_and_schema() {
    let expected = [
        "time:read",
        "time:schedule",
        "time:timeline",
        "time:event:write",
        "time:admin",
    ];
    let schema = serde_json::to_value(schemars::schema_for!(TimeScope)).unwrap();
    assert_eq!(schema["enum"], serde_json::json!(expected));
    assert_eq!(TimeScope::ALL.len(), expected.len());
    for (scope, wire) in TimeScope::ALL.iter().zip(expected) {
        assert_eq!(scope.name().as_str(), wire);
        assert_eq!(scope.to_string(), wire);
        assert_eq!(wire.parse::<TimeScope>().unwrap(), *scope);
        assert_eq!(serde_json::to_value(scope).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<TimeScope>(wire.into()).unwrap(),
            *scope
        );
    }
    for unknown in ["time:reed", "map:read", "time:read time:schedule", ""] {
        assert!(unknown.parse::<TimeScope>().is_err());
        assert!(serde_json::from_value::<TimeScope>(unknown.into()).is_err());
    }
    assert!(ScopeName::parse("installation:additional").is_ok());
}

fn check_id<T: Serialize + DeserializeOwned>(valid: &str, foreign: &str) {
    let value = serde_json::from_value::<T>(valid.into()).unwrap();
    assert_eq!(serde_json::to_value(value).unwrap(), valid);
    for invalid in ["", foreign, "../escape", "calendar-one/two"] {
        assert!(serde_json::from_value::<T>(invalid.into()).is_err());
    }
    assert!(serde_json::from_value::<T>(serde_json::json!(123)).is_err());
    assert!(serde_json::from_value::<T>(format!("{valid}{}", "a".repeat(129)).into()).is_err());
}

#[test]
fn public_id_deserialization_applies_domain_validation() {
    check_id::<TimeSourceId>("time-source-iana", "time-release-iana");
    check_id::<AuthorityReleaseId>("time-release-iana", "time-source-iana");
    check_id::<TimeAcquisitionId>("time-acquisition-iana", "time-source-iana");
    check_id::<CalendarId>("calendar-mission", "epoch-launch");
    check_id::<MissionEpochId>("epoch-launch", "calendar-mission");
    check_id::<TemporalEventId>("event-launch", "epoch-launch");
    assert!(
        serde_json::from_value::<TimeExpression>(serde_json::json!({
            "format": "epoch_relative", "epochId": "calendar-mission", "offsetNanoseconds": 0,
        }))
        .is_err()
    );
}

#[test]
fn consumer_constructs_authority_identity_from_the_owning_library() {
    let id = AuthorityReleaseId::parse("time-release-iana").unwrap();
    let uri = TimeAuthorityReleaseUri::new(&id);
    assert_eq!(
        uri.as_str(),
        "time://authorities/releases/time-release-iana"
    );
    let decoded: TimeAuthorityReleaseUri = serde_json::from_value(uri.as_str().into()).unwrap();
    assert_eq!(decoded.release_id(), &id);
    assert_eq!(uri.to_uri().unwrap().as_str(), uri.as_str());
    assert_eq!(
        <TimeAuthorityReleaseUri as ResourceAddress>::parse(&uri.to_uri().unwrap()).unwrap(),
        uri
    );
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(TimeAuthorityReleaseUri)).unwrap()["type"],
        "string"
    );
}

#[test]
fn release_uri_rejects_wrong_routes_ids_queries_and_encoded_aliases() {
    for invalid in [
        "time://authorities/releases/calendar-one",
        "time://authorities/releases/time-release-",
        "map://authorities/releases/time-release-one",
        "time://epochs/releases/time-release-one",
        "time://authorities/release/time-release-one",
        "time://authorities/releases/time-release-one/extra",
        "time://authorities/releases/time-release-one/",
        "time://authorities/releases/time-release-one?",
        "time://authorities/releases/time-release-one?cursor=one",
        "time://authorities/releases/time-release-one#fragment",
        "time://authorities/releases/time-release-%6fne",
        "time://authorities/releases/time-release-one%2Ftwo",
        "time://authorities/releases/../releases/time-release-one",
        "time://authorities/releases/time-release-one%",
        "time://authorities/releases/{release_id}",
    ] {
        assert!(
            TimeAuthorityReleaseUri::parse(invalid).is_err(),
            "accepted {invalid}"
        );
        assert!(serde_json::from_value::<TimeAuthorityReleaseUri>(invalid.into()).is_err());
        if let Ok(reference) = ResourceUri::new(invalid) {
            assert!(<TimeAuthorityReleaseUri as ResourceAddress>::parse(&reference).is_err());
        }
    }
}

#[test]
fn release_uri_preserves_every_supported_id_character() {
    for id in [
        "time-release-alpha",
        "time-release-Alpha_9.2:edition",
        "time-release-.",
    ] {
        let id = AuthorityReleaseId::parse(id).unwrap();
        let uri = TimeAuthorityReleaseUri::new(&id);
        let decoded = TimeAuthorityReleaseUri::parse(uri.as_str()).unwrap();
        assert_eq!(decoded.release_id(), &id);
        assert_eq!(decoded, uri);
    }
}

#[test]
fn time_expression_variants_and_resolution_requests_are_closed() {
    use serde_json::json;
    let expressions = [
        json!({"format":"rfc3339","value":"2026-01-01T00:00:00Z"}),
        json!({"format":"rfc9557","value":"2026-01-01T00:00:00Z[UTC]"}),
        json!({"format":"civil","value":{"localDatetime":"2026-01-01T00:00:00","zoneId":"UTC","tzdbReleaseId":"time-release-tzdb"}}),
        json!({"format":"unix","seconds":0}),
        json!({"format":"tai","secondsSince1970":0}),
        json!({"format":"gps","week":1,"secondsOfWeek":2}),
        json!({"format":"julian_tai","day":2440587.5}),
        json!({"format":"military_dtg","value":"010000ZJAN26"}),
        json!({"format":"epoch_relative","epochId":"epoch-launch","offsetNanoseconds":0}),
    ];
    for input in expressions {
        assert!(serde_json::from_value::<TimeExpression>(input.clone()).is_ok());
        let mut extra = input.clone();
        extra["undeclared"] = json!(true);
        assert!(serde_json::from_value::<TimeExpression>(extra).is_err());
        let mut request = json!({"expression":input});
        assert!(serde_json::from_value::<ResolveTimeRequest>(request.clone()).is_ok());
        request["undeclared"] = json!(true);
        assert!(serde_json::from_value::<ResolveTimeRequest>(request).is_err());
    }
    assert!(
        serde_json::from_value::<TimeExpression>(
            json!({"format":"rfc9557","value":"2026-01-01T00:00:00Z"})
        )
        .is_err()
    );
    assert!(serde_json::from_value::<TimeExpression>(json!({"format":"civil","value":{"localDatetime":"2026-01-01T00:00:00","zoneId":"UTC","tzdbReleaseId":"time-release-tzdb","undeclared":true}})).is_err());
}

#[test]
fn acquisition_phase_wire_and_schema_use_the_owner_vocabulary() {
    use veoveo_time_mcp::contract::TimeAcquisitionPhase;
    let expected = [
        "queued",
        "downloading",
        "validating",
        "complete",
        "cancelling",
        "cancelled",
        "failed",
    ];
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(TimeAcquisitionPhase)).unwrap()["enum"],
        serde_json::json!(expected)
    );
    for (phase, spelling) in TimeAcquisitionPhase::ALL.iter().zip(expected) {
        assert_eq!(phase.as_str(), spelling);
        assert_eq!(serde_json::to_value(phase).unwrap(), spelling);
        assert_eq!(
            serde_json::from_value::<TimeAcquisitionPhase>(spelling.into()).unwrap(),
            *phase
        );
    }
    for invalid in [
        serde_json::json!("staged"),
        serde_json::json!("unknown"),
        serde_json::Value::Null,
        serde_json::json!(3),
    ] {
        assert!(serde_json::from_value::<TimeAcquisitionPhase>(invalid).is_err());
    }
}

fn assert_current_member_cut<T: Serialize + DeserializeOwned + schemars::JsonSchema>(value: T) {
    let wire = serde_json::to_value(value).unwrap();
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(
        validator.is_valid(&wire),
        "{} current schema rejected producer",
        std::any::type_name::<T>()
    );
    serde_json::from_value::<T>(wire.clone()).unwrap();
    fn members(value: &serde_json::Value, path: &str, result: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, child) in object {
                    if key.bytes().any(|b| b.is_ascii_uppercase()) {
                        result.push((path.into(), key.clone()));
                    }
                    members(child, &format!("{path}/{key}"), result);
                }
            }
            serde_json::Value::Array(values) => {
                for (i, child) in values.iter().enumerate() {
                    members(child, &format!("{path}/{i}"), result);
                }
            }
            _ => {}
        }
    }
    let mut keys = vec![];
    members(&wire, "", &mut keys);
    for (path, current) in keys {
        let retired: String = current
            .chars()
            .flat_map(|c| {
                if c.is_ascii_uppercase() {
                    vec!['_', c.to_ascii_lowercase()]
                } else {
                    vec![c]
                }
            })
            .collect();
        for mixed in [false, true] {
            let mut bad = wire.clone();
            let object = bad.pointer_mut(&path).unwrap().as_object_mut().unwrap();
            let value = object[&current].clone();
            if !mixed {
                object.remove(&current);
            }
            object.insert(retired.clone(), value);
            assert!(
                !validator.is_valid(&bad),
                "{} {path}/{retired} mixed={mixed}",
                std::any::type_name::<T>()
            );
            assert!(
                serde_json::from_value::<T>(bad).is_err(),
                "{} {path}/{retired} mixed={mixed}",
                std::any::type_name::<T>()
            );
        }
    }
    let mut unknown = wire;
    unknown
        .as_object_mut()
        .unwrap()
        .insert("unsupported".into(), true.into());
    assert!(!validator.is_valid(&unknown));
    assert!(serde_json::from_value::<T>(unknown).is_err());
}

#[test]
fn actual_owner_request_variants_refuse_retired_and_mixed_nested_members() {
    use veoveo_time_mcp::contract::{
        ConvertTimeRequest, EvaluateWindowsRequest, ExpandScheduleRequest,
    };
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../testdata/controlled-inputs.json")).unwrap();
    for case in cases {
        let input = case["arguments"].clone();
        match case["tool"].as_str().unwrap() {
            "resolve_time" => assert_current_member_cut(
                serde_json::from_value::<ResolveTimeRequest>(input).unwrap(),
            ),
            "convert_time" => assert_current_member_cut(
                serde_json::from_value::<ConvertTimeRequest>(input).unwrap(),
            ),
            "expand_schedule" => assert_current_member_cut(
                serde_json::from_value::<ExpandScheduleRequest>(input).unwrap(),
            ),
            "evaluate_windows" => assert_current_member_cut(
                serde_json::from_value::<EvaluateWindowsRequest>(input).unwrap(),
            ),
            tool => panic!("missing owner decoder for {tool}"),
        }
    }
    use veoveo_time_mcp::contract::{
        AuthorityBinding, ClockQualityPolicy, TemporalEvent, TemporalEventState, TimeInstant,
        TimeVersion,
    };
    let event = TemporalEvent {
        event_id: TemporalEventId::parse("event-current").unwrap(),
        name: "Current event".into(),
        due: TimeInstant {
            tai_seconds_since_1970: 100,
            nanosecond: Default::default(),
            uncertainty_nanoseconds: 10,
            authority: AuthorityBinding::new(
                AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
                AuthorityReleaseId::parse("time-release-leaps").unwrap(),
            )
            .unwrap(),
        },
        state: TemporalEventState::Scheduled,
        record_version: TimeVersion::FIRST,
    };
    assert_current_member_cut(event);
    let policy = ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(1000)
        .maximum_stratum(3)
        .minimum_source_diversity(1)
        .maximum_holdover_seconds(30)
        .build()
        .unwrap();
    assert_current_member_cut(policy);
}

#[test]
fn authority_source_and_current_page_refuse_retired_members() {
    use veoveo_time_mcp::contract::{
        AuthorityBinding, CollectionPage, EventCursor, SubsecondNanoseconds, TemporalEvent,
        TemporalEventState, TimeAuthoritySource, TimeInstant, TimeVersion,
    };
    assert_current_member_cut(TimeAuthoritySource::Acquisition {
        source_id: TimeSourceId::parse("time-source-fixture").unwrap(),
        acquisition_id: TimeAcquisitionId::parse("time-acquisition-fixture").unwrap(),
    });
    assert_current_member_cut(TimeAuthoritySource::Bootstrap {});
    assert_eq!(
        serde_json::to_value(TimeAuthoritySource::Bootstrap {}).unwrap(),
        serde_json::json!({"kind":"bootstrap"})
    );
    for field in [
        "sourceId",
        "source_id",
        "acquisitionId",
        "acquisition_id",
        "unsupported",
    ] {
        let mut invalid = serde_json::json!({"kind":"bootstrap"});
        invalid
            .as_object_mut()
            .unwrap()
            .insert(field.into(), true.into());
        assert!(
            serde_json::from_value::<TimeAuthoritySource>(invalid.clone()).is_err(),
            "{field}"
        );
        assert!(
            serde_json::from_slice::<TimeAuthoritySource>(&serde_json::to_vec(&invalid).unwrap())
                .is_err(),
            "{field}"
        );
    }
    let event = TemporalEvent {
        event_id: TemporalEventId::parse("event-fixture").unwrap(),
        name: "Fixture".into(),
        due: TimeInstant {
            tai_seconds_since_1970: 0,
            nanosecond: SubsecondNanoseconds::ZERO,
            uncertainty_nanoseconds: 0,
            authority: AuthorityBinding::new(
                AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
                AuthorityReleaseId::parse("time-release-leaps").unwrap(),
            )
            .unwrap(),
        },
        state: TemporalEventState::Scheduled,
        record_version: TimeVersion::FIRST,
    };
    let cursor = EventCursor::new(&event.event_id, 0, SubsecondNanoseconds::ZERO);
    assert_current_member_cut(CollectionPage {
        items: vec![event],
        limit: 100,
        next_cursor: Some(cursor),
    });
}
