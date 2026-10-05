//! Typed facts for authenticated WebhookWait journal transactions.
use crate::{ProviderJobId, TenantId};
use veoveo_types::{ExtensionName, TaskId, TaskTypeName};

#[veoveo_types::id(text(ProviderKeys))]
pub struct ProviderJobKey(String);
#[veoveo_types::id(text(ProviderKeys))]
pub struct ProviderEventKey(String);
#[doc(hidden)]
pub struct ProviderKeys;
impl veoveo_types::IdProfile for ProviderKeys {
    type Error = ProviderKeyError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            if value.is_empty() || value.contains('\0') {
                Err(ProviderKeyError)
            } else {
                Ok(())
            }
        });
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("provider identity must be nonempty and contain no NUL")]
pub struct ProviderKeyError;

/// Correlation facts supplied to a registered Task owner after journal admission.
/// Server, tenant and operation are rechecked by the same transaction that binds the job.
#[derive(Clone, Debug)]
pub struct WebhookJobBinding {
    pub job_id: ProviderJobId,
    pub task_id: TaskId,
    pub server: ExtensionName,
    pub tenant: TenantId,
    pub task_type: TaskTypeName,
    pub provider: ExtensionName,
    pub external_job_id: ProviderJobKey,
}
