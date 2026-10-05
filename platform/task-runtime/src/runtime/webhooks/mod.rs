//! Authenticated provider journals for WebhookWait Tasks.
mod journal;
mod reads;
mod settlement;
mod transactions;

use crate::{RecoveryClass, TaskError, TaskRuntime, TaskSnapshot};
use veoveo_platform_store::{ProviderEventRecord, ProviderJobRecord};
use veoveo_types::ExtensionName;

/// Trusted-service access to one provider's journal. Caller-facing Task policy remains separate.
#[derive(Clone)]
pub struct WebhookJournal {
    runtime: TaskRuntime,
    provider: ExtensionName,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebhookTerminal {
    Succeeded,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug)]
pub struct AuthenticatedWebhookReceipt {
    pub event: ProviderEventRecord,
    pub job: ProviderJobRecord,
    pub inserted: bool,
    pub authoritative: bool,
}
impl TaskRuntime {
    pub fn webhooks(&self, provider: ExtensionName) -> WebhookJournal {
        WebhookJournal {
            runtime: self.clone(),
            provider,
        }
    }
}
impl WebhookJournal {
    fn check_task(&self, task: &TaskSnapshot) -> Result<(), TaskError> {
        if task.server != self.runtime.server() || task.recovery_class != RecoveryClass::WebhookWait
        {
            return Err(TaskError::InvalidRecord(
                "webhook journal requires the exact server's WebhookWait Task".into(),
            ));
        }
        self.runtime.require_contribution(&task.task_type)
    }
}

fn task_id(record: &veoveo_platform_store::RecordId) -> Result<veoveo_types::TaskId, TaskError> {
    if record.table.as_str() != "task" {
        return Err(TaskError::InvalidRecord(
            "provider journal parent is not a Task".into(),
        ));
    }
    let veoveo_platform_store::RecordIdKey::Uuid(id) = &record.key else {
        return Err(TaskError::InvalidRecord(
            "provider journal parent lacks native Task identity".into(),
        ));
    };
    crate::types::validate_task_id(veoveo_types::TaskId::from_uuid(**id))
}

// Each journal transaction ends with RETURN followed by COMMIT. The server
// includes the empty COMMIT response, so the retained return is the penultimate slot.
fn return_slot(statements: usize) -> Result<usize, TaskError> {
    statements
        .checked_sub(2)
        .ok_or_else(|| TaskError::InvalidRecord("webhook transaction return missing".into()))
}
