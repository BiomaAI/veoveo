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
        "artifactId": OCCURRENCE,
        "byteLen": 3,
        "mimeType": "text/plain",
        "filename": "informe-ñ.txt",
        "artifactUri": format!("artifact://{OCCURRENCE}"),
        "downloadUrl": "https://example.test/download",
        "createdAt": "2026-09-27T00:00:00Z",
        "releaseState": "private",
        "compliance": {
            "classification": "cui",
            "tenantId": "enterprise",
            "owner": {"kind": "group", "id": "operators"},
            "workContext": "operations",
            "provenance": {
                "producer": "issuer#worker",
                "invocationMode": "delegated",
                "initiator": "issuer#operator",
                "delegationId": "delegation/17",
                "policyRevision": "v1"
            },
            "dataLabels": ["cui"],
            "retentionExpiresAt": "2026-10-27T00:00:00Z"
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
            .get("downloadUrl")
            .is_none()
    );
    for pointer in [
        "/artifactId",
        "/compliance/tenantId",
        "/compliance/workContext",
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
    let actual = Value::Object(schemas);
    if std::env::var_os("VEOVEO_CAPTURE_ARTIFACT_SCHEMAS").is_some() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schemas.json");
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&actual).unwrap()),
        )
        .unwrap();
    } else {
        assert_eq!(actual, expected);
    }
}

#[test]
fn metadata_wire_refuses_obsolete_keys_without_changing_open_producer_metadata() {
    let value: Value = serde_json::from_str(include_str!("fixtures/metadata-output.json")).unwrap();
    for (old, current) in [
        ("artifact_id", "artifactId"),
        ("byte_len", "byteLen"),
        ("mime_type", "mimeType"),
        ("artifact_uri", "artifactUri"),
        ("created_at", "createdAt"),
    ] {
        let mut invalid = value.clone();
        let original = invalid.as_object_mut().unwrap().remove(current).unwrap();
        invalid[old] = original;
        assert!(
            serde_json::from_value::<ArtifactMetadata>(invalid).is_err(),
            "{old}"
        );
    }
    let mut producer = value;
    producer["metadata"] = json!({"producer_owned_key": {"arbitrary spelling": true}});
    let admitted: ArtifactMetadata = serde_json::from_value(producer.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(admitted).unwrap()["metadata"],
        producer["metadata"]
    );
}

#[test]
fn chrono_timestamp_schemas_admit_actual_owner_values_and_keep_optional_profiles() {
    use veoveo_artifact_contract::{
        ArtifactAccessRequest, ArtifactMetadataSnapshot, ArtifactReadGrant, ArtifactShareLink,
        ArtifactUploadId, ArtifactUploadReceipt, ArtifactUploadSession,
        CreateArtifactShareLinkRequest, Grant, IssueArtifactReadCapabilityRequest,
        IssueArtifactWriteCapabilityRequest, IssuedArtifactReadCapability,
        IssuedArtifactWriteCapability, PutArtifactRequest, UploadSha256,
    };
    let receipt_schema =
        serde_json::to_value(schemars::schema_for!(ArtifactUploadReceipt)).unwrap();
    let metadata_schema = serde_json::to_value(schemars::schema_for!(ArtifactMetadata)).unwrap();
    let receipt_validator = jsonschema::validator_for(&receipt_schema).unwrap();
    let metadata_validator = jsonschema::validator_for(&metadata_schema).unwrap();
    let original: ArtifactMetadata =
        serde_json::from_str(include_str!("fixtures/metadata-output.json")).unwrap();
    let id = original.artifact_id();
    let times = [
        "2026-10-05T12:34:56Z".parse().unwrap(),
        "2026-10-05T12:34:56.123456789Z".parse().unwrap(),
        "2026-10-05T12:34:56.123456001Z".parse().unwrap(),
        "2026-10-05T12:34:56.123456002Z".parse().unwrap(),
        "2016-12-31T23:59:60.123456789Z".parse().unwrap(),
        "0000-01-01T00:00:00.000000001Z".parse().unwrap(),
        "-0001-01-01T00:00:00.000000001Z".parse().unwrap(),
        "+10000-01-01T00:00:00.000000001Z".parse().unwrap(),
        chrono::DateTime::<chrono::Utc>::MIN_UTC,
        chrono::DateTime::<chrono::Utc>::MAX_UTC,
        "2026-10-05T14:34:56.123456789+02:00".parse().unwrap(),
        "2026-10-05T07:04:56.123456789-05:30".parse().unwrap(),
    ];
    for timestamp in times {
        let receipt = ArtifactUploadReceipt {
            upload_id: ArtifactUploadId::new(),
            artifact_id: id,
            artifact_uri: id.plane_uri(),
            sha256: UploadSha256::parse("a".repeat(64)).unwrap(),
            byte_len: 1,
            mime_type: "text/plain".into(),
            filename: "timestamp.txt".into(),
            created_at: timestamp,
        };
        let receipt_wire = serde_json::to_value(&receipt).unwrap();
        assert!(receipt_validator.is_valid(&receipt_wire), "{timestamp}");
        assert_eq!(
            serde_json::from_value::<ArtifactUploadReceipt>(receipt_wire).unwrap(),
            receipt
        );
        let mut metadata = original.clone();
        metadata.created_at = timestamp;
        metadata.compliance.retention_expires_at = Some(timestamp);
        let wire = serde_json::to_value(&metadata).unwrap();
        assert!(metadata_validator.is_valid(&wire), "{timestamp}");
        assert_eq!(
            serde_json::from_value::<ArtifactMetadata>(wire).unwrap(),
            metadata
        );
    }
    // Every public timestamp location delegates to the same inline schema carrier.
    // Private SnapshotWire/ArtifactMetadataWire fields are covered by their public roots.
    let profiles = [
        (
            serde_json::to_value(schemars::schema_for!(Grant)).unwrap(),
            vec![("retentionExpiresAt", true)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(ArtifactShareLink)).unwrap(),
            vec![("expiresAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(ArtifactMetadataSnapshot)).unwrap(),
            vec![("metadataUpdatedAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(ArtifactReadGrant)).unwrap(),
            vec![("expiresAt", true)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(ArtifactAccessRequest)).unwrap(),
            vec![
                ("createdAt", false),
                ("updatedAt", false),
                ("decidedAt", true),
            ],
        ),
        (
            serde_json::to_value(schemars::schema_for!(IssueArtifactWriteCapabilityRequest))
                .unwrap(),
            vec![("expiresAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(IssuedArtifactWriteCapability)).unwrap(),
            vec![("expiresAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(IssueArtifactReadCapabilityRequest))
                .unwrap(),
            vec![("expiresAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(IssuedArtifactReadCapability)).unwrap(),
            vec![("expiresAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(ComplianceMetadata)).unwrap(),
            vec![("retentionExpiresAt", true)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(ArtifactMetadata)).unwrap(),
            vec![("createdAt", false)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(CreateArtifactShareLinkRequest)).unwrap(),
            vec![("expiresAt", true)],
        ),
        (
            serde_json::to_value(schemars::schema_for!(PutArtifactRequest)).unwrap(),
            vec![("retentionExpiresAt", true)],
        ),
        (receipt_schema, vec![("createdAt", false)]),
        (
            serde_json::to_value(schemars::schema_for!(ArtifactUploadSession)).unwrap(),
            vec![("createdAt", false), ("expiresAt", false)],
        ),
    ];
    for (schema, fields) in profiles {
        for (name, optional) in fields {
            let field = &schema["properties"][name];
            assert!(field.is_object(), "missing public {name} in {schema}");
            let validator = jsonschema::validator_for(field).unwrap();
            for timestamp in times {
                assert!(
                    validator.is_valid(&serde_json::to_value(timestamp).unwrap()),
                    "{name}: {timestamp}"
                );
            }
            assert_eq!(validator.is_valid(&Value::Null), optional, "{name} null");
            let required = schema["required"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item == name));
            assert_eq!(required, !optional, "{name} omission");
            for bad in [
                json!(true),
                json!(1),
                json!("2026-01-01T00:00:00"),
                json!("2026-01-01T24:00:00Z"),
                json!("invalid"),
            ] {
                assert!(!validator.is_valid(&bad), "{name} accepted {bad}");
            }
        }
    }
    for bad in [
        "2026-02-30T00:00:00Z",
        "-262144-01-01T00:00:00Z",
        "+262143-01-01T00:00:00Z",
    ] {
        let mut wire = serde_json::to_value(&original).unwrap();
        wire["createdAt"] = json!(bad);
        assert!(serde_json::from_value::<ArtifactMetadata>(wire).is_err());
    }
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["artifactId"] = json!(ArtifactId::new());
    assert!(serde_json::from_value::<ArtifactMetadata>(wire).is_err());
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["created_at"] = wire["createdAt"].clone();
    assert!(!metadata_validator.is_valid(&wire));
    assert!(serde_json::from_value::<ArtifactMetadata>(wire).is_err());
    let optional_schema = serde_json::to_value(schemars::schema_for!(PutArtifactRequest)).unwrap();
    let optional_validator = jsonschema::validator_for(&optional_schema).unwrap();
    for wire in [json!({}), json!({"retentionExpiresAt":null})] {
        assert!(optional_validator.is_valid(&wire));
        assert_eq!(
            serde_json::from_value::<PutArtifactRequest>(wire)
                .unwrap()
                .retention_expires_at,
            None
        );
    }
}
