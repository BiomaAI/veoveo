//! Content identity shared by disposable gateway activation and GitOps installations.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use veoveo_extension_contract::ArtifactDigest;

/// Hashes every public ConfigMap data key and its exact UTF-8 value using the
/// length-prefixed `veoveo.io/gateway-activation/v1` encoding.
pub fn gateway_bundle_digest(data: &BTreeMap<String, String>) -> Result<ArtifactDigest> {
    ensure!(!data.is_empty(), "gateway bundle cannot be empty");
    let mut hasher = Sha256::new();
    hasher.update(b"veoveo.io/gateway-activation/v1\0");
    for (key, value) in data {
        ensure!(
            !key.is_empty()
                && key.len() <= 253
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')),
            "gateway bundle requires explicit ConfigMap data keys"
        );
        hasher.update(u64::try_from(key.len())?.to_be_bytes());
        hasher.update(key.as_bytes());
        hasher.update(u64::try_from(value.len())?.to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(ArtifactDigest::new(format!("sha256:{hex}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_name_and_exact_value_participates_in_bundle_identity() {
        let original = BTreeMap::from([
            ("gateway.json".into(), "{}\n".into()),
            ("trust.json".into(), "{\"keys\":[]}\n".into()),
        ]);
        let expected = gateway_bundle_digest(&original).unwrap();
        assert_eq!(
            expected.as_str(),
            "sha256:4e08cc43e46b795579ead8233a9fafdb5ecd51b42eb638b77f7dd9885f4c5df6"
        );
        for (key, value) in [("gateway.json", "{}"), ("trust.json", "{\"keys\":[]}")] {
            let mut changed = original.clone();
            changed.insert(key.into(), value.into());
            assert_ne!(gateway_bundle_digest(&changed).unwrap(), expected);
        }
        let mut renamed = original.clone();
        let value = renamed.remove("trust.json").unwrap();
        renamed.insert("other-trust.json".into(), value);
        assert_ne!(gateway_bundle_digest(&renamed).unwrap(), expected);
        let reordered = original.into_iter().rev().collect();
        assert_eq!(gateway_bundle_digest(&reordered).unwrap(), expected);
        assert!(gateway_bundle_digest(&BTreeMap::new()).is_err());
    }

    #[test]
    fn length_framing_prevents_key_value_boundary_collisions() {
        let left = BTreeMap::from([("a".into(), "bc".into())]);
        let right = BTreeMap::from([("ab".into(), "c".into())]);
        assert_ne!(
            gateway_bundle_digest(&left).unwrap(),
            gateway_bundle_digest(&right).unwrap()
        );
        assert!(
            gateway_bundle_digest(&BTreeMap::from([("../key".into(), "value".into())])).is_err()
        );
    }
}
