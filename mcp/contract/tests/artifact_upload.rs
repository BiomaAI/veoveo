use std::{
    collections::BTreeSet,
    num::{NonZeroU32, NonZeroU64},
};
use veoveo_mcp_contract::{
    ArtifactUploadId, ArtifactUploadPolicy, ArtifactUploadRequestId, CompleteArtifactUpload,
    CreateArtifactUpload, GatewayAction, MAX_UPLOAD_BYTES, UploadErrorCode, UploadPartReceipt,
    UploadSha256,
};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
fn n(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}
fn policy() -> ArtifactUploadPolicy {
    ArtifactUploadPolicy {
        max_object_bytes: n(100 * GIB),
        tenant_quota_bytes: n(200 * GIB),
        max_active_uploads_per_tenant: NonZeroU32::new(8).unwrap(),
        part_bytes: n(16 * MIB),
        max_part_bytes: n(64 * MIB),
        max_parts: NonZeroU32::new(10_000).unwrap(),
        parallel_parts: NonZeroU32::new(4).unwrap(),
        max_inflight_bytes: n(128 * MIB),
        inactivity_seconds: n(24 * 3600),
        lifetime_seconds: n(7 * 24 * 3600),
        part_timeout_seconds: n(120),
        allowed_mime_types: BTreeSet::from(["application/octet-stream".into()]),
    }
}
fn descriptor(size: Option<u64>) -> CreateArtifactUpload {
    CreateArtifactUpload {
        filename: "observations.bin".into(),
        mime_type: "application/octet-stream".into(),
        byte_len: size,
        sha256: None,
    }
}
fn part(number: u32, byte_len: u64) -> UploadPartReceipt {
    UploadPartReceipt {
        part_number: NonZeroU32::new(number).unwrap(),
        byte_len,
        sha256: UploadSha256::parse("ab".repeat(32)).unwrap(),
    }
}

#[test]
fn large_layout_preserves_64_bit_offsets_and_bounds_unknown_producers() {
    let policy = policy();
    let layout = policy.admit(&descriptor(Some(10 * GIB))).unwrap();
    assert_eq!(layout.part_bytes.get(), 16 * MIB);
    assert_eq!(
        layout.part_offset(NonZeroU32::new(640).unwrap()).unwrap(),
        10 * GIB - 16 * MIB
    );
    assert_eq!(
        layout.part_offset(NonZeroU32::new(641).unwrap()),
        Err(UploadErrorCode::TooLarge)
    );
    let unknown = policy.admit(&descriptor(None)).unwrap();
    assert_eq!(unknown.max_total_bytes.get(), 100 * GIB);
    assert_eq!(
        unknown.part_offset(NonZeroU32::new(6401).unwrap()),
        Err(UploadErrorCode::TooLarge)
    );
}

#[test]
fn layout_grows_with_part_limit_and_reduces_parallelism_to_fit_memory() {
    let mut policy = policy();
    policy.max_parts = NonZeroU32::new(1600).unwrap();
    let layout = policy.admit(&descriptor(Some(100 * GIB))).unwrap();
    assert_eq!(layout.part_bytes.get(), 64 * MIB);
    assert_eq!(layout.parallel_parts.get(), 2);
    policy.max_parts = NonZeroU32::new(1599).unwrap();
    assert_eq!(policy.validate(), Err(UploadErrorCode::Malformed));
}

#[test]
fn explicit_policy_rejects_missing_limits_unsafe_counters_and_invalid_types() {
    assert!(serde_json::from_str::<ArtifactUploadPolicy>("{}").is_err());
    let mut policy = policy();
    policy.max_inflight_bytes = n(32 * MIB);
    assert_eq!(policy.validate(), Err(UploadErrorCode::Malformed));
    assert_eq!(
        descriptor(Some(MAX_UPLOAD_BYTES + 1)).validate(),
        Err(UploadErrorCode::Malformed)
    );
    assert_eq!(
        self::policy().admit(&descriptor(Some(101 * GIB))),
        Err(UploadErrorCode::TooLarge)
    );
    let mut request = descriptor(Some(1));
    request.mime_type = "text/html".into();
    assert_eq!(
        self::policy().admit(&request),
        Err(UploadErrorCode::UnsupportedType)
    );
}

#[test]
fn descriptors_cannot_assert_authority_or_inject_paths_and_headers() {
    let value = serde_json::json!({"filename":"test.bin", "mime_type":"application/octet-stream",
        "tenant":"another-tenant"});
    assert!(serde_json::from_value::<CreateArtifactUpload>(value).is_err());
    for filename in ["../test.bin", "a\\b", "a\r\nb", "", "..", " test.bin"] {
        let mut request = descriptor(Some(1));
        request.filename = filename.into();
        assert!(request.validate().is_err(), "accepted {filename:?}");
    }
    for mime in [
        "text/plain; charset=utf-8",
        "text/plain\r\nx: y",
        "TEXT/plain",
        "*/plain",
    ] {
        let mut request = descriptor(Some(1));
        request.mime_type = mime.into();
        assert!(request.validate().is_err(), "accepted {mime:?}");
    }
}

#[test]
fn manifest_rejects_holes_duplicates_short_middle_parts_and_wrong_totals() {
    let layout = policy().admit(&descriptor(Some(32 * MIB + 1))).unwrap();
    let manifest = CompleteArtifactUpload {
        byte_len: 32 * MIB + 1,
        part_count: NonZeroU32::new(3).unwrap(),
        sha256: None,
    };
    let valid = vec![part(1, 16 * MIB), part(2, 16 * MIB), part(3, 1)];
    assert!(layout.validate_manifest(&manifest, &valid).is_ok());
    for invalid in [
        vec![part(1, 16 * MIB), part(3, 16 * MIB), part(4, 1)],
        vec![part(1, 16 * MIB), part(1, 16 * MIB), part(3, 1)],
        vec![part(1, 16 * MIB - 1), part(2, 16 * MIB), part(3, 2)],
        vec![part(1, 16 * MIB), part(2, 16 * MIB), part(3, 0)],
        valid[..2].to_vec(),
    ] {
        assert_eq!(
            layout.validate_manifest(&manifest, &invalid),
            Err(UploadErrorCode::Conflict)
        );
    }
}

#[test]
fn empty_files_have_one_empty_final_part() {
    let layout = policy().admit(&descriptor(Some(0))).unwrap();
    let manifest = CompleteArtifactUpload {
        byte_len: 0,
        part_count: NonZeroU32::new(1).unwrap(),
        sha256: None,
    };
    assert!(layout.validate_manifest(&manifest, &[part(1, 0)]).is_ok());
}

#[test]
fn upload_ids_are_uuidv7_and_digest_schema_rejects_composite_hashes() {
    assert!(ArtifactUploadId::parse(ArtifactUploadId::new().to_string()).is_ok());
    assert!(ArtifactUploadRequestId::parse("00000000-0000-4000-8000-000000000000").is_err());
    assert!(UploadSha256::parse(format!("{}-2", "ab".repeat(32))).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(UploadSha256)).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&serde_json::json!("ab".repeat(32))));
    assert!(!validator.is_valid(&serde_json::json!("AB".repeat(32))));
    let schema = serde_json::to_value(schemars::schema_for!(CreateArtifactUpload)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(GatewayAction::ArtifactUpload.mcp_method(), None);
    assert_eq!(
        serde_json::to_value(GatewayAction::ArtifactUpload).unwrap(),
        "artifact_upload"
    );
}
