use serde::{Serialize, de::DeserializeOwned};
use veoveo_time_mcp::contract::{
    AuthorityReleaseId, CalendarId, MissionEpochId, TemporalEventId, TimeAcquisitionId,
    TimeAuthorityReleaseUri, TimeExpression, TimeScope, TimeSourceId,
};
use veoveo_types::{ScopeDefinition, ScopeName};

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
    assert!(ScopeName::new("installation:additional").is_ok());
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
    let id = AuthorityReleaseId::new("time-release-iana").unwrap();
    let uri = TimeAuthorityReleaseUri::new(&id);
    assert_eq!(
        uri.as_str(),
        "time://authorities/releases/time-release-iana"
    );
    let decoded: TimeAuthorityReleaseUri = serde_json::from_value(uri.as_str().into()).unwrap();
    assert_eq!(decoded.release_id(), id);
}
