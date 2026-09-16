//! Human-owned dispatch receipts, native MCP Task references and MRTR fences.
mod records;
pub use records::*;

use chrono::Utc;
use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;

use super::{Result, WorkspaceAuthority, WorkspaceError, classify, retryable, validate_text};
use crate::{
    PlatformStore, WorkspaceChatId, WorkspaceOperationId, store::primary_transaction_error,
};

#[derive(Clone, SurrealValue)]
struct Start {
    operation: RecordId,
    chat: RecordId,
    run: Option<RecordId>,
    run_fence: Option<Uuid>,
    profile: String,
    app_uri: Option<String>,
    tool: String,
    arguments: String,
    fence: Uuid,
}
#[derive(Clone, SurrealValue)]
struct Resume {
    operation: RecordId,
    revision: i64,
    fence: Uuid,
}
#[derive(Clone, SurrealValue)]
struct Dispatch {
    operation: RecordId,
    fence: Uuid,
    run_fence: Option<Uuid>,
}
#[derive(Clone, SurrealValue)]
struct Page {
    chat: Option<RecordId>,
    before: Option<RecordId>,
}
#[derive(Clone, SurrealValue)]
struct Settle {
    operation: RecordId,
    fence: Uuid,
    phase: WorkspaceOperationPhase,
    task_id: Option<String>,
    response: Option<String>,
}

impl PlatformStore {
    /// Recheck the person's live chat and optional run authority immediately
    /// before the single external invocation. This never issues a new claim.
    pub async fn check_workspace_operation_dispatch(
        &self,
        authority: &WorkspaceAuthority,
        id: WorkspaceOperationId,
        fence: Uuid,
        run_fence: Option<Uuid>,
    ) -> Result<()> {
        let _: bool = self
            .workspace_query(
                authority,
                Dispatch {
                    operation: id.record_id(),
                    fence,
                    run_fence,
                },
                include_str!("dispatch.surql"),
            )
            .await?;
        Ok(())
    }

    pub async fn start_workspace_operation(
        &self,
        authority: &WorkspaceAuthority,
        id: WorkspaceOperationId,
        intent: WorkspaceOperationIntent,
    ) -> Result<WorkspaceOperationAdmission> {
        validate_text(&intent.profile, "profile", 128)?;
        if let Some(uri) = &intent.app_uri {
            validate_text(uri, "App URI", 2048)?;
            if !uri.starts_with("ui://") || intent.run.is_some() {
                return Err(WorkspaceError::Invalid("App origin"));
            }
        }
        validate_text(&intent.tool, "tool", 256)?;
        validate_text(&intent.arguments, "arguments", 65_536)?;
        let arguments: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&intent.arguments)
                .map_err(|_| WorkspaceError::Invalid("arguments"))?;
        // Stable key order makes idempotency independent of JSON object order.
        let arguments =
            serde_json::to_string(&arguments).map_err(|_| WorkspaceError::Invalid("arguments"))?;
        let fence = Uuid::new_v4();
        let operation: WorkspaceOperation = self
            .workspace_query(
                authority,
                Start {
                    operation: id.record_id(),
                    chat: intent.chat.record_id(),
                    run: intent.run.map(|(run, _)| run.record_id()),
                    run_fence: intent.run.map(|(_, fence)| fence),
                    profile: intent.profile,
                    app_uri: intent.app_uri,
                    tool: intent.tool,
                    arguments,
                    fence,
                },
                include_str!("start.surql"),
            )
            .await?;
        let dispatch = operation.fence == fence;
        Ok(WorkspaceOperationAdmission {
            operation: recover(operation),
            dispatch,
        })
    }

    /// A fresh human answer claims one exact MRTR revision. An old dialog or a
    /// second tab cannot resubmit that revision, even if delivery was ambiguous.
    pub async fn resume_workspace_operation(
        &self,
        authority: &WorkspaceAuthority,
        id: WorkspaceOperationId,
        revision: i64,
    ) -> Result<WorkspaceOperation> {
        self.workspace_query(
            authority,
            Resume {
                operation: id.record_id(),
                revision,
                fence: Uuid::new_v4(),
            },
            include_str!("resume.surql"),
        )
        .await
    }

    pub async fn workspace_operation(
        &self,
        authority: &WorkspaceAuthority,
        id: WorkspaceOperationId,
    ) -> Result<WorkspaceOperation> {
        self.workspace_query(authority, id.record_id(),
            "IF !fn::workspace_operation_owner($authority, $command) { THROW 'workspace_not_found'; }; \
             RETURN SELECT * FROM ONLY $command;",
        ).await.map(recover)
    }

    /// An App may recover only its own native Tasks under the current human and
    /// Work Context. Knowing another App's opaque Task identity grants no access.
    pub async fn workspace_app_task(
        &self,
        authority: &WorkspaceAuthority,
        profile: &str,
        app_uri: &str,
        task_id: &str,
    ) -> Result<WorkspaceOperation> {
        #[derive(Clone, SurrealValue)]
        struct Lookup {
            profile: String,
            app_uri: String,
            task_id: String,
        }
        validate_text(profile, "profile", 128)?;
        validate_text(app_uri, "App URI", 2048)?;
        validate_text(task_id, "Task ID", 4096)?;
        let rows: Vec<WorkspaceOperation> = self
            .workspace_query(
                authority,
                Lookup {
                    profile: profile.into(),
                    app_uri: app_uri.into(),
                    task_id: task_id.into(),
                },
                "RETURN SELECT * FROM workspace_operation
            WHERE tenant = $authority.tenant AND work_context = $authority.work_context
            AND owner = $authority.principal AND profile = $command.profile
            AND app_uri = $command.app_uri AND task_id = $command.task_id LIMIT 1;",
            )
            .await?;
        rows.into_iter()
            .next()
            .map(recover)
            .ok_or(WorkspaceError::NotFound)
    }

    /// Personal activity survives leaving a conversation. This never grants chat
    /// history or native Task access; the gateway must reauthorize Tasks via MCP.
    pub async fn workspace_operations(
        &self,
        authority: &WorkspaceAuthority,
        chat: Option<WorkspaceChatId>,
        before: Option<WorkspaceOperationId>,
    ) -> Result<Vec<WorkspaceOperation>> {
        let operations: Vec<WorkspaceOperation> = self
            .workspace_query(
                authority,
                Page {
                    chat: chat.map(WorkspaceChatId::record_id),
                    before: before.map(WorkspaceOperationId::record_id),
                },
                include_str!("list.surql"),
            )
            .await?;
        Ok(operations.into_iter().map(recover).collect())
    }

    /// Save the observed outcome even if the person's authority changed while
    /// the external call was in flight. A secret server-owned fence authorizes
    /// only this receipt update; it grants no dispatch or subsequent read access.
    pub async fn settle_workspace_operation(
        &self,
        id: WorkspaceOperationId,
        fence: Uuid,
        outcome: WorkspaceOperationOutcome,
    ) -> Result<WorkspaceOperation> {
        use WorkspaceOperationOutcome as Outcome;
        use WorkspaceOperationPhase as Phase;
        let (phase, task_id, response) = match outcome {
            Outcome::InputRequired(response) => (Phase::InputRequired, None, Some(response)),
            Outcome::Task(id) => (Phase::Task, Some(id), None),
            Outcome::Completed(response) => (Phase::Completed, None, Some(response)),
            Outcome::Failed => (Phase::Failed, None, None),
            Outcome::Unconfirmed => (Phase::Unconfirmed, None, None),
        };
        if let Some(task_id) = &task_id {
            validate_text(task_id, "task ID", 4096)?;
        }
        if let Some(response) = &response {
            validate_text(response, "MCP response", 1_048_576)?;
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(response)
                .map_err(|_| WorkspaceError::Invalid("MCP response"))?;
        }
        let command = Settle {
            operation: id.record_id(),
            fence,
            phase,
            task_id,
            response,
        };
        let query = format!(
            "BEGIN TRANSACTION; {} COMMIT TRANSACTION;",
            include_str!("settle.surql")
        );
        for attempt in 0..8 {
            let mut result = self
                .client()
                .query(query.clone())
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
            let last = result
                .num_statements()
                .checked_sub(2)
                .ok_or(WorkspaceError::Unavailable)?;
            let value: Value = result.take(last).map_err(|_| WorkspaceError::Unavailable)?;
            return WorkspaceOperation::from_value(value).map_err(|_| WorkspaceError::Unavailable);
        }
        Err(WorkspaceError::Unavailable)
    }
}

fn recover(mut operation: WorkspaceOperation) -> WorkspaceOperation {
    if operation.phase == WorkspaceOperationPhase::Dispatching
        && operation.dispatch_until <= Utc::now()
    {
        operation.phase = WorkspaceOperationPhase::Unconfirmed;
    }
    operation
}
