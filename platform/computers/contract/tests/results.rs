use serde_json::json;
use veoveo_computers_contract::TerminalVersion;
use veoveo_computers_contract::*;
use veoveo_types::{Sha256Digest, TaskId};

// Known result trees are controlled; intentional corruption stays at the raw
// receiver boundary and does not bypass checked producer construction.
fn result_wire_refuses_retired_members<T: serde::Serialize + serde::de::DeserializeOwned>(
    result: &T,
) {
    let current = serde_json::to_value(result).unwrap();
    let received: T = serde_json::from_slice(&serde_json::to_vec(&current).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(received).unwrap(), current);
    fn members(
        value: &serde_json::Value,
        path: String,
        output: &mut Vec<(String, String, String)>,
    ) {
        if let Some(fields) = value.as_object() {
            for (key, child) in fields {
                let retired = key
                    .chars()
                    .flat_map(|c| {
                        if c.is_ascii_uppercase() {
                            vec!['_', c.to_ascii_lowercase()]
                        } else {
                            vec![c]
                        }
                    })
                    .collect::<String>();
                if retired != *key {
                    output.push((path.clone(), key.clone(), retired));
                }
                members(child, format!("{path}/{key}"), output);
            }
        }
    }
    let mut fields = Vec::new();
    members(&current, String::new(), &mut fields);
    assert!(!fields.is_empty());
    for (path, key, retired) in fields {
        for mixed in [false, true] {
            let mut bad = current.clone();
            let node = bad.pointer_mut(&path).unwrap().as_object_mut().unwrap();
            let value = node.get(&key).unwrap().clone();
            if !mixed {
                node.remove(&key);
            }
            node.insert(retired.clone(), value);
            assert!(
                serde_json::from_value::<T>(bad.clone()).is_err(),
                "{path}/{key} mixed={mixed}"
            );
            assert!(
                serde_json::from_slice::<T>(&serde_json::to_vec(&bad).unwrap()).is_err(),
                "JSON {path}/{key} mixed={mixed}"
            );
        }
    }
}

#[test]
fn lifecycle_and_maintenance_results_bind_the_computer_address() {
    let computer = ComputerId::new();
    let result = LifecycleResult::new(computer, TaskId::new(), Action::Create);
    result_wire_refuses_retired_members(&result);
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(
        serde_json::from_value::<LifecycleResult>(value.clone()).unwrap(),
        result
    );
    let mut obsolete = value.clone();
    obsolete["result_uri"] = obsolete
        .as_object_mut()
        .unwrap()
        .remove("resultUri")
        .unwrap();
    assert!(serde_json::from_value::<LifecycleResult>(obsolete).is_err());
    for field in ["computerId", "resultUri", "action"] {
        let mut changed = value.clone();
        changed[field] = match field {
            "computerId" => json!(ComputerId::new()),
            "resultUri" => json!(null),
            _ => json!("stop"),
        };
        assert!(
            serde_json::from_value::<LifecycleResult>(changed).is_err(),
            "{field}"
        );
    }
    let maintenance = MaintenanceResult::new(computer, TaskId::new(), "dev".parse().unwrap());
    result_wire_refuses_retired_members(&maintenance);
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
    result_wire_refuses_retired_members(&result);
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
    result_wire_refuses_retired_members(&result);
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

#[test]
fn controlled_inputs_refuse_retired_and_mixed_members_through_current_owner_decoders() {
    fn decode(tool: &str, value: serde_json::Value) -> bool {
        match tool {
            "transfer_file" => serde_json::from_value::<TransferFileInput>(value).is_ok(),
            "grant_automation" => {
                serde_json::from_value::<IssueAutomationGrantInput>(value).is_ok()
            }
            _ => panic!("unexpected Computers input fixture tool"),
        }
    }
    fn decode_bytes(tool: &str, bytes: &[u8]) -> bool {
        match tool {
            "transfer_file" => serde_json::from_slice::<TransferFileInput>(bytes).is_ok(),
            "grant_automation" => {
                serde_json::from_slice::<IssueAutomationGrantInput>(bytes).is_ok()
            }
            _ => panic!("unexpected Computers input fixture tool"),
        }
    }
    fn objects(
        value: &serde_json::Value,
        path: &mut Vec<String>,
        found: &mut Vec<(Vec<String>, String, String)>,
    ) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, child) in fields {
                    let retired = key
                        .chars()
                        .flat_map(|c| {
                            if c.is_ascii_uppercase() {
                                vec!['_', c.to_ascii_lowercase()]
                            } else {
                                vec![c]
                            }
                        })
                        .collect::<String>();
                    if retired != *key {
                        found.push((path.clone(), key.clone(), retired));
                    }
                    path.push(key.clone());
                    objects(child, path, found);
                    path.pop();
                }
            }
            serde_json::Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    path.push(index.to_string());
                    objects(child, path, found);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let cases: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../servers/computers-mcp/testdata/controlled-inputs.json"
    ))
    .unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 7);
    let mut changed = 0;
    for case in cases {
        let tool = case["tool"].as_str().unwrap();
        let current = case["arguments"].clone();
        assert!(decode(tool, current.clone()), "current {tool}");
        let mut members = Vec::new();
        objects(&current, &mut Vec::new(), &mut members);
        for (path, key, retired) in members {
            for mixed in [false, true] {
                let mut bad = current.clone();
                let mut node = &mut bad;
                for part in &path {
                    node = if node.is_array() {
                        &mut node[part.parse::<usize>().unwrap()]
                    } else {
                        &mut node[part]
                    };
                }
                let fields = node.as_object_mut().unwrap();
                let value = fields.get(&key).unwrap().clone();
                if !mixed {
                    fields.remove(&key);
                }
                fields.insert(retired.clone(), value);
                assert!(
                    !decode(tool, bad.clone()),
                    "{tool} {path:?} {key} mixed={mixed}"
                );
                assert!(
                    !decode_bytes(tool, &serde_json::to_vec(&bad).unwrap()),
                    "JSON {tool} {path:?} {key} mixed={mixed}"
                );
                changed += 1;
            }
        }
    }
    assert!(changed > 50);
}

#[test]
fn terminal_current_marker_is_admitted_by_both_owner_receivers() {
    let attach = TerminalAttach {
        version: TERMINAL_VERSION,
        kind: TerminalAttachKind::Attach,
        computer_id: ComputerId::new(),
        token: TerminalToken::new("one-use-fixture".into()),
        cols: 80,
        rows: 24,
    };
    result_wire_refuses_retired_members(&attach);
    let attach = serde_json::to_value(attach).unwrap();
    let ready = TerminalServerControl::Ready(TerminalReady {
        version: TERMINAL_VERSION,
        kind: TerminalReadyKind::Ready,
        expires_at: chrono::Utc::now(),
    });
    result_wire_refuses_retired_members(&ready);
    let ready = serde_json::to_value(ready).unwrap();
    assert_eq!(TerminalVersion::try_from(2).unwrap(), TERMINAL_VERSION);
    for version in [0, 1, 3, 255] {
        assert!(TerminalVersion::try_from(version).is_err());
        let mut bad = attach.clone();
        bad["version"] = json!(version);
        assert!(serde_json::from_value::<TerminalAttach>(bad.clone()).is_err());
        assert!(
            serde_json::from_slice::<TerminalAttach>(&serde_json::to_vec(&bad).unwrap()).is_err()
        );
        let mut bad = ready.clone();
        bad["version"] = json!(version);
        assert!(serde_json::from_value::<TerminalServerControl>(bad.clone()).is_err());
        assert!(
            serde_json::from_slice::<TerminalServerControl>(&serde_json::to_vec(&bad).unwrap())
                .is_err()
        );
    }
}
