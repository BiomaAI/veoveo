use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{fmt::Debug, str::FromStr};
use veoveo_types::{
    AccessSubject, DataLabelId, DelegationId, GroupId, InvocationMode, InvocationProvenance,
    PolicyVersion, PrincipalId, RoleId, TenantId, WorkContextId,
};

fn assert_wire_profile<T>(accepted: &[&str], rejected: &[&str])
where
    T: FromStr + Serialize + DeserializeOwned + AsRef<str> + Debug + PartialEq,
    T::Err: Debug,
{
    for spelling in accepted {
        let value: T = spelling.parse().unwrap();
        assert_eq!(value.as_ref(), *spelling);
        assert_eq!(serde_json::to_value(&value).unwrap(), json!(spelling));
        assert_eq!(serde_json::from_value::<T>(json!(spelling)).unwrap(), value);
    }
    for spelling in rejected {
        assert!(spelling.parse::<T>().is_err(), "accepted {spelling:?}");
        assert!(serde_json::from_value::<T>(json!(spelling)).is_err());
    }
    for value in [Value::Null, json!(2), json!(["id"]), json!({"id": "value"})] {
        assert!(serde_json::from_value::<T>(value).is_err());
    }
}

#[test]
fn claim_identities_preserve_external_spelling() {
    // Claim IDs admit whitespace and Unicode without normalization or a byte limit.
    // Domain-specific IDs have their own lexical profiles.
    let long = "subject".repeat(200);
    let accepted = [
        "issuer#subject",
        "https://idp.example/subject/1",
        " Ops ",
        " ",
        "操作者",
        &long,
    ];
    let rejected = ["", "subject\n", "a\tb", "a\0b", "a\u{7f}b", "a\u{85}b"];
    assert_wire_profile::<PrincipalId>(&accepted, &rejected);
    assert_wire_profile::<TenantId>(&accepted, &rejected);
    assert_wire_profile::<DelegationId>(&accepted, &rejected);
    assert_wire_profile::<GroupId>(&accepted, &rejected);
    assert_wire_profile::<RoleId>(&accepted, &rejected);
}

#[test]
fn label_and_policy_tokens_preserve_the_repository_profile() {
    let accepted = ["cui", "vendor:label", "policy/v2", "機密", "a+b"];
    let rejected = [
        "",
        "two tokens",
        "x\n",
        "x\t",
        "x\0",
        "x\u{2003}y",
        "x\u{a0}y",
    ];
    assert_wire_profile::<DataLabelId>(&accepted, &rejected);
    assert_wire_profile::<PolicyVersion>(&accepted, &rejected);
}

#[test]
fn work_context_profile_is_a_path_identity() {
    let long = "a".repeat(1024);
    assert_wire_profile::<WorkContextId>(
        &["operations", "a-z_9", "0", "-", "_", &long],
        &[
            "",
            "Operations",
            "ops/one",
            "ops.one",
            "ops:one",
            " ops ",
            "é",
            "ops\n",
        ],
    );
    assert_eq!(
        WorkContextId::new("").unwrap_err().to_string(),
        "invalid identifier \"\": must not be empty and must contain lowercase ASCII letters, digits, hyphen, or underscore",
    );
    assert_eq!(
        WorkContextId::new("Ops").unwrap_err().to_string(),
        "invalid identifier \"Ops\": must contain only lowercase ASCII letters, digits, hyphen, or underscore",
    );
    assert_eq!(
        PrincipalId::new("").unwrap_err().to_string(),
        "invalid identifier \"\": must not be empty"
    );
    assert_eq!(
        DataLabelId::new("a b").unwrap_err().to_string(),
        "invalid identifier \"a b\": must not contain whitespace or control characters"
    );
}

#[test]
fn subjects_preserve_their_tagged_wire_representation() {
    for (subject, wire) in [
        (
            AccessSubject::Principal(PrincipalId::new("idp#actor").unwrap()),
            json!({"kind": "principal", "id": "idp#actor"}),
        ),
        (
            AccessSubject::Group(GroupId::new("operators").unwrap()),
            json!({"kind": "group", "id": "operators"}),
        ),
    ] {
        assert_eq!(serde_json::to_value(&subject).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<AccessSubject>(wire).unwrap(),
            subject
        );
    }
    for wire in [
        json!({"kind": "principal", "id": ""}),
        json!({"kind": "group"}),
        json!({"kind": "role", "id": "operator"}),
    ] {
        assert!(serde_json::from_value::<AccessSubject>(wire).is_err());
    }
}

#[test]
fn provenance_preserves_attribution_and_required_delegation() {
    let actor = PrincipalId::new("idp#actor").unwrap();
    for (value, mode, wire) in [
        (
            InvocationProvenance::Direct {
                initiator: actor.clone(),
            },
            InvocationMode::Direct,
            json!({"mode": "direct", "initiator": "idp#actor"}),
        ),
        (
            InvocationProvenance::Delegated {
                initiator: actor.clone(),
                delegation_id: DelegationId::new("delegation/17").unwrap(),
            },
            InvocationMode::Delegated,
            json!({"mode": "delegated", "initiator": "idp#actor", "delegation_id": "delegation/17"}),
        ),
        (
            InvocationProvenance::Automated,
            InvocationMode::Automated,
            json!({"mode": "automated"}),
        ),
    ] {
        assert_eq!(value.mode(), mode);
        assert_eq!(
            value.initiator(),
            if mode == InvocationMode::Automated {
                None
            } else {
                Some(&actor)
            }
        );
        assert_eq!(serde_json::to_value(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<InvocationProvenance>(wire.clone()).unwrap(),
            value
        );
        assert_eq!(serde_json::to_value(mode).unwrap(), wire["mode"]);
        assert_eq!(
            serde_json::from_value::<InvocationMode>(wire["mode"].clone()).unwrap(),
            mode
        );
    }
    for wire in [
        json!({"mode": "direct"}),
        json!({"mode": "delegated", "initiator": "idp#actor"}),
        json!({"mode": "delegated", "initiator": "idp#actor", "delegation_id": ""}),
        json!({"mode": "direct", "initiator": ""}),
        json!({"mode": "unknown"}),
    ] {
        assert!(serde_json::from_value::<InvocationProvenance>(wire).is_err());
    }
}

#[test]
fn identity_schemas_match_the_pre_extraction_contract() {
    // Captured from the MCP-owned types before moving them into this crate.
    let expected: Value =
        serde_json::from_str(include_str!("fixtures/identity_schemas.json")).unwrap();
    let mut schemas = serde_json::Map::new();
    macro_rules! capture {
        ($($name:ty),+) => { $(schemas.insert(stringify!($name).into(), serde_json::to_value(schemars::schema_for!($name)).unwrap());)+ };
    }
    capture!(
        DataLabelId,
        PrincipalId,
        TenantId,
        WorkContextId,
        DelegationId,
        GroupId,
        RoleId,
        PolicyVersion,
        AccessSubject,
        InvocationMode,
        InvocationProvenance
    );
    assert_eq!(Value::Object(schemas), expected);
}
