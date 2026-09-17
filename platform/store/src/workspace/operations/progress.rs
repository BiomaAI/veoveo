use super::*;

#[derive(Clone, SurrealValue)]
struct Progress {
    operation: RecordId,
    fence: Uuid,
    progress: WorkspaceOperationProgress,
}

impl PlatformStore {
    /// The exact active dispatch fence permits observations, never another call.
    pub async fn record_workspace_progress(
        &self,
        id: WorkspaceOperationId,
        fence: Uuid,
        progress: WorkspaceOperationProgress,
    ) -> Result<()> {
        if !progress.valid() {
            return Err(WorkspaceError::Invalid("operation progress"));
        }
        let command = Progress {
            operation: id.record_id(),
            fence,
            progress,
        };
        let query = concat!(
            "BEGIN TRANSACTION; ",
            include_str!("progress.surql"),
            " COMMIT TRANSACTION;"
        );
        for attempt in 0..8 {
            let mut result = self
                .client()
                .query(query)
                .bind(("command", command.clone()))
                .await
                .map_err(|_| WorkspaceError::Unavailable)?;
            if let Some(error) = primary_transaction_error(result.take_errors()) {
                if retryable(&error) && attempt < 7 {
                    tokio::time::sleep(std::time::Duration::from_millis(1 << attempt)).await;
                    continue;
                }
                return Err(classify(&error));
            }
            return Ok(());
        }
        Err(WorkspaceError::Unavailable)
    }
}
