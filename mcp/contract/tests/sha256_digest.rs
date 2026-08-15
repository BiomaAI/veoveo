use veoveo_mcp_contract::Sha256Digest;

#[test]
fn sha256_digest_accepts_only_the_canonical_prefixed_lowercase_shape() {
    let hex = "a".repeat(64);
    let digest = Sha256Digest::parse(format!("sha256:{hex}")).unwrap();

    assert_eq!(digest.as_str(), format!("sha256:{hex}"));
    assert_eq!(digest.hex(), hex);
    for invalid in [
        "a".repeat(64),
        format!("sha256:{}", "A".repeat(64)),
        format!("sha256:{}", "a".repeat(63)),
        format!("sha256:{}", "a".repeat(65)),
    ] {
        assert!(Sha256Digest::parse(invalid).is_err());
    }
}

#[test]
fn sha256_digest_schema_matches_runtime_validation() {
    let schema = serde_json::to_value(schemars::schema_for!(Sha256Digest)).unwrap();

    assert_eq!(schema["type"], "string");
    assert_eq!(schema["pattern"], "^sha256:[0-9a-f]{64}$");
}
