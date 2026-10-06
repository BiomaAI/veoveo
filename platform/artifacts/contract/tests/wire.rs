fn capture_schema<T: schemars::JsonSchema>(
    schemas: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) {
    schemas.insert(
        name.into(),
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
    );
}
use serde_json::{Value, json};
use veoveo_artifact_contract::{
    ArtifactId, ArtifactMetadata, ArtifactProvenance, ArtifactReleaseState, ComplianceMetadata,
};

const OCCURRENCE: &str = "0197f78e-f2f0-7a6e-8a5d-f41c691e4471";

#[test]
fn occurrence_admission_and_canonical_spelling_are_preserved() {
    let expected = ArtifactId::parse(OCCURRENCE).unwrap();
    for spelling in [
        OCCURRENCE.to_owned(),
        OCCURRENCE.to_ascii_uppercase(),
        OCCURRENCE.replace('-', ""),
        format!("urn:uuid:{OCCURRENCE}"),
        format!("{{{OCCURRENCE}}}"),
    ] {
        assert_eq!(ArtifactId::parse(&spelling).unwrap(), expected);
        assert_eq!(spelling.parse::<ArtifactId>().unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<ArtifactId>(json!(spelling)).unwrap(),
            expected
        );
    }
    assert_eq!(serde_json::to_value(expected).unwrap(), json!(OCCURRENCE));
    for invalid in [
        "",
        "not-a-uuid",
        "00000000-0000-0000-0000-000000000000",
        "0197f78e-f2f0-4a6e-8a5d-f41c691e4471",
    ] {
        assert_eq!(
            ArtifactId::parse(invalid).unwrap_err().to_string(),
            "artifact id must be a UUIDv7"
        );
        assert!(serde_json::from_value::<ArtifactId>(json!(invalid)).is_err());
    }
    for value in [Value::Null, json!(1), json!({"id": OCCURRENCE})] {
        assert!(serde_json::from_value::<ArtifactId>(value).is_err());
    }
    assert_eq!(ArtifactId::new().as_uuid().get_version_num(), 7);
}

#[test]
fn artifact_metadata_preserves_nested_identity_and_attribution() {
    let wire = json!({
        "artifact_id": OCCURRENCE,
        "byte_len": 3,
        "mime_type": "text/plain",
        "filename": "informe-ñ.txt",
        "artifact_uri": format!("artifact://{OCCURRENCE}"),
        "download_url": "https://example.test/download",
        "created_at": "2026-09-27T00:00:00Z",
        "release_state": "private",
        "compliance": {
            "classification": "cui",
            "tenant_id": "enterprise",
            "owner": {"kind": "group", "id": "operators"},
            "work_context": "operations",
            "provenance": {
                "producer": "issuer#worker",
                "invocation_mode": "delegated",
                "initiator": "issuer#operator",
                "delegation_id": "delegation/17",
                "policy_revision": "v1"
            },
            "data_labels": ["cui"],
            "retention_expires_at": "2026-10-27T00:00:00Z"
        },
        "metadata": {"producer_annotation": [1, "open metadata"]}
    });
    let value: ArtifactMetadata = serde_json::from_value(wire.clone()).unwrap();
    let shared: Value =
        serde_json::from_str(include_str!("fixtures/metadata-output.json")).unwrap();
    assert_eq!(serde_json::to_value(&value).unwrap(), shared);
    assert_eq!(serde_json::to_value(&value).unwrap(), wire);
    let without_url = value.without_download_url();
    assert!(
        serde_json::to_value(without_url)
            .unwrap()
            .get("download_url")
            .is_none()
    );
    for pointer in [
        "/artifact_id",
        "/compliance/tenant_id",
        "/compliance/work_context",
        "/compliance/owner/id",
        "/compliance/provenance/producer",
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!("");
        assert!(
            serde_json::from_value::<ArtifactMetadata>(invalid).is_err(),
            "accepted {pointer}"
        );
    }
}

#[test]
fn schemas_match_the_published_contract() {
    let expected: Value = serde_json::from_str(include_str!("fixtures/schemas.json")).unwrap();
    let mut schemas = serde_json::Map::new();

    capture_schema::<ArtifactId>(&mut schemas, stringify!(ArtifactId));
    capture_schema::<ArtifactMetadata>(&mut schemas, stringify!(ArtifactMetadata));
    capture_schema::<ArtifactProvenance>(&mut schemas, stringify!(ArtifactProvenance));
    capture_schema::<ArtifactReleaseState>(&mut schemas, stringify!(ArtifactReleaseState));
    capture_schema::<ComplianceMetadata>(&mut schemas, stringify!(ComplianceMetadata));
    assert_eq!(Value::Object(schemas), expected);
}
