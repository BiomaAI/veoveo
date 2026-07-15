//! Per-episode budget enforcement, inside the run.
//!
//! The hook counts completion calls and tool calls against the manifest caps
//! and terminates the run on breach; the driver recognizes the marker in the
//! cancellation reason and books the episode as `budget_terminated` instead
//! of an error. Window budgets (episodes per hour) are enforced by the
//! scheduler before an episode starts, from the episode table itself.

use std::sync::atomic::{AtomicU64, Ordering};

use rig_core::{
    agent::{
        AgentHook, CompletionCallAction, CompletionCallEvent, HookContext, ToolCall, ToolCallAction,
    },
    completion::CompletionModel,
};

use crate::manifest::PerEpisodeBudget;

pub const BUDGET_TERMINATED_PREFIX: &str = "episode budget exhausted";

pub struct BudgetHook {
    budget: PerEpisodeBudget,
    completion_calls: AtomicU64,
    tool_calls: AtomicU64,
}

impl BudgetHook {
    pub fn new(budget: PerEpisodeBudget) -> Self {
        Self {
            budget,
            completion_calls: AtomicU64::new(0),
            tool_calls: AtomicU64::new(0),
        }
    }
}

impl<M> AgentHook<M> for BudgetHook
where
    M: CompletionModel,
{
    async fn on_completion_call(
        &self,
        _ctx: &HookContext,
        _event: CompletionCallEvent<'_>,
    ) -> CompletionCallAction {
        let seen = self.completion_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(max) = self.budget.max_completion_calls
            && seen > max
        {
            return CompletionCallAction::stop(format!(
                "{BUDGET_TERMINATED_PREFIX}: completion calls exceeded {max}"
            ));
        }
        CompletionCallAction::continue_run()
    }

    async fn on_tool_call(&self, _ctx: &HookContext, _event: ToolCall<'_>) -> ToolCallAction {
        let seen = self.tool_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(max) = self.budget.max_tool_calls
            && seen > max
        {
            return ToolCallAction::stop(format!(
                "{BUDGET_TERMINATED_PREFIX}: tool calls exceeded {max}"
            ));
        }
        ToolCallAction::run()
    }
}
