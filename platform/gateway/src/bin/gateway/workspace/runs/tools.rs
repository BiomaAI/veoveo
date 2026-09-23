//! Model tools share the human operation journal; private results never enter a prompt.
use super::{Caller, ResolvedAgent, RunState};
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

pub(super) struct RunClaim {
    pub chat: WorkspaceChatId,
    pub run: WorkspaceRunId,
    pub fence: Uuid,
}

struct RunTools {
    state: RunState,
    caller: Caller,
    catalog: Arc<GatewayCatalog>,
    claim: RunClaim,
    budget: Mutex<BTreeSet<Uuid>>,
    definition: ResolvedAgent,
    permission_changed: CancellationToken,
    feedback: super::feedback::Feedback,
}

pub(super) async fn for_run(
    state: &RunState,
    caller: &Caller,
    definition: &ResolvedAgent,
    claim: RunClaim,
    permission_changed: CancellationToken,
    feedback: super::feedback::Feedback,
) -> Result<Vec<DynamicTool>, WorkspaceRunFailure> {
    if definition.tools.is_empty() {
        return Ok(vec![]);
    }
    let scope = Arc::new(RunTools {
        state: state.clone(),
        caller: caller.clone(),
        catalog: state.catalog.current(),
        claim,
        budget: Mutex::default(),
        definition: definition.clone(),
        permission_changed,
        feedback,
    });
    let capabilities = state
        .operations
        .capabilities(caller, &definition.tools)
        .await
        .map_err(|status| {
            if matches!(status.as_u16(), 401 | 403 | 404) {
                WorkspaceRunFailure::PermissionChanged
            } else {
                WorkspaceRunFailure::ModelUnavailable
            }
        })?;
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
                "Your permissions changed during this response. The tool was not run.",
            ));
        }
        if self
            .state
            .agents
            .resolve(
                &self.caller.profile,
                &self.caller.subject,
                &self.definition.id,
                Some(&self.definition.revision),
            )
            .await
            .is_err()
        {
            self.permission_changed.cancel();
            return Err(ToolExecutionError::other(
                "This agent's access changed during this response. The tool was not run.",
            ));
        }
        if !arguments.is_object() {
            return Err(ToolExecutionError::other(
                "Tool arguments must be a JSON object.",
            ));
        }
        if let Some(error) = validator.iter_errors(&arguments).next() {
            let path = error.instance_path().to_string();
            let location = if path.is_empty() {
                "the arguments"
            } else {
                path.as_str()
            };
            return Err(ToolExecutionError::other(format!(
                "The arguments don't match the tool's input schema at {location}: {error}. Fix that field and call the tool again."
            )));
        }
        arguments.sort_all_objects();
        let encoded = serde_json::to_string(&arguments)
            .map_err(|_| ToolExecutionError::other("Arguments cannot be encoded."))?;
        if encoded.len() > 65_536 {
            return Err(ToolExecutionError::other(
                "Tool arguments are larger than 64 KiB. Send a smaller request.",
            ));
        }
        let key = serde_json::to_vec(&(&name, &arguments))
            .map_err(|_| ToolExecutionError::other("Arguments cannot be encoded."))?;
        let id = Uuid::new_v5(&self.claim.run.as_uuid(), &Sha256::digest(key));
        let fresh = {
            let mut budget = self
                .budget
                .lock()
                .map_err(|_| ToolExecutionError::other("The tool-call limit for this response couldn't be checked. The tool was not run."))?;
            if !budget.contains(&id)
                && budget.len() >= self.definition.budgets.max_tool_calls as usize
            {
                return Err(ToolExecutionError::other(
                    "This response has reached its tool-call limit. Finish the response without more tool calls.",
                ));
            }
            budget.insert(id)
        };
        let activity = self.feedback.tool();
        self.state.operations.submit(self.caller.clone(), WorkspaceOperationId::from_uuid(id), WorkspaceOperationIntent { app_uri: None,
            chat: self.claim.chat, run: Some((self.claim.run, self.claim.fence)), profile: self.caller.profile.to_string(), tool: name, arguments: encoded,
        }).await.map_err(|status| {
            if matches!(status.as_u16(), 401 | 403 | 404 | 409) { self.permission_changed.cancel(); }
            ToolExecutionError::other("The gateway did not confirm this tool call. Ask the person to check their Activity before retrying; if you retry, use the same arguments.")
        })?;
        if fresh {
            activity.admitted();
        }
        // Returning private tool data would publish it through the shared
        // assistant response. A receipt establishes no completion claim.
        Ok(ToolOutput::text(
            "The tool call was started. Its progress, any input it needs, and its result appear only in the private Activity of the person who sent the message, where they can also cancel it. This reply does not mean the call succeeded, and no result has been shared with this chat.",
        ))
    }
}
