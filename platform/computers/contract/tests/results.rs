use serde_json::json;
use veoveo_computers_contract::*;
use veoveo_types::{Sha256Digest, TaskId};

#[test]
fn lifecycle_and_maintenance_results_bind_the_computer_address() {
    let computer = ComputerId::new();
    let result = LifecycleResult::new(computer, TaskId::new(), Action::Create);
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        serde_json::from_value::<LifecycleResult>(value.clone()).unwrap(),
        result
    );
    for field in ["computerId", "result_uri", "action"] {
        let mut changed = value.clone();
        changed[field] = match field {
            "computerId" => json!(ComputerId::new()),
            "result_uri" => json!(null),
            _ => json!("stop"),
        };
        assert!(
            serde_json::from_value::<LifecycleResult>(changed).is_err(),
            "{field}"
        );
    }
    let maintenance = MaintenanceResult::new(computer, TaskId::new(), "dev".parse().unwrap());
    let mut value = serde_json::to_value(&maintenance).unwrap();
    assert_eq!(
        serde_json::from_value::<MaintenanceResult>(value.clone()).unwrap(),
        maintenance
    );
    value["computerId"] = json!(ComputerId::new());
    assert!(serde_json::from_value::<MaintenanceResult>(value).is_err());
}

#[test]
fn command_results_reject_mismatched_products_unknown_exit_and_impossible_outputs() {
    let stdout = ExecutionOutput {
        artifact_id: ArtifactId::new(),
        byte_count: 0,
    };
    let stderr = ExecutionOutput {
        artifact_id: ArtifactId::new(),
        byte_count: 3,
    };
    let computer = ComputerId::new();
    let execution = ExecutionId::new();
    let result = ExecutionResult::new(computer, execution, 0, stdout, stderr).unwrap();
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(
        serde_json::from_value::<ExecutionResult>(value.clone()).unwrap(),
        result
    );
    for field in ["executionId", "exitCode", "stdout", "stderr"] {
        let mut changed = value.clone();
        changed[field] = match field {
            "executionId" => json!(ExecutionId::new()),
            "exitCode" => json!(124),
            "stdout" => {
                json!({"artifactId": stdout.artifact_id, "byteCount": MAX_TRANSFER_BYTES + 1})
            }
            _ => json!(stdout),
        };
        assert!(
            serde_json::from_value::<ExecutionResult>(changed).is_err(),
            "{field}"
        );
    }
    assert!(ExecutionResult::new(computer, execution, 124, stdout, stderr).is_err());
    assert!(ExecutionResult::new(computer, execution, 0, stdout, stdout).is_err());
}

#[test]
fn file_results_bind_the_transfer_and_checked_artifact_integrity() {
    let result = FileTransferResult::new(
        ComputerId::new(),
        FileTransferId::new(),
        FileTransferDirection::Export,
        ArtifactId::new(),
        4,
        Sha256Digest::from_hex("a".repeat(64)).unwrap(),
    )
    .unwrap();
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["sha256"], "a".repeat(64));
    assert_eq!(
        serde_json::from_value::<FileTransferResult>(value.clone()).unwrap(),
        result
    );
    for field in ["transferId", "artifactId", "bytes", "sha256"] {
        let mut changed = value.clone();
        changed[field] = match field {
            "transferId" => json!(FileTransferId::new()),
            "artifactId" => json!(uuid::Uuid::nil()),
            "bytes" => json!(MAX_TRANSFER_BYTES + 1),
            _ => json!("not-a-digest"),
        };
        assert!(
            serde_json::from_value::<FileTransferResult>(changed).is_err(),
            "{field}"
        );
    }
}
