use serde_json::{Value, json};
use veoveo_artifact_contract::{ArtifactMetadata, ArtifactProvenance};
use veoveo_types::{DelegationId, InvocationProvenance, PolicyVersion, PrincipalId};

fn principal(id: &str) -> PrincipalId {
    PrincipalId::parse(id).unwrap()
}

fn invocations() -> [(InvocationProvenance, Value); 3] {
    [
        (
            InvocationProvenance::Direct {
                initiator: principal("issuer#operator"),
            },
            json!({"invocationMode": "direct", "initiator": "issuer#operator"}),
        ),
        (
            InvocationProvenance::Delegated {
                initiator: principal("issuer#operator"),
                delegation_id: DelegationId::parse("delegation/17").unwrap(),
            },
            json!({
                "invocationMode": "delegated",
                "initiator": "issuer#operator",
                "delegationId": "delegation/17"
            }),
        ),
        (
            InvocationProvenance::Automated,
            json!({"invocationMode": "automated"}),
        ),
    ]
}

#[test]
fn typed_invocation_matches_current_wire_profile() {
    let schema = serde_json::to_value(schemars::schema_for!(ArtifactProvenance)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for (invocation, mut wire) in invocations() {
        wire["producer"] = json!("issuer#worker");
        wire["policyRevision"] = json!("v1");
        let typed = ArtifactProvenance::new(
            principal("issuer#worker"),
            invocation.clone(),
            PolicyVersion::parse("v1").unwrap(),
        );
        let serialized = serde_json::to_value(&typed).unwrap();
        assert_eq!(serialized, wire);
        assert!(validator.is_valid(&serialized));
        assert_eq!(
            serde_json::from_value::<ArtifactProvenance>(wire.clone()).unwrap(),
            typed
        );
        // Optional fields admit explicit null and omit it on output.
        if invocation.initiator().is_none() {
            wire["initiator"] = Value::Null;
        }
        if !matches!(invocation, InvocationProvenance::Delegated { .. }) {
            wire["delegationId"] = Value::Null;
        }
        assert!(validator.is_valid(&wire));
        assert_eq!(
            serde_json::from_value::<ArtifactProvenance>(wire).unwrap(),
            typed
        );
    }
}

#[test]
fn schema_and_decoder_agree_on_mode_and_identity_combinations() {
    let schema = serde_json::to_value(schemars::schema_for!(ArtifactProvenance)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    // Missing, null, valid identity, and wrong JSON shapes at each optional field.
    let values = [
        None,
        Some(Value::Null),
        Some(json!("identity")),
        Some(json!(1)),
        Some(json!({})),
        Some(json!([])),
        Some(json!(false)),
    ];
    for mode in ["direct", "delegated", "automated", "unknown"] {
        for (initiator_index, initiator) in values.iter().enumerate() {
            for (delegation_index, delegation) in values.iter().enumerate() {
                let mut wire = json!({
                    "producer": "issuer#worker",
                    "invocationMode": mode,
                    "policyRevision": "v1"
                });
                if let Some(initiator) = initiator {
                    wire["initiator"] = initiator.clone();
                }
                if let Some(delegation) = delegation {
                    wire["delegationId"] = delegation.clone();
                }
                let expected = match mode {
                    "direct" => initiator_index == 2 && delegation_index < 2,
                    "delegated" => initiator_index == 2 && delegation_index == 2,
                    "automated" => initiator_index < 2 && delegation_index < 2,
                    _ => false,
                };
                assert_eq!(validator.is_valid(&wire), expected, "schema: {wire}");
                assert_eq!(
                    serde_json::from_value::<ArtifactProvenance>(wire.clone()).is_ok(),
                    expected,
                    "decoder: {wire}"
                );
            }
        }
    }
}

#[test]
fn nested_metadata_rejects_inconsistent_attribution() {
    let mut metadata = json!({
        "artifactId": "0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
        "byteLen": 0,
        "artifactUri": "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
        "createdAt": "2026-09-27T00:00:00Z",
        "compliance": {"provenance": {
            "producer": "issuer#worker",
            "invocationMode": "delegated",
            "initiator": "issuer#operator",
            "policyRevision": "v1"
        }}
    });
    let schema = serde_json::to_value(schemars::schema_for!(ArtifactMetadata)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(!validator.is_valid(&metadata));
    assert!(serde_json::from_value::<ArtifactMetadata>(metadata.clone()).is_err());
    metadata["compliance"]["provenance"]["delegationId"] = json!("delegation/17");
    assert!(validator.is_valid(&metadata));
    assert!(serde_json::from_value::<ArtifactMetadata>(metadata).is_ok());
}

#[test]
fn provenance_refuses_obsolete_owned_keys_even_when_optional() {
    let wire = json!({"invocationMode":"direct", "producer":"issuer#worker", "initiator":"issuer#operator", "policyRevision":"v1"});
    assert!(serde_json::from_value::<ArtifactProvenance>(wire.clone()).is_ok());
    for old in ["invocation_mode", "delegation_id", "policy_revision"] {
        let mut mixed = wire.clone();
        mixed[old] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<ArtifactProvenance>(mixed).is_err(),
            "obsolete field {old}"
        );
    }
}
