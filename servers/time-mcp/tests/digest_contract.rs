use serde_json::json;
use veoveo_time_mcp::{AuthoritySourceDigest, CreateAcquisitionRequest, TimeSourceId};
use veoveo_types::Sha256Digest;

#[test]
fn bare_hex_wire_spelling_and_canonical_content_have_explicit_equality() {
    let lower = AuthoritySourceDigest::parse("ab".repeat(32)).unwrap();
    let upper = AuthoritySourceDigest::parse("AB".repeat(32)).unwrap();
    assert_ne!(
        lower, upper,
        "idempotency preserves the original request spelling"
    );
    assert_eq!(lower.canonical(), upper.canonical());
    assert_eq!(upper.as_hex(), "AB".repeat(32));
    assert_eq!(
        upper.canonical().as_str(),
        format!("sha256:{}", "ab".repeat(32))
    );
    for value in [lower, upper] {
        let wire = serde_json::to_value(&value).unwrap();
        assert_eq!(wire, json!(value.as_hex()));
        assert_eq!(
            serde_json::from_value::<AuthoritySourceDigest>(wire).unwrap(),
            value
        );
    }
    let canonical = Sha256Digest::from_hex("ab".repeat(32)).unwrap();
    assert_eq!(
        AuthoritySourceDigest::from_canonical(canonical.clone()).canonical(),
        &canonical
    );
}

#[test]
fn malformed_digests_fail_at_request_decoding_without_echoing_input() {
    for input in [
        String::new(),
        "a".repeat(63),
        "a".repeat(65),
        "g".repeat(64),
        format!("sha256:{}", "a".repeat(64)),
        "é".repeat(32),
        format!("{}\n", "a".repeat(63)),
        format!("{}\n", "a".repeat(64)),
        "PRIVATE_INVALID_DIGEST".into(),
    ] {
        assert!(AuthoritySourceDigest::parse(&input).is_err());
        let request = json!({"sourceId":"time-source-fixture",
            "expectedSourceDigestSha256":input, "idempotencyKey":"fixture"});
        let error = serde_json::from_value::<CreateAcquisitionRequest>(request).unwrap_err();
        assert!(!error.to_string().contains("PRIVATE_INVALID_DIGEST"));
    }
    let schema = serde_json::to_value(schemars::schema_for!(AuthoritySourceDigest)).unwrap();
    assert_eq!(schema["pattern"], "^[0-9a-fA-F]{64}$");
    assert_eq!(schema["minLength"], 64);
    assert_eq!(schema["maxLength"], 64);
    for value in [json!(null), json!(12), json!({}), json!([])] {
        assert!(serde_json::from_value::<AuthoritySourceDigest>(value).is_err());
    }
}

#[test]
fn acquisition_requests_keep_optional_digest_fields_and_bare_hex() {
    for digest in [
        None,
        Some(AuthoritySourceDigest::parse("aB".repeat(32)).unwrap()),
    ] {
        let request = veoveo_time_mcp::CreateAcquisitionRequestValue {
            source_id: TimeSourceId::parse("time-source-fixture").unwrap(),
            expected_source_digest_sha256: digest.clone(),
            idempotency_key: "fixture".into(),
        }
        .build()
        .unwrap();
        let wire = serde_json::to_value(&request).unwrap();
        assert_eq!(
            wire["expectedSourceDigestSha256"],
            json!(digest.as_ref().map(|d| d.as_hex()))
        );
        assert_eq!(
            serde_json::from_value::<CreateAcquisitionRequest>(wire).unwrap(),
            request
        );
    }
}
