#[path = "support/naming.rs"]
mod naming_baseline;
use schemars::schema_for;
use serde_json::{Value, json};
use veoveo_types::{
    AccessLevel, InvocationAuthority, WorkContextGrant, WorkContextMembershipLevel,
    WorkContextOutputPolicy,
};

#[test]
fn authority_schemas_match_the_shared_wire_contract() {
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/authority_schemas.json")).unwrap();
    let actual = json!({
        "AccessLevel": schema_for!(AccessLevel),
        "WorkContextMembershipLevel": schema_for!(WorkContextMembershipLevel),
        "WorkContextGrant": schema_for!(WorkContextGrant),
        "WorkContextOutputPolicy": schema_for!(WorkContextOutputPolicy),
        "InvocationAuthority": schema_for!(InvocationAuthority),
    });
    assert_eq!(naming_baseline::constraints(actual), expected);
}

fn authority() -> Value {
    json!({
        "work_context": "operations", "tenant": "tenant", "membership": "custodian", "policy_revision": "r1",
        "output_policy": {
            "owner": {"kind":"principal","id":"operator"},
            "initial_grants": [{"subject":{"kind":"group","id":"inspectors"},"level":"read"}],
            "classification":"internal", "data_labels":["internal","sensitive"]
        },
        "provenance": {"mode":"delegated","initiator":"operator","delegation_id":"mission-grant"}
    })
}

#[test]
fn authority_retains_attribution_output_defaults_and_typed_nested_identity() {
    let wire = authority();
    let value: InvocationAuthority = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(value.work_context.as_str(), "operations");
    assert_eq!(value.tenant.as_str(), "tenant");
    assert_eq!(value.artifact_access(), AccessLevel::Admin);
    assert_eq!(serde_json::to_value(&value).unwrap(), wire);
    // View composition identities hash these bytes, including struct field order.
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        r#"{"work_context":"operations","tenant":"tenant","membership":"custodian","policy_revision":"r1","output_policy":{"owner":{"kind":"principal","id":"operator"},"initial_grants":[{"subject":{"kind":"group","id":"inspectors"},"level":"read"}],"classification":"internal","data_labels":["internal","sensitive"]},"provenance":{"mode":"delegated","initiator":"operator","delegation_id":"mission-grant"}}"#
    );
    for (path, replacement) in [
        ("/work_context", json!("Operations")),
        ("/tenant", json!("")),
        ("/policy_revision", json!("two revisions")),
        ("/membership", json!("administrator")),
        ("/output_policy/owner/id", json!("")),
        ("/output_policy/initial_grants/0/level", json!("owner")),
        ("/output_policy/classification", json!("two labels")),
        ("/provenance/delegation_id", Value::Null),
    ] {
        let mut invalid = authority();
        *invalid.pointer_mut(path).unwrap() = replacement;
        assert!(
            serde_json::from_value::<InvocationAuthority>(invalid).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn capability_and_membership_levels_keep_distinct_hierarchies() {
    use AccessLevel::{Admin, Read, Write};
    use WorkContextMembershipLevel::{Contributor, Custodian, Owner, Viewer};
    for (held, allowed) in [
        (Read, [true, false, false]),
        (Write, [true, true, false]),
        (Admin, [true, true, true]),
    ] {
        for (required, expected) in [Read, Write, Admin].into_iter().zip(allowed) {
            assert_eq!(held.allows(required), expected);
        }
    }
    for (membership, output, allowed) in [
        (Viewer, Read, [true, false, false, false]),
        (Contributor, Write, [true, true, false, false]),
        (Custodian, Admin, [true, true, true, false]),
        (Owner, Admin, [true, true, true, true]),
    ] {
        assert_eq!(membership.artifact_access(), output);
        for (required, expected) in [Viewer, Contributor, Custodian, Owner]
            .into_iter()
            .zip(allowed)
        {
            assert_eq!(membership.allows(required), expected);
        }
    }
}
