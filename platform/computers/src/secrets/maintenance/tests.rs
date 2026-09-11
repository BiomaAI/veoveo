use super::*;
use crate::secrets::{CommandBinding, ComputerSealingKey};
use serde_json::json;

fn keys(active: u128, retained: &[u128]) -> ComputerKeyRing {
    ComputerKeyRing::new(
        Uuid::from_u128(active),
        retained
            .iter()
            .map(|n| {
                ComputerSealingKey::new(Uuid::from_u128(*n), Zeroizing::new([*n as u8; 32]))
                    .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn binding() -> MaintenanceBinding {
    MaintenanceBinding {
        operation_id: Uuid::now_v7(),
        request_id: Uuid::from_u128(1),
        computer_id: Uuid::from_u128(2),
        provider_instance_id: Uuid::from_u128(3),
        owner_key: "a".repeat(64),
        actor_key: "b".repeat(64),
        source_instance_id: Uuid::from_u128(2),
        target_instance_id: Uuid::from_u128(4),
        source_template_fingerprint: "c".repeat(64),
        target_template_fingerprint: "d".repeat(64),
        source_resource_id: "native-resource".into(),
        source_process_id: "native-process".into(),
        required_labels: Default::default(),
    }
}
fn checkpoint() -> MaintenanceCheckpoint {
    MaintenanceCheckpoint::new(Zeroizing::new(
        b"private-provider-configuration\0\xff".to_vec(),
    ))
    .unwrap()
}

#[test]
fn protected_checkpoint_survives_rotation_without_plaintext_or_unkeyed_fingerprint() {
    let ring = keys(1, &[1]);
    let binding = binding();
    let original = checkpoint();
    let first = ring.seal_maintenance(&binding, &original).unwrap();
    let second = ring.seal_maintenance(&binding, &original).unwrap();
    let a = serde_json::to_value(&first).unwrap();
    let b = serde_json::to_value(&second).unwrap();
    assert_ne!(a["ciphertext"], b["ciphertext"]);
    assert_ne!(a["nonce"], b["nonce"]);
    assert_eq!(a["fingerprint"], b["fingerprint"]);
    assert!(!a.to_string().contains("private-provider-configuration"));
    let restored: SealedMaintenanceCheckpoint = serde_json::from_value(a).unwrap();
    let rotated = keys(2, &[1, 2]);
    assert!(
        rotated
            .open_maintenance(&binding, &restored)
            .unwrap()
            .bytes()
            == original.bytes()
    );
    let new = rotated.seal_maintenance(&binding, &original).unwrap();
    assert_ne!(
        serde_json::to_value(&new).unwrap()["fingerprint"],
        b["fingerprint"]
    );
    assert!(keys(2, &[2]).open_maintenance(&binding, &restored).is_err());
    assert!(ring.open_maintenance(&binding, &new).is_err());
}

#[test]
fn every_maintenance_identity_and_label_is_authenticated() {
    let ring = keys(1, &[1]);
    let binding = binding();
    let sealed = ring.seal_maintenance(&binding, &checkpoint()).unwrap();
    let value = serde_json::to_value(&binding).unwrap();
    for name in value.as_object().unwrap().keys() {
        let mut altered = value.clone();
        altered[name] = match name.as_str() {
            "owner_key"
            | "actor_key"
            | "source_template_fingerprint"
            | "target_template_fingerprint" => json!("e".repeat(64)),
            "source_resource_id" | "source_process_id" => json!("different-native-identity"),
            "required_labels" => json!(["retained-home"]),
            _ => json!(Uuid::now_v7()),
        };
        let altered: MaintenanceBinding = serde_json::from_value(altered).unwrap();
        assert!(ring.open_maintenance(&altered, &sealed).is_err(), "{name}");
    }
    for id in [Uuid::nil(), binding.source_instance_id, binding.computer_id] {
        let mut invalid = binding.clone();
        invalid.target_instance_id = id;
        assert!(ring.seal_maintenance(&invalid, &checkpoint()).is_err());
    }
}

#[test]
fn maintenance_envelopes_reject_cross_purpose_tampering_and_size_overflow() {
    let ring = keys(1, &[1]);
    let binding = binding();
    let sealed = ring.seal_maintenance(&binding, &checkpoint()).unwrap();
    let encoded = serde_json::to_value(&sealed).unwrap();
    for field in ["nonce", "ciphertext", "fingerprint"] {
        let mut damaged = encoded.clone();
        let mut bytes = damaged[field].as_str().unwrap().as_bytes().to_vec();
        bytes[0] = if bytes[0] == b'A' { b'B' } else { b'A' };
        damaged[field] = json!(String::from_utf8(bytes).unwrap());
        let damaged = serde_json::from_value(damaged).unwrap();
        assert!(ring.open_maintenance(&binding, &damaged).is_err());
    }
    let command_binding = CommandBinding {
        execution_id: binding.operation_id,
        request_id: binding.request_id,
        computer_id: binding.computer_id,
        grant_id: Uuid::from_u128(9),
        provider_instance_id: binding.provider_instance_id,
        owner_key: binding.owner_key.clone(),
        actor_key: binding.actor_key.clone(),
        template_fingerprint: binding.source_template_fingerprint.clone(),
        resource_id: binding.source_resource_id.clone(),
        process_id: binding.source_process_id.clone(),
        required_output_labels: binding.required_labels.clone(),
        replacement_instance_id: None,
    };
    let wrong_purpose: SealedCommand = serde_json::from_value(encoded).unwrap();
    assert!(ring.open(&command_binding, &wrong_purpose).is_err());
    let sealed_as_command = ring
        .seal_bytes(&command_binding, checkpoint().bytes(), SecretKind::Command)
        .unwrap();
    assert!(
        ring.open_maintenance(&binding, &SealedMaintenanceCheckpoint(sealed_as_command))
            .is_err()
    );
    for size in [0, 11, MAX_CHECKPOINT_BYTES + 1] {
        assert!(MaintenanceCheckpoint::new(Zeroizing::new(vec![0; size])).is_err());
    }
    let maximum =
        MaintenanceCheckpoint::new(Zeroizing::new(vec![7; MAX_CHECKPOINT_BYTES])).unwrap();
    let sealed = ring.seal_maintenance(&binding, &maximum).unwrap();
    assert!(ring.open_maintenance(&binding, &sealed).unwrap().bytes() == maximum.bytes());
}
