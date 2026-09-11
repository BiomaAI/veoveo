//! Validate public file identity and bounds; the domain owns current authority.
use uuid::Uuid;
use veoveo_computers_contract as api;

pub(super) fn input(bytes: &[u8], computer: Uuid) -> Result<Vec<u8>, ()> {
    let input: api::TransferFileInput = serde_json::from_slice(bytes).map_err(|_| ())?;
    if input.computer_id != computer
        || input.request_id.get_version_num() != 7
        || input.grant_id.is_some_and(|id| id.get_version_num() != 7)
        || !(1..=300).contains(&input.limits.maximum_seconds)
        || !(1..=api::MAX_TRANSFER_BYTES).contains(&input.limits.maximum_bytes)
    {
        return Err(());
    }
    serde_json::to_vec(&input).map_err(|_| ())
}

pub(super) fn receipt(bytes: &[u8], computer: Uuid, task: Option<Uuid>) -> Result<Vec<u8>, ()> {
    let value: api::FileTransferView = serde_json::from_slice(bytes).map_err(|_| ())?;
    if value.computer_id != computer
        || value.task_id.get_version_num() != 7
        || task.is_some_and(|id| id != value.task_id)
        || value.message.as_ref().is_some_and(|s| s.len() > 4096)
        || (value.can_cancel
            && (value.cancellation_requested_at.is_some()
                || matches!(
                    value.stage,
                    api::FileTransferStage::Completed
                        | api::FileTransferStage::Failed
                        | api::FileTransferStage::Cancelled
                        | api::FileTransferStage::RecoveryRequired
                )))
    {
        return Err(());
    }
    if let Some(result) = &value.result
        && (value.stage != api::FileTransferStage::Completed
            || result.computer_id != computer
            || result.transfer_id != value.task_id
            || result.result_uri.transfer_id() != value.task_id
            || result.direction != value.direction
            || result.artifact_id.get_version_num() != 7
            || result.bytes > api::MAX_TRANSFER_BYTES
            || result.sha256.len() != 64
            || !result
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    {
        return Err(());
    }
    serde_json::to_vec(&value).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn routes_keep_file_admission_and_current_owner_cancellation_separate() {
        use super::super::routes::Operation;
        use axum::http::Method;
        let computer = Uuid::now_v7();
        let task = Uuid::now_v7();
        let admission = Operation::from_route(
            "/computers/{profile}/{id}/files",
            &Method::POST,
            Some(computer),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            admission.service_path(),
            format!("computers/{computer}/files")
        );
        assert!(admission.requires_contributor());
        assert!(
            matches!(admission.authorization().0, veoveo_mcp_contract::PolicyTarget::Tool { tool, .. } if tool.as_str() == "transfer_file")
        );
        for (suffix, method) in [("", Method::GET), ("/cancel", Method::POST)] {
            let route = format!("/computers/{{profile}}/{{id}}/files/{{operation_id}}{suffix}");
            let action =
                Operation::from_route(&route, &method, Some(computer), Some(task), None, None)
                    .unwrap();
            assert_eq!(
                action.service_path(),
                format!("computers/{computer}/files/{task}{suffix}")
            );
            assert!(!action.requires_contributor());
            assert!(
                matches!(action.authorization().0, veoveo_mcp_contract::PolicyTarget::Resource { uri, .. } if uri.as_str() == api::computer_uri(computer))
            );
            assert!(
                Operation::from_route(
                    &route,
                    &method,
                    Some(computer),
                    Some(task),
                    Some(task),
                    None
                )
                .is_err()
            );
        }
    }

    #[test]
    fn file_routes_bind_input_and_output_to_the_exact_computer_and_task() {
        let computer = Uuid::now_v7();
        let task = Uuid::now_v7();
        let request = json!({"computerId":computer,"requestId":Uuid::now_v7(),"grantId":null,
            "transfer":{"kind":"import","artifactId":Uuid::now_v7(),"path":"data.bin"},
            "limits":{"maximumSeconds":30,"maximumBytes":1024,"onInterruption":"stop_computer"}});
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(input(&bytes, computer).is_ok());
        assert!(input(&bytes, Uuid::now_v7()).is_err());
        let mut invalid = request;
        invalid["transfer"]["overwrite"] = true.into();
        assert!(input(&serde_json::to_vec(&invalid).unwrap(), computer).is_err());
        let now = chrono::Utc::now();
        let result = json!({"taskId":task,"computerId":computer,"direction":"import","stage":"completed",
            "message":"File transferred","canCancel":false,"cancellationRequestedAt":null,
            "createdAt":now,"updatedAt":now,"completedAt":now,
            "result":{"result_uri":format!("computer://transfers/{task}"),"computerId":computer,"transferId":task,
                "direction":"import","artifactId":Uuid::now_v7(),"bytes":1024,"sha256":"07".repeat(32)}});
        let bytes = serde_json::to_vec(&result).unwrap();
        assert!(receipt(&bytes, computer, Some(task)).is_ok());
        assert!(receipt(&bytes, computer, Some(Uuid::now_v7())).is_err());
        assert!(receipt(&bytes, Uuid::now_v7(), Some(task)).is_err());
        for (field, value) in [
            ("computerId", json!(Uuid::now_v7())),
            ("transferId", json!(Uuid::now_v7())),
            ("direction", json!("export")),
            ("bytes", json!(api::MAX_TRANSFER_BYTES + 1)),
            ("sha256", json!("invalid")),
        ] {
            let mut invalid = result.clone();
            invalid["result"][field] = value;
            assert!(receipt(&serde_json::to_vec(&invalid).unwrap(), computer, Some(task)).is_err());
        }
    }
}
