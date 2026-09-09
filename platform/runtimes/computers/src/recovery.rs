//! One bounded observation of a durably recorded lifecycle intent.
//!
//! The caller persists the checkpoint and charges its durable recovery budget before
//! each call. This adapter never retries a mutation or starts a background poller.
use crate::{Binding, Observation, OpenShellRuntime, Phase, Result, RuntimeFailure};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

const READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Goal {
    Create,
    Start {
        sandbox_id: String,
        previous_process_id: String,
    },
    Stop {
        sandbox_id: String,
        process_id: String,
    },
}

/// Internal persisted intent. Deserialization revalidates its identity and epoch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Record", into = "Record")]
pub struct LifecycleCheckpoint {
    operation_id: Uuid,
    provider_instance_id: Uuid,
    binding: Binding,
    goal: Goal,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    version: u8,
    operation_id: Uuid,
    provider_instance_id: Uuid,
    computer_id: Uuid,
    replacement_instance_id: Option<Uuid>,
    template_fingerprint: String,
    goal: Goal,
}

impl LifecycleCheckpoint {
    pub fn create(
        provider_instance_id: Uuid,
        operation_id: Uuid,
        binding: Binding,
    ) -> Result<Self> {
        Self::checked(provider_instance_id, operation_id, binding, Goal::Create)
    }

    pub fn start(
        provider_instance_id: Uuid,
        operation_id: Uuid,
        binding: Binding,
        before: &Observation,
    ) -> Result<Self> {
        if before.phase != Phase::Stopped {
            return Err(RuntimeFailure::InvalidState);
        }
        Self::checked(
            provider_instance_id,
            operation_id,
            binding,
            Goal::Start {
                sandbox_id: before.sandbox_id.clone(),
                previous_process_id: before.main_process_instance_id.clone(),
            },
        )
    }

    pub fn stop(
        provider_instance_id: Uuid,
        operation_id: Uuid,
        binding: Binding,
        before: &Observation,
    ) -> Result<Self> {
        if before.phase != Phase::Ready {
            return Err(RuntimeFailure::InvalidState);
        }
        Self::checked(
            provider_instance_id,
            operation_id,
            binding,
            Goal::Stop {
                sandbox_id: before.sandbox_id.clone(),
                process_id: before.main_process_instance_id.clone(),
            },
        )
    }

    fn checked(
        provider_instance_id: Uuid,
        operation_id: Uuid,
        binding: Binding,
        goal: Goal,
    ) -> Result<Self> {
        if operation_id.is_nil() || provider_instance_id.is_nil() {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let epoch = match &goal {
            Goal::Create => None,
            Goal::Start {
                sandbox_id,
                previous_process_id,
            } => Some((sandbox_id, previous_process_id)),
            Goal::Stop {
                sandbox_id,
                process_id,
            } => Some((sandbox_id, process_id)),
        };
        if epoch.is_some_and(|(sandbox, process)| {
            !crate::models::identifier(sandbox) || !crate::models::identifier(process)
        }) {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(Self {
            operation_id,
            provider_instance_id,
            binding,
            goal,
        })
    }

    pub fn operation_id(&self) -> Uuid {
        self.operation_id
    }

    fn assess(&self, seen: Observation) -> Result<LifecycleObservation> {
        if seen.phase == Phase::Ready && seen.exit_code.is_some() {
            return Err(RuntimeFailure::LifecycleUnknown);
        }
        let reached = match &self.goal {
            Goal::Create => seen.phase == Phase::Ready,
            Goal::Start {
                sandbox_id,
                previous_process_id,
            } => {
                if seen.sandbox_id != *sandbox_id {
                    return Err(RuntimeFailure::BindingMismatch);
                }
                // Starting retains the stopped run's identity until the new
                // supervisor session commits. Old Ready cannot settle this Start.
                seen.phase == Phase::Ready && seen.main_process_instance_id != *previous_process_id
            }
            Goal::Stop {
                sandbox_id,
                process_id,
            } => {
                if seen.sandbox_id != *sandbox_id || seen.main_process_instance_id != *process_id {
                    return Err(RuntimeFailure::BindingMismatch);
                }
                seen.phase == Phase::Stopped
            }
        };
        if reached {
            if seen.main_process_instance_id.is_empty() {
                return Err(RuntimeFailure::LifecycleUnknown);
            }
            Ok(LifecycleObservation::Reached(seen))
        } else if matches!(
            seen.phase,
            Phase::Provisioning | Phase::Starting | Phase::Stopping | Phase::Ready | Phase::Stopped
        ) {
            Ok(LifecycleObservation::Pending(seen))
        } else {
            Err(RuntimeFailure::LifecycleUnknown)
        }
    }
}

/// Pending does not authorize redispatch. Neither variant releases a domain fence.
#[derive(Clone, PartialEq)]
pub enum LifecycleObservation {
    Reached(Observation),
    Pending(Observation),
}

impl OpenShellRuntime {
    /// Observe once after a lost reply/watch. The caller owns the persisted budget
    /// and selects this runtime from its installation-owned provider identity.
    pub async fn reconcile_lifecycle(
        &self,
        checkpoint: &LifecycleCheckpoint,
        remaining: Duration,
    ) -> Result<LifecycleObservation> {
        if self.provider_instance_id != checkpoint.provider_instance_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        if remaining.is_zero() {
            return Err(RuntimeFailure::LifecycleUnknown);
        }
        let seen = tokio::time::timeout(remaining.min(READ_TIMEOUT), self.get(&checkpoint.binding))
            .await
            .map_err(|_| RuntimeFailure::LifecycleUnknown)??
            // Not found does not prove a lost Create never took effect.
            .ok_or(RuntimeFailure::LifecycleUnknown)?;
        checkpoint.assess(seen)
    }
}

impl From<LifecycleCheckpoint> for Record {
    fn from(value: LifecycleCheckpoint) -> Self {
        Self {
            version: 1,
            operation_id: value.operation_id,
            provider_instance_id: value.provider_instance_id,
            computer_id: value.binding.computer_id(),
            replacement_instance_id: value.binding.replacement_instance_id(),
            template_fingerprint: value.binding.template_fingerprint().to_owned(),
            goal: value.goal,
        }
    }
}

impl TryFrom<Record> for LifecycleCheckpoint {
    type Error = RuntimeFailure;

    fn try_from(value: Record) -> Result<Self> {
        if value.version != 1 {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let binding = match value.replacement_instance_id {
            Some(instance) => {
                Binding::replacement(value.computer_id, instance, value.template_fingerprint)
            }
            None => Binding::new(value.computer_id, value.template_fingerprint),
        }?;
        Self::checked(
            value.provider_instance_id,
            value.operation_id,
            binding,
            value.goal,
        )
    }
}
