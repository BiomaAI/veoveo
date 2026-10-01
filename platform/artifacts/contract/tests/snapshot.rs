use serde_json::json;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadataSnapshot, Grant};

#[test]
fn snapshot_admits_complete_access_state_and_rejects_cross_record_or_transfer_data() {
    let id = ArtifactId::new();
    let wire = json!({
        "metadata": {
            "artifact_id": id,
            "artifact_uri": id.plane_uri(),
            "byte_len": 3,
            "created_at": "2026-10-01T00:00:00Z",
            "compliance": {
                "tenant_id": "acme",
                "owner": {"kind":"principal", "id":"alice"},
                "work_context": "mission",
                "provenance": {"producer":"alice", "invocation_mode":"direct", "initiator":"alice", "policy_revision":"r1"}
            }
        },
        "read_grants": [{"subject":{"kind":"principal","id":"alice"}}],
        "metadata_updated_at":"2026-10-01T00:00:00Z"
    });
    let snapshot: ArtifactMetadataSnapshot = serde_json::from_value(wire.clone()).unwrap();
    let output = serde_json::to_value(&snapshot).unwrap();
    let schema = serde_json::to_value(schemars::schema_for!(ArtifactMetadataSnapshot)).unwrap();
    assert!(
        jsonschema::validator_for(&schema)
            .unwrap()
            .is_valid(&output)
    );
    assert_eq!(
        serde_json::from_value::<ArtifactMetadataSnapshot>(output).unwrap(),
        snapshot
    );
    for (path, value) in [
        ("/metadata/compliance/owner", json!(null)),
        ("/metadata/compliance/tenant_id", json!(null)),
        ("/metadata/compliance/work_context", json!(null)),
        ("/metadata/compliance/provenance", json!(null)),
        ("/metadata_updated_at", json!("2026-09-30T00:00:00Z")),
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<ArtifactMetadataSnapshot>(invalid).is_err(),
            "{path}"
        );
    }
    let mut invalid = wire.clone();
    invalid["metadata"]["download_url"] = json!("https://example.test/download");
    assert!(serde_json::from_value::<ArtifactMetadataSnapshot>(invalid).is_err());
    let mut invalid = wire.clone();
    invalid["read_grants"]
        .as_array_mut()
        .unwrap()
        .push(wire["read_grants"][0].clone());
    assert!(serde_json::from_value::<ArtifactMetadataSnapshot>(invalid).is_err());
    let grant: Grant = serde_json::from_value(json!({
        "artifact":id, "subject":{"kind":"principal","id":"alice"}, "level":"admin", "tenant":"acme"
    }))
    .unwrap();
    let constructed = ArtifactMetadataSnapshot::new(
        snapshot.metadata().clone(),
        vec![grant.clone()],
        snapshot.metadata_updated_at(),
    )
    .unwrap();
    assert_eq!(constructed, snapshot);
    let mut foreign = grant.clone();
    foreign.tenant = "other".parse().unwrap();
    assert!(
        ArtifactMetadataSnapshot::new(
            snapshot.metadata().clone(),
            vec![foreign],
            snapshot.metadata_updated_at()
        )
        .is_err()
    );
    let mut wrong_id = grant;
    wrong_id.artifact = ArtifactId::new();
    assert!(
        ArtifactMetadataSnapshot::new(
            snapshot.metadata().clone(),
            vec![wrong_id],
            snapshot.metadata_updated_at()
        )
        .is_err()
    );
    assert!(
        serde_json::to_value(snapshot).unwrap()["read_grants"][0]
            .get("level")
            .is_none()
    );
    let mut invalid = wire;
    invalid["unknown"] = json!(true);
    assert!(serde_json::from_value::<ArtifactMetadataSnapshot>(invalid).is_err());
}
