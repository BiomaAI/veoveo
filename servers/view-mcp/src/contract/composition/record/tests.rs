use super::*;
use crate::contract::test_support as fixture;
use serde_json::json;

#[test]
fn composition_identity_binds_request_and_authority_but_not_creation_clock() {
    let original = fixture::composition();
    let later = SceneComposition::new(
        fixture::request(),
        fixture::authority(),
        fixture::now() + chrono::Duration::seconds(1),
    )
    .unwrap();
    assert_eq!(original.composition_id(), later.composition_id());
    assert_eq!(
        original.composition_digest_sha256(),
        later.composition_digest_sha256()
    );
    let mut request = fixture::request();
    request.base_layer = LayerId::new("other").unwrap();
    let different = SceneComposition::new(request, fixture::authority(), fixture::now()).unwrap();
    assert_ne!(original.composition_id(), different.composition_id());
    let mut authority = fixture::authority();
    authority.invocation.policy_revision = veoveo_types::PolicyVersion::new("r2").unwrap();
    let different = SceneComposition::new(fixture::request(), authority, fixture::now()).unwrap();
    assert_ne!(original.composition_id(), different.composition_id());
}

#[test]
fn composition_decoder_rejects_inconsistent_parents_digests_or_inputs() {
    let value = serde_json::to_value(fixture::composition()).unwrap();
    for (path, replacement) in [
        (
            "/composition_id",
            json!(SceneCompositionId::from_stable_key(b"other")),
        ),
        (
            "/composition_uri",
            json!(super::super::super::CompositionUri::new(
                SceneCompositionId::from_stable_key(b"other")
            )),
        ),
        ("/revision", json!(2)),
        ("/schema_version", json!(2)),
        ("/base_layer", json!("other")),
        ("/algorithm_revision", json!("unknown")),
        (
            "/request_digest_sha256",
            json!(Sha256Digest::from_bytes(b"other")),
        ),
        (
            "/composition_digest_sha256",
            json!(Sha256Digest::from_bytes(b"other")),
        ),
        ("/authority/principal_id", json!("other")),
        (
            "/overlays/0/geometry/geometry/position/position/latitude_degrees",
            json!(91.0),
        ),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(path).unwrap() = replacement;
        assert!(
            serde_json::from_value::<SceneComposition>(invalid).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn composition_serialized_bytes_round_trip_without_losing_float_identity() {
    let original = fixture::composition();
    let bytes = serde_json::to_vec(&original).unwrap();
    let decoded: SceneComposition = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, original);
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    let schema = schemars::schema_for!(SceneComposition);
    jsonschema::validator_for(schema.as_value())
        .unwrap()
        .validate(&serde_json::to_value(original).unwrap())
        .unwrap();
}

#[test]
fn input_order_is_canonical_at_construction_and_checked_at_decode() {
    let mut request = fixture::request();
    let mut other = request.governed_inputs[0].clone();
    other.input_id = SceneInputId::new("aaa").unwrap();
    request.governed_inputs.push(other);
    let one = SceneComposition::new(request.clone(), fixture::authority(), fixture::now()).unwrap();
    request.governed_inputs.reverse();
    let two = SceneComposition::new(request, fixture::authority(), fixture::now()).unwrap();
    assert_eq!(one, two);
    let mut invalid = serde_json::to_value(one).unwrap();
    invalid["governed_inputs"].as_array_mut().unwrap().reverse();
    assert!(serde_json::from_value::<SceneComposition>(invalid).is_err());
}
