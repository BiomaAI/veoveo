use super::*;
use crate::contract::test_support as fixture;
use serde_json::json;

#[test]
fn composition_content_preimage_and_identities_match_frozen_contract() {
    // A complete governed marker includes typed f64 coordinates and f32 color,
    // omitted optional fields, empty releases and foundational authority fields.
    const PREIMAGE: &str = r#"{"schemaVersion":2,"compositionId":"composition-281b253a-55b2-5b69-8f2b-cc214e0be326","compositionUri":"view://composition/composition-281b253a-55b2-5b69-8f2b-cc214e0be326","revision":1,"baseLayer":"base","mapReleases":[],"styleId":"default:1","governedInputs":[{"inputId":"geometry","resourceUri":"artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471","digestSha256":"41cf6794ba4200b839c53531555f0f3998df4cbb01a4d5cb0b94e3ca5e23947d","license":"Test","attribution":"Fixture"}],"overlays":[{"overlayId":"marker","governedInputIds":["geometry"],"geometry":{"kind":"inline","geometry":{"kind":"marker","position":{"kind":"wgs84","position":{"latitudeDegrees":13.723645678901233,"longitudeDegrees":-89.2123123123123,"ellipsoidalHeightMeters":100.12345678912345}}}},"style":{"strokeColor":{"red":0.0,"green":0.85,"blue":1.0,"alpha":1.0},"lineWidthMeters":2.0,"markerSizeMeters":8.0,"labelHeightMeters":12.0},"visibility":{"visible":true}}],"algorithmRevision":"veoveo.ai/view-scene-composition/v2","requestDigestSha256":"064c687744ac30894e9005e24410c3ba9ed400aabb201155a1aa82b60418561f","authority":{"principalId":"operator","invocation":{"work_context":"operations","tenant":"tenant","membership":"custodian","policy_revision":"r1","output_policy":{"owner":{"kind":"principal","id":"operator"},"classification":"internal"},"provenance":{"mode":"delegated","initiator":"operator","delegation_id":"capture"}}}}"#;
    const CONTENT: &str = "4e036a30436c93f40b07594ac3ba260382e678ff039df6e79a7e5067c5f4c87f";
    let record = fixture::composition();
    assert_eq!(
        serde_json::to_vec(&SceneCompositionContent::from(&*record.0)).unwrap(),
        PREIMAGE.as_bytes()
    );
    assert_eq!(
        Sha256Digest::from_bytes(PREIMAGE.as_bytes()).as_str(),
        CONTENT
    );
    assert_eq!(record.composition_digest_sha256().as_str(), CONTENT);
    assert_eq!(
        record.request_digest_sha256().as_str(),
        "064c687744ac30894e9005e24410c3ba9ed400aabb201155a1aa82b60418561f"
    );
    assert_eq!(
        digest_json(&fixture::authority()).unwrap().as_str(),
        "cc75930f5598fbca14fe3ed69fa13fb7a858a2a48c50f71f9965bdf7cd6f9d2c"
    );
    assert_eq!(
        record.composition_id().as_str(),
        "composition-281b253a-55b2-5b69-8f2b-cc214e0be326"
    );
}

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
    request.base_layer = LayerId::parse("other").unwrap();
    let different = SceneComposition::new(request, fixture::authority(), fixture::now()).unwrap();
    assert_ne!(original.composition_id(), different.composition_id());
    let mut authority = fixture::authority();
    authority.invocation.policy_revision = veoveo_types::PolicyVersion::parse("r2").unwrap();
    let different = SceneComposition::new(fixture::request(), authority, fixture::now()).unwrap();
    assert_ne!(original.composition_id(), different.composition_id());
    let mut authorities = Vec::new();
    let mut authority = fixture::authority();
    authority.principal_id = veoveo_types::PrincipalId::parse("other").unwrap();
    authorities.push(authority);
    let mut authority = fixture::authority();
    authority.invocation.tenant = veoveo_types::TenantId::parse("other").unwrap();
    authorities.push(authority);
    let mut authority = fixture::authority();
    authority.invocation.work_context = veoveo_types::WorkContextId::parse("other").unwrap();
    authorities.push(authority);
    for authority in authorities {
        let different =
            SceneComposition::new(fixture::request(), authority, fixture::now()).unwrap();
        assert_eq!(
            original.request_digest_sha256(),
            different.request_digest_sha256()
        );
        assert_ne!(original.composition_id(), different.composition_id());
        assert_ne!(
            original.composition_digest_sha256(),
            different.composition_digest_sha256()
        );
    }
}

#[test]
fn composition_decoder_rejects_inconsistent_parents_digests_or_inputs() {
    let value = serde_json::to_value(fixture::composition()).unwrap();
    for (path, replacement) in [
        (
            "/compositionId",
            json!(SceneCompositionId::from_stable_key(b"other")),
        ),
        (
            "/compositionUri",
            json!(super::super::super::CompositionUri::new(
                SceneCompositionId::from_stable_key(b"other")
            )),
        ),
        ("/revision", json!(2)),
        ("/schemaVersion", json!(1)),
        ("/baseLayer", json!("other")),
        ("/algorithmRevision", json!("unknown")),
        (
            "/requestDigestSha256",
            json!(Sha256Digest::from_bytes(b"other")),
        ),
        (
            "/compositionDigestSha256",
            json!(Sha256Digest::from_bytes(b"other")),
        ),
        ("/compositionDigestSha256", json!("not-a-digest")),
        ("/authority/principalId", json!("other")),
        (
            "/overlays/0/geometry/geometry/position/position/latitudeDegrees",
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
    other.input_id = SceneInputId::parse("aaa").unwrap();
    request.governed_inputs.push(other);
    let one = SceneComposition::new(request.clone(), fixture::authority(), fixture::now()).unwrap();
    request.governed_inputs.reverse();
    let two = SceneComposition::new(request, fixture::authority(), fixture::now()).unwrap();
    assert_eq!(one, two);
    let mut invalid = serde_json::to_value(one).unwrap();
    invalid["governedInputs"].as_array_mut().unwrap().reverse();
    assert!(serde_json::from_value::<SceneComposition>(invalid).is_err());
}

#[test]
fn current_version_and_algorithm_are_required_by_producer_decoder_and_schema() {
    let schema = schemars::schema_for!(SceneComposition);
    let tag =
        schemars::Schema::try_from(schema.as_value()["properties"]["algorithmRevision"].clone())
            .unwrap();
    assert_eq!(
        veoveo_types::naming_profile(&tag, veoveo_types::NamingSchemaContext::new(&tag))
            .unwrap()
            .unwrap()
            .role(),
        &veoveo_types::NamingRole::Scalar {
            profile: veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag)
        }
    );
    let original = serde_json::to_value(fixture::composition()).unwrap();
    for version in [0, 1, 3] {
        let mut request = fixture::request();
        request.schema_version = version;
        assert!(SceneComposition::new(request, fixture::authority(), fixture::now()).is_err());
        let mut invalid = original.clone();
        invalid["schemaVersion"] = json!(version);
        assert!(serde_json::from_value::<SceneComposition>(invalid.clone()).is_err());
        assert!(!jsonschema::is_valid(schema.as_value(), &invalid));
    }
    for algorithm in [
        "view-scene-composition-v1",
        "veoveo.ai/view-scene-composition/v1",
        "unknown",
    ] {
        let mut invalid = original.clone();
        invalid["algorithmRevision"] = json!(algorithm);
        assert!(serde_json::from_value::<SceneComposition>(invalid.clone()).is_err());
        assert!(!jsonschema::is_valid(schema.as_value(), &invalid));
    }
}
