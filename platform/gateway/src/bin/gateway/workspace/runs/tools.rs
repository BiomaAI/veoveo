//! Model tools share the human operation journal; private results never enter a prompt.
use super::{Caller, RunState, config::Definition};
use rig::tool::{DynamicTool, ToolExecutionError, ToolOutput};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_mcp_gateway::GatewayCatalog;
use veoveo_platform_store::{
    WorkspaceChatId, WorkspaceOperationId, WorkspaceRunId,
    workspace::{WorkspaceOperationIntent, WorkspaceRunFailure},
};

type ToolResult = Result<ToolOutput, ToolExecutionError>;

struct RunTools {
    state: RunState,
    caller: Caller,
    catalog: Arc<GatewayCatalog>,
    chat: WorkspaceChatId,
    run: WorkspaceRunId,
    fence: Uuid,
    budget: Mutex<BTreeSet<Uuid>>,
    permission_changed: CancellationToken,
}

pub(super) async fn for_run(
    state: &RunState,
    caller: &Caller,
    definition: &Definition,
    chat: WorkspaceChatId,
    run: WorkspaceRunId,
    fence: Uuid,
    permission_changed: CancellationToken,
) -> Result<Vec<DynamicTool>, WorkspaceRunFailure> {
    if definition.tools.is_empty() {
        return Ok(vec![]);
    }
    let scope = Arc::new(RunTools {
        state: state.clone(),
        caller: caller.clone(),
        catalog: state.catalog.current(),
        chat,
        run,
        fence,
        budget: Mutex::default(),
        permission_changed,
    });
    let capabilities = state
        .operations
        .capabilities(caller)
        .await
        .map_err(|_| WorkspaceRunFailure::PermissionChanged)?;
    let admitted: Vec<_> = capabilities
        .into_iter()
        .filter(|tool| {
            definition
                .tools
                .iter()
                .any(|name| name.as_str() == tool.name)
        })
        .collect();
    let bytes = serde_json::to_vec(&admitted).map_err(|_| WorkspaceRunFailure::ModelUnavailable)?;
    if bytes.len() > 128 * 1024 {
        return Err(WorkspaceRunFailure::OutputLimit);
    }
    admitted
        .into_iter()
        .map(|tool| {
            let scope = scope.clone();
            let name = tool.name.into_owned();
            let schema = Value::Object((*tool.input_schema).clone());
            let validator = Arc::new(
                jsonschema::validator_for(&schema)
                    .map_err(|_| WorkspaceRunFailure::ModelUnavailable)?,
            );
            Ok(DynamicTool::new(
                name.clone(),
                tool.description
                    .map(|text| text.into_owned())
                    .unwrap_or_default(),
                schema,
                move |_, arguments| {
                    let (scope, name, validator) = (scope.clone(), name.clone(), validator.clone());
                    Box::pin(async move { scope.call(name, validator, arguments).await })
                },
            ))
        })
        .collect()
}

impl RunTools {
    async fn call(
        &self,
        name: String,
        validator: Arc<jsonschema::Validator>,
        mut arguments: Value,
    ) -> ToolResult {
        if !Arc::ptr_eq(&self.catalog, &self.state.catalog.current()) {
            self.permission_changed.cancel();
            return Err(ToolExecutionError::other(
                "Capability permission changed. No new operation was admitted.",
            ));
        }
        if !arguments.is_object() || !validator.is_valid(&arguments) {
            return Err(ToolExecutionError::other(
                "Arguments do not satisfy the capability schema.",
            ));
        }
        arguments.sort_all_objects();
        let encoded = serde_json::to_string(&arguments)
            .map_err(|_| ToolExecutionError::other("Arguments cannot be encoded."))?;
        if encoded.len() > 65_536 {
            return Err(ToolExecutionError::other(
                "Arguments exceed the operation limit.",
            ));
        }
        let key = serde_json::to_vec(&(&name, &arguments))
            .map_err(|_| ToolExecutionError::other("Arguments cannot be encoded."))?;
        let id = Uuid::new_v5(&self.run.as_uuid(), &Sha256::digest(key));
        {
            let mut budget = self
                .budget
                .lock()
                .map_err(|_| ToolExecutionError::other("Operation budget is unavailable."))?;
            if !budget.contains(&id) && budget.len() >= 8 {
                return Err(ToolExecutionError::other(
                    "This response has reached its eight-operation limit.",
                ));
            }
            budget.insert(id);
        }
        self.state.operations.submit(self.caller.clone(), WorkspaceOperationId::from_uuid(id), WorkspaceOperationIntent { app_uri: None,
            chat: self.chat, run: Some((self.run, self.fence)), profile: self.caller.profile.to_string(), tool: name, arguments: encoded,
        }).await.map_err(|status| {
            if matches!(status.as_u16(), 401 | 403 | 404 | 409) { self.permission_changed.cancel(); }
            ToolExecutionError::other("Operation admission was not confirmed. Check Activity; do not change arguments merely to retry.")
        })?;
        // Returning private tool data would publish it through the shared
        // assistant response. A receipt establishes no completion claim.
        Ok(ToolOutput::text(
            "The operation is recorded in the initiating person's private Activity. They can inspect its current status, provide requested input, and request cancellation there. This receipt does not establish completion or success; no result has been shared with this chat.",
        ))
    }
}
