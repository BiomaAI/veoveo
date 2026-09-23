use jsonschema::Validator;
use veoveo_deploy_contract::{
    ArtifactCoordinate, ArtifactDescriptor, ArtifactDigest, ReleaseVersion, SourceRevision,
};

#[test]
fn artifact_identity_rejects_ambiguous_and_mutable_inputs() {
    assert!(ArtifactDigest::new("sha256:abc").is_err());
    assert!(ArtifactCoordinate::new("oci://registry.example/image:latest").is_err());
    assert!(ArtifactCoordinate::new("https://user:secret@example.test/image").is_err());
    assert!(ReleaseVersion::new("release-one").is_err());
    assert!(SourceRevision::new("main").is_err());
    assert!(SourceRevision::new("a".repeat(40)).is_ok());
}

#[test]
fn artifact_schema_is_closed_and_checks_identifier_shapes() {
    let schema = serde_json::to_value(schemars::schema_for!(ArtifactDescriptor)).unwrap();
    let validator = Validator::new(&schema).unwrap();
    let value = serde_json::json!({
        "name":"image", "kind":"oci_image", "version":"1.0.0",
        "coordinate":"oci://registry.example/image:1.0.0",
        "digest":format!("sha256:{}", "a".repeat(64))
    });
    assert!(validator.is_valid(&value));
    assert!(serde_json::from_value::<ArtifactDescriptor>(value.clone()).is_ok());
    let mut invalid = value.clone();
    invalid["digest"] = serde_json::json!("sha256:abc");
    assert!(!validator.is_valid(&invalid));
    let mut unknown = value;
    unknown["extension"] = serde_json::json!("obsolete");
    assert!(!validator.is_valid(&unknown));
    assert!(serde_json::from_value::<ArtifactDescriptor>(unknown).is_err());
}
