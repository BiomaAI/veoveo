use veoveo_types::Sha256Digest;

#[test]
fn fixed_size_digest_bytes_preserve_leading_zeroes_and_all_hex_digits() {
    let bytes = std::array::from_fn(|index| index as u8);
    let digest = Sha256Digest::from_bytes(bytes);
    assert_eq!(
        digest.hex(),
        "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
    );
    assert_eq!(Sha256Digest::parse(digest.as_str()).unwrap(), digest);
    assert_eq!(
        serde_json::from_value::<Sha256Digest>(serde_json::to_value(&digest).unwrap()).unwrap(),
        digest
    );
    assert_eq!(Sha256Digest::from_bytes([255; 32]).hex(), "f".repeat(64));
}

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

#[test]
fn bare_hex_fields_keep_their_declared_wire_profile() {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    struct Wire {
        #[serde(with = "veoveo_types::sha256_hex")]
        bare: veoveo_types::Sha256Digest,
        #[serde(default, with = "veoveo_types::sha256_hex::optional")]
        optional: Option<veoveo_types::Sha256Digest>,
        canonical: veoveo_types::Sha256Digest,
    }
    let digest = veoveo_types::Sha256Digest::from_bytes([0xab; 32]);
    let wire = Wire {
        bare: digest.clone(),
        optional: Some(digest.clone()),
        canonical: digest.clone(),
    };
    let value = serde_json::to_value(&wire).unwrap();
    assert_eq!(value["bare"], digest.hex());
    assert_eq!(value["optional"], digest.hex());
    assert_eq!(value["canonical"], digest.as_str());
    let decoded: Wire = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded.bare, digest);
    for bad in [
        serde_json::json!(digest.as_str()),
        serde_json::json!("AB".repeat(32)),
        serde_json::json!("ab".repeat(31)),
    ] {
        for field in ["bare", "optional"] {
            let mut invalid = value.clone();
            invalid[field] = bad.clone();
            assert!(serde_json::from_value::<Wire>(invalid).is_err());
        }
    }
    let mut absent = value;
    absent.as_object_mut().unwrap().remove("optional");
    let decoded: Wire = serde_json::from_value(absent).unwrap();
    assert!(decoded.optional.is_none());
    assert!(serde_json::to_value(decoded).unwrap()["optional"].is_null());
}
