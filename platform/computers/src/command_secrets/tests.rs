use super::*;
use crate::{ComputerError, api::AutomationExecutionLimits};
use serde_json::json;
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_computer_execution::ExecutionRequest;
use zeroize::Zeroizing;

fn binding() -> CommandBinding {
    CommandBinding {
        execution_id: Uuid::from_u128(1),
        request_id: Uuid::from_u128(2),
        computer_id: Uuid::from_u128(3),
        grant_id: Uuid::from_u128(4),
        provider_instance_id: Uuid::from_u128(5),
        owner_key: "a".repeat(64),
        actor_key: "b".repeat(64),
        template_fingerprint: "c".repeat(64),
        resource_id: "native-resource".into(),
        process_id: "native-process".into(),
    }
}
fn key(n: u128) -> CommandSealingKey {
    CommandSealingKey::new(Uuid::from_u128(n), Zeroizing::new([n as u8; 32])).unwrap()
}
fn ring(active: u128, keys: &[u128]) -> CommandKeyRing {
    CommandKeyRing::new(
        Uuid::from_u128(active),
        keys.iter().map(|&n| key(n)).collect(),
    )
    .unwrap()
}
fn command(value: &str, seconds: u32, bytes: u32) -> CommandPayload {
    CommandPayload::new(
        ExecutionRequest::new(
            vec!["/bin/printf".into(), value.into(), "".into()],
            "work".into(),
            BTreeMap::from([("TOKEN".into(), "private-environment-fixture".into())]),
            b"private-stdin-fixture\0\xff".to_vec(),
        )
        .unwrap(),
        AutomationExecutionLimits {
            maximum_seconds: seconds,
            maximum_output_bytes: bytes,
            on_interruption: veoveo_computers_contract::AutomationInterruption::StopComputer,
        },
    )
    .unwrap()
}

#[test]
fn queued_payload_is_randomized_recoverable_and_has_only_keyed_fingerprints() {
    let keys = ring(1, &[1]);
    let binding = binding();
    let original = command("private-argument-fixture", 30, 1024);
    let first = keys.seal(&binding, &original).unwrap();
    let second = keys.seal(&binding, &original).unwrap();
    let a = serde_json::to_value(&first).unwrap();
    let b = serde_json::to_value(&second).unwrap();
    assert_ne!(a["ciphertext"], b["ciphertext"]);
    assert_ne!(a["nonce"], b["nonce"]);
    assert_eq!(a["fingerprint"], b["fingerprint"]);
    for value in [
        "private-argument-fixture",
        "private-environment-fixture",
        "private-stdin-fixture",
        "/bin/printf",
    ] {
        assert!(!a.to_string().contains(value));
    }
    let restored = keys.open(&binding, &first).unwrap();
    assert!(
        restored.request().encode().unwrap().as_slice()
            == original.request().encode().unwrap().as_slice()
    );
    assert_eq!(restored.limits(), original.limits());
    assert!(keys.matches(&binding, &first, &original).unwrap());
    for altered in [
        command("changed", 30, 1024),
        command("private-argument-fixture", 31, 1024),
        command("private-argument-fixture", 30, 1025),
    ] {
        assert!(!keys.matches(&binding, &first, &altered).unwrap());
    }
}

#[test]
fn every_identity_is_authenticated_and_cannot_be_rebound() {
    let keys = ring(1, &[1]);
    let original = binding();
    let command = command("private", 30, 1024);
    let sealed = keys.seal(&original, &command).unwrap();
    let value = serde_json::to_value(&original).unwrap();
    for (name, _) in value.as_object().unwrap() {
        let mut changed = value.clone();
        changed[name] = match name.as_str() {
            "owner_key" | "actor_key" | "template_fingerprint" => json!("d".repeat(64)),
            "resource_id" | "process_id" => json!("different-native-run"),
            _ => json!(Uuid::from_u128(99)),
        };
        let altered: CommandBinding = serde_json::from_value(changed).unwrap();
        assert!(keys.open(&altered, &sealed).is_err(), "{name}");
        assert!(keys.matches(&altered, &sealed, &command).is_err(), "{name}");
    }
}

#[test]
fn rotation_reads_old_work_and_compares_retries_with_the_original_key() {
    let first = ring(1, &[1]);
    let rotated = ring(2, &[1, 2]);
    let retired = ring(2, &[2]);
    let binding = binding();
    let command = command("private", 30, 1024);
    let old = first.seal(&binding, &command).unwrap();
    assert!(rotated.open(&binding, &old).is_ok());
    assert!(rotated.matches(&binding, &old, &command).unwrap());
    assert!(retired.open(&binding, &old).is_err());
    assert!(retired.matches(&binding, &old, &command).is_err());
    let new = rotated.seal(&binding, &command).unwrap();
    assert_eq!(
        serde_json::to_value(&new).unwrap()["key_id"],
        json!(Uuid::from_u128(2))
    );
    assert!(retired.open(&binding, &new).is_ok());
    assert!(first.open(&binding, &new).is_err());
    assert_ne!(
        serde_json::to_value(&old).unwrap()["fingerprint"],
        serde_json::to_value(&new).unwrap()["fingerprint"]
    );
}

#[test]
fn corruption_and_unknown_versions_never_resolve_a_retry() {
    let keys = ring(1, &[1, 2]);
    let binding = binding();
    let command = command("private", 30, 1024);
    let sealed = keys.seal(&binding, &command).unwrap();
    let value = serde_json::to_value(&sealed).unwrap();
    for (name, replacement) in [
        ("version", json!(2)),
        ("key_id", json!(Uuid::from_u128(2))),
        ("nonce", json!("A".repeat(32))),
        ("nonce", json!("invalid")),
        ("fingerprint", json!("A".repeat(44))),
        ("fingerprint", json!("!".repeat(44))),
        ("ciphertext", json!("A".repeat(40))),
        ("ciphertext", json!("!".repeat(40))),
        ("ciphertext", json!("A".repeat(3 * 1024 * 1024))),
    ] {
        let mut altered = value.clone();
        altered[name] = replacement;
        let altered: SealedCommand = serde_json::from_value(altered).unwrap();
        assert!(keys.open(&binding, &altered).is_err(), "{name}");
        assert!(
            keys.matches(&binding, &altered, &command).is_err(),
            "{name}"
        );
    }
    let mut unknown = value;
    unknown["authority"] = json!("forged");
    assert!(serde_json::from_value::<SealedCommand>(unknown).is_err());
}

#[test]
fn binding_shape_keys_and_plaintext_framing_fail_closed() {
    assert!(CommandSealingKey::new(Uuid::nil(), Zeroizing::new([0; 32])).is_err());
    assert!(CommandKeyRing::new(Uuid::from_u128(1), vec![]).is_err());
    assert!(CommandKeyRing::new(Uuid::from_u128(1), vec![key(1), key(1)]).is_err());
    assert!(CommandKeyRing::new(Uuid::from_u128(1), vec![key(2)]).is_err());
    assert!(CommandKeyRing::new(Uuid::from_u128(1), (1..=5).map(key).collect()).is_err());
    let keys = ring(1, &[1]);
    let command = command("private", 30, 1024);
    let mut binding = binding();
    binding.actor_key = "forged".into();
    assert!(matches!(
        keys.seal(&binding, &command),
        Err(ComputerError::InvalidInput)
    ));
    let mut bytes = command.encode().unwrap();
    bytes.push(1);
    assert!(CommandPayload::decode(&bytes).is_err());
    bytes.pop();
    bytes[0..4].copy_from_slice(&0u32.to_be_bytes());
    assert!(CommandPayload::decode(&bytes).is_err());
    for length in 0..12 {
        assert!(CommandPayload::decode(&bytes[..length]).is_err());
    }
}

#[test]
fn identical_raw_keys_under_different_ids_cannot_substitute_an_envelope() {
    let keys = CommandKeyRing::new(
        Uuid::from_u128(1),
        vec![
            key(1),
            CommandSealingKey::new(Uuid::from_u128(2), Zeroizing::new([1; 32])).unwrap(),
        ],
    )
    .unwrap();
    let binding = binding();
    let sealed = keys.seal(&binding, &command("private", 30, 1024)).unwrap();
    let mut value = serde_json::to_value(sealed).unwrap();
    value["key_id"] = json!(Uuid::from_u128(2));
    let altered: SealedCommand = serde_json::from_value(value).unwrap();
    assert!(keys.open(&binding, &altered).is_err());
}
