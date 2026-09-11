//! Private provider effects consume journal tickets; recovery retains exact identities.
use super::*;
use std::future::Future;
use veoveo_computers::{
    maintenance::{
        MaintenanceEvidence as Evidence, MaintenanceSource, MaintenanceStep, MaintenanceTicket,
    },
    secrets::MaintenanceCheckpoint,
};
use veoveo_computers_runtime::{
    Binding, LifecycleCheckpoint, LifecycleObservation, Observation, Phase, PolicyRestoration,
    ReplacementPolicy, RetainedHandoff, RuntimeFailure,
};

type NativeResult<T> = std::result::Result<T, RuntimeFailure>;
const INVALID: RuntimeFailure = RuntimeFailure::BindingMismatch;
pub(super) enum Reached {
    Evidence(Evidence),
    Capture(MaintenanceCheckpoint),
}
struct Intent {
    source: Binding,
    target: Binding,
}
impl Intent {
    fn new(operation: &MaintenanceOperation) -> NativeResult<Self> {
        Ok(Self {
            source: Binding::from_instance(
                operation.computer_id,
                operation.source_instance_id,
                operation.source_template_fingerprint.clone(),
            )?,
            target: Binding::from_instance(
                operation.computer_id,
                operation.target_instance_id,
                operation.target.template_fingerprint.clone(),
            )?,
        })
    }
}
fn source_run(operation: &MaintenanceOperation, phase: Phase) -> NativeResult<Observation> {
    let (resource, process) = match &operation.source {
        MaintenanceSource::Ready {
            resource_id,
            process_id,
        }
        | MaintenanceSource::Stopped {
            resource_id,
            process_id,
        } => (resource_id, process_id),
        _ => return Err(INVALID),
    };
    Ok(Observation {
        sandbox_id: resource.clone(),
        main_process_instance_id: process.clone(),
        phase,
        exit_code: None,
    })
}
fn target_run(operation: &MaintenanceOperation, seen: &Observation) -> NativeResult<()> {
    let (resource, process) = operation.created_run().ok_or(INVALID)?;
    if seen.phase != Phase::Ready
        || seen.sandbox_id != resource
        || seen.main_process_instance_id != process
    {
        return Err(INVALID);
    }
    Ok(())
}

impl MaintenanceWorker {
    pub(super) async fn prepare_create(
        &self,
        operation: &MaintenanceOperation,
    ) -> NativeResult<()> {
        let intent = Intent::new(operation)?;
        // Transfer already committed this exact target. This is the same retained
        // preflight used by lifecycle Start, before any Create dispatch ticket.
        self.homes
            .allocator(intent.target.template_fingerprint())
            .map_err(|_| INVALID)?
            .restore(&intent.target)
            .await
    }
    pub(super) async fn perform(
        &self,
        claim: &ClaimedTask,
        ticket: &MaintenanceTicket,
    ) -> NativeResult<Reached> {
        tokio::time::timeout(ticket.remaining(), self.native_step(claim, ticket))
            .await
            .map_err(|_| RuntimeFailure::LifecycleUnknown)?
    }
    async fn native_step(
        &self,
        claim: &ClaimedTask,
        ticket: &MaintenanceTicket,
    ) -> NativeResult<Reached> {
        let operation = ticket.operation();
        let intent = Intent::new(operation)?;
        let (source_template, target_template) = self
            .profiles
            .for_operation(operation)
            .map_err(|_| INVALID)?;
        match ticket.step() {
            MaintenanceStep::Stop => {
                let before = source_run(operation, Phase::Ready)?;
                let checkpoint = LifecycleCheckpoint::stop(
                    operation.provider_instance_id,
                    operation.operation_id,
                    intent.source.clone(),
                    &before,
                )?;
                let seen = if ticket.is_dispatch() {
                    let current = self
                        .dispatch(ticket, self.runtime.stop(&intent.source, &before))
                        .await?;
                    self.runtime
                        .wait_for_lifecycle(&checkpoint, &current, ticket.remaining())
                        .await?
                } else {
                    let LifecycleObservation::Reached(seen) = self
                        .runtime
                        .reconcile_lifecycle(&checkpoint, ticket.remaining())
                        .await?
                    else {
                        return Err(RuntimeFailure::LifecycleUnknown);
                    };
                    seen
                };
                Ok(Reached::Evidence(Evidence::Stopped {
                    resource_id: seen.sandbox_id,
                    process_id: seen.main_process_instance_id,
                }))
            }
            MaintenanceStep::Capture => {
                let snapshot = self
                    .runtime
                    .capture_replacement_policy(&intent.source, source_template)
                    .await?;
                let bytes = snapshot.checkpoint()?;
                // Bind the captured process to the journal's original stopped run.
                self.runtime.recover_replacement_policy(
                    &bytes,
                    &intent.source,
                    source_template,
                    &source_run(operation, Phase::Stopped)?,
                )?;
                Ok(Reached::Capture(
                    MaintenanceCheckpoint::new(bytes)
                        .map_err(|_| RuntimeFailure::PolicyContinuity)?,
                ))
            }
            MaintenanceStep::Retire => {
                // A missing key or invalid snapshot must be discovered while the
                // stopped source still exists. Recovery uses the same encrypted row.
                self.policy(claim, operation, &intent).await?;
                if ticket.is_dispatch() {
                    let stopped = self.runtime.get(&intent.source).await?.ok_or(INVALID)?;
                    let expected = source_run(operation, Phase::Stopped)?;
                    if stopped.phase != Phase::Stopped
                        || stopped.sandbox_id != expected.sandbox_id
                        || stopped.main_process_instance_id != expected.main_process_instance_id
                    {
                        return Err(INVALID);
                    }
                    self.dispatch(ticket, self.runtime.retire(&intent.source, &stopped))
                        .await?;
                }
                if self.runtime.get(&intent.source).await?.is_some() {
                    return Err(RuntimeFailure::LifecycleUnknown);
                }
                // Absence is not physical writer proof. Transfer must obtain that
                // independent proof before the next instance may acquire the home.
                Ok(Reached::Evidence(Evidence::Retired))
            }
            MaintenanceStep::Transfer => {
                let transfer = async {
                    if matches!(operation.source, MaintenanceSource::InitialFailure { .. }) {
                        self.homes
                            .allocator(intent.target.template_fingerprint())
                            .map_err(|_| INVALID)?
                            .abandon(operation.operation_id, &intent.source, &intent.target)
                            .await
                    } else {
                        self.policy(claim, operation, &intent).await?;
                        self.handoff(operation, &intent).await.map(|_| ())
                    }
                };
                if ticket.is_dispatch() {
                    self.dispatch(ticket, transfer).await?;
                } else {
                    // This allocator profile durably deduplicates the complete
                    // original operation/source/target and cannot create another
                    // writer. It is the one qualified mutation replay in recovery.
                    transfer.await?;
                }
                Ok(Reached::Evidence(Evidence::Transferred))
            }
            MaintenanceStep::Create => {
                let checkpoint = LifecycleCheckpoint::create(
                    operation.provider_instance_id,
                    operation.operation_id,
                    intent.target.clone(),
                )?;
                let seen = if ticket.is_dispatch() {
                    let current = self
                        .dispatch(ticket, self.runtime.create(&intent.target, target_template))
                        .await?;
                    self.runtime
                        .wait_for_lifecycle(&checkpoint, &current, ticket.remaining())
                        .await?
                } else {
                    let LifecycleObservation::Reached(seen) = self
                        .runtime
                        .reconcile_lifecycle(&checkpoint, ticket.remaining())
                        .await?
                    else {
                        return Err(RuntimeFailure::LifecycleUnknown);
                    };
                    seen
                };
                Ok(Reached::Evidence(Evidence::Created {
                    resource_id: seen.sandbox_id,
                    process_id: seen.main_process_instance_id,
                }))
            }
            MaintenanceStep::Restore => {
                let policy = self.policy(claim, operation, &intent).await?;
                let proof = self.handoff(operation, &intent).await?;
                target_run(
                    operation,
                    &self.runtime.get(&intent.target).await?.ok_or(INVALID)?,
                )?;
                let restored = if ticket.is_dispatch() {
                    self.dispatch(
                        ticket,
                        self.runtime.restore_replacement_policy(
                            &policy,
                            &proof,
                            source_template,
                            target_template,
                        ),
                    )
                    .await?
                } else {
                    self.runtime
                        .reconcile_replacement_policy(
                            &policy,
                            &proof,
                            source_template,
                            target_template,
                            ticket.remaining(),
                        )
                        .await?
                };
                target_run(
                    operation,
                    &self.runtime.get(&intent.target).await?.ok_or(INVALID)?,
                )?;
                Ok(Reached::Evidence(Evidence::Restored {
                    policy_version: restored.policy_version,
                    policy_hash: restored.policy_hash,
                }))
            }
        }
    }
    async fn policy(
        &self,
        claim: &ClaimedTask,
        operation: &MaintenanceOperation,
        intent: &Intent,
    ) -> NativeResult<ReplacementPolicy> {
        let checkpoint = self
            .store
            .maintenance_checkpoint(claim, &self.keys)
            .await
            .map_err(|_| RuntimeFailure::PolicyContinuity)?;
        let (source_template, _) = self
            .profiles
            .for_operation(operation)
            .map_err(|_| INVALID)?;
        self.runtime.recover_replacement_policy(
            checkpoint.bytes(),
            &intent.source,
            source_template,
            &source_run(operation, Phase::Stopped)?,
        )
    }
    async fn handoff(
        &self,
        operation: &MaintenanceOperation,
        intent: &Intent,
    ) -> NativeResult<RetainedHandoff> {
        let source = source_run(operation, Phase::Stopped)?;
        self.homes
            .allocator(intent.target.template_fingerprint())
            .map_err(|_| INVALID)?
            .handoff(
                operation.operation_id,
                &intent.source,
                &intent.target,
                &source.sandbox_id,
            )
            .await
    }
    pub(super) async fn qualify_target(
        &self,
        claim: &ClaimedTask,
        operation: &MaintenanceOperation,
    ) -> NativeResult<()> {
        let intent = Intent::new(operation)?;
        target_run(
            operation,
            &self.runtime.get(&intent.target).await?.ok_or(INVALID)?,
        )?;
        if !matches!(operation.source, MaintenanceSource::InitialFailure { .. }) {
            let snapshot = self.policy(claim, operation, &intent).await?;
            let proof = self.handoff(operation, &intent).await?;
            let (source, target) = self
                .profiles
                .for_operation(operation)
                .map_err(|_| INVALID)?;
            let restored = self
                .runtime
                .reconcile_replacement_policy(
                    &snapshot,
                    &proof,
                    source,
                    target,
                    Duration::from_secs(10),
                )
                .await?;
            let Some(Evidence::Restored {
                policy_version,
                policy_hash,
            }) = operation.steps().last().and_then(|s| s.evidence.as_ref())
            else {
                return Err(INVALID);
            };
            if restored
                != (PolicyRestoration {
                    policy_version: *policy_version,
                    policy_hash: policy_hash.clone(),
                })
            {
                return Err(RuntimeFailure::PolicyContinuity);
            }
        }
        target_run(
            operation,
            &self.runtime.get(&intent.target).await?.ok_or(INVALID)?,
        )
    }
    async fn dispatch<T, F: Future<Output = NativeResult<T>>>(
        &self,
        ticket: &MaintenanceTicket,
        future: F,
    ) -> NativeResult<T> {
        let deadline = ticket
            .authority_deadline()
            .ok_or(RuntimeFailure::LeaseExpired)?;
        if !ticket.is_dispatch() || deadline <= std::time::Instant::now() {
            return Err(RuntimeFailure::LeaseExpired);
        }
        tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), future)
            .await
            .map_err(|_| RuntimeFailure::LifecycleUnknown)?
    }
}
