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
            "format": "epoch_relative", "epoch_id": "calendar-mission", "offset_nanoseconds": 0,
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
        json!({"format":"rfc9557","value":"2026-01-01T00:00:00Z"}),
        json!({"format":"civil","value":{"local_datetime":"2026-01-01T00:00:00","zone_id":"UTC","tzdb_release_id":"time-release-tzdb"}}),
        json!({"format":"unix","seconds":0}),
        json!({"format":"tai","seconds_since_1970":0}),
        json!({"format":"gps","week":1,"seconds_of_week":2}),
        json!({"format":"julian_tai","day":2440587.5}),
        json!({"format":"military_dtg","value":"010000ZJAN26"}),
        json!({"format":"epoch_relative","epoch_id":"epoch-launch","offset_nanoseconds":0}),
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
    assert!(serde_json::from_value::<TimeExpression>(json!({"format":"civil","value":{"local_datetime":"2026-01-01T00:00:00","zone_id":"UTC","tzdb_release_id":"time-release-tzdb","undeclared":true}})).is_err());
}
