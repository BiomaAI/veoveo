//! Episode-local projection that hands durable MCP tasks to kernel watchers.

use rig::{
    tool::{
        DeferredToolDescriptor, DeferredToolDriver, DeferredToolHandle, DeferredToolResolver,
        DeferredToolState, InputResponses, ToolContext, ToolExecutionError, ToolOutput, ToolResult,
    },
    wasm_compat::WasmBoxedFuture,
};
use veoveo_mcp_contract::CanonicalTaskId;

/// Result metadata that tells the recorder not to settle the durable task in-run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundTaskDetached {
    pub task_id: CanonicalTaskId,
}

/// Resolves every accepted gateway task to an immediate background-task handoff.
///
/// The gateway has already created the durable task before Rig receives its
/// descriptor. The kernel persists that descriptor in the recorder hook, ends
/// the bounded episode, and lets the credential-rotating watcher reconstruct
/// the real task from its canonical id.
#[derive(Clone, Debug)]
pub struct BackgroundTaskResolver {
    backend_type: String,
}

impl BackgroundTaskResolver {
    pub fn new(backend_type: impl Into<String>) -> Self {
        Self {
            backend_type: backend_type.into(),
        }
    }
}

impl DeferredToolResolver for BackgroundTaskResolver {
    fn backend_type(&self) -> &str {
        &self.backend_type
    }

    fn resolve<'a>(
        &'a self,
        descriptor: &'a DeferredToolDescriptor,
    ) -> WasmBoxedFuture<'a, Result<DeferredToolHandle, ToolExecutionError>> {
        Box::pin(async move {
            if descriptor.backend_type() != self.backend_type {
                return Err(ToolExecutionError::invalid_args(format!(
                    "deferred descriptor targets '{}' but the background resolver owns '{}'",
                    descriptor.backend_type(),
                    self.backend_type
                )));
            }
            let task_id =
                CanonicalTaskId::new(descriptor.execution_id().to_owned()).map_err(|error| {
                    ToolExecutionError::invalid_args(format!(
                        "deferred descriptor has a non-canonical task id: {error}"
                    ))
                })?;
            Ok(DeferredToolHandle::new(
                descriptor.clone(),
                BackgroundTaskDriver { task_id },
            ))
        })
    }
}

#[derive(Clone, Debug)]
struct BackgroundTaskDriver {
    task_id: CanonicalTaskId,
}

impl BackgroundTaskDriver {
    fn detached(&self) -> DeferredToolState {
        DeferredToolState::Completed(ToolResult::success(ToolOutput::text(format!(
            "Durable task `{}` is running in the background. Do not repeat or poll this tool call. End this episode; a later task-result wake will carry its terminal result.",
            self.task_id
        ))))
    }
}

impl DeferredToolDriver for BackgroundTaskDriver {
    fn state(&self) -> WasmBoxedFuture<'_, Result<DeferredToolState, ToolExecutionError>> {
        Box::pin(async { Ok(self.detached()) })
    }

    fn submit_input(
        &self,
        _responses: InputResponses,
    ) -> WasmBoxedFuture<'_, Result<DeferredToolState, ToolExecutionError>> {
        Box::pin(async {
            Err(ToolExecutionError::invalid_args(
                "background tasks accept input through their durable watcher",
            ))
        })
    }

    fn cancel(&self) -> WasmBoxedFuture<'_, Result<DeferredToolState, ToolExecutionError>> {
        Box::pin(async { Ok(DeferredToolState::Cancelled) })
    }

    fn publish_result_context(&self, context: &mut ToolContext) {
        context.insert_result(BackgroundTaskDetached {
            task_id: self.task_id.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn accepted_task_is_projected_as_a_non_terminal_background_handoff() {
        let descriptor = DeferredToolDescriptor::new(
            "mcp:test",
            "gtr_0123456789abcdef",
            serde_json::json!({ "kind": "task" }),
        );
        let resolver = BackgroundTaskResolver::new("mcp:test");
        let handle = resolver.resolve(&descriptor).await.unwrap();
        let state = handle.state().await.unwrap();
        let DeferredToolState::Completed(result) = state else {
            panic!("background projection must return a model-visible handoff");
        };
        assert!(result.output().render().contains("Do not repeat or poll"));

        let mut context = ToolContext::new();
        handle.publish_result_context(&mut context);
        let detached = context.result::<BackgroundTaskDetached>().unwrap();
        assert_eq!(detached.task_id.as_str(), "gtr_0123456789abcdef");
    }

    #[tokio::test]
    async fn resolver_rejects_a_different_backend() {
        let descriptor =
            DeferredToolDescriptor::new("mcp:other", "gtr_0123456789abcdef", serde_json::json!({}));
        let error = BackgroundTaskResolver::new("mcp:test")
            .resolve(&descriptor)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("background resolver owns"));
    }
}
