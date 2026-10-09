//! Task delivery settlement; no simulator steps or offline command are replayed here.
use crate::contract::{DurableOperation, DurableTaskRequest, SumoTaskKind};
use tokio_util::sync::CancellationToken;
use veoveo_task_runtime::{
    RecoveryClass, ResumeCancellationPolicy, TaskError, TaskRuntime, TaskSnapshot, TaskStatus,
    TaskTransition,
};
use veoveo_types::{TaskId, TaskTypeDefinition};

pub(super) async fn update(
    runtime: &TaskRuntime,
    id: TaskId,
    transition: TaskTransition,
    stop: Option<&CancellationToken>,
) -> Result<TaskSnapshot, TaskError> {
    let selected = snapshot(runtime, id).await?;
    settle(runtime, &selected, transition, stop).await
}

pub(super) async fn settle(
    runtime: &TaskRuntime,
    selected: &TaskSnapshot,
    transition: TaskTransition,
    stop: Option<&CancellationToken>,
) -> Result<TaskSnapshot, TaskError> {
    if selected.recovery_class == RecoveryClass::Resume {
        return runtime
            .transition_resumable_if_current(
                selected,
                transition,
                ResumeCancellationPolicy::PreserveFailure,
                stop,
            )
            .await;
    }
    if matches!(transition, TaskTransition::CancelRequested) {
        return Err(TaskError::InvalidRecord(
            "SUMO execution cannot request cancellation".into(),
        ));
    }
    let current = snapshot(runtime, selected.task_id).await?;
    admit(runtime, selected, &current)?;
    if current.is_terminal() {
        return Ok(current);
    }
    lease(runtime, &current)?;
    let selected_transition = outcome(&current, transition.clone());
    if stopped_success(&selected_transition, stop) {
        let observed = snapshot(runtime, selected.task_id).await?;
        admit(runtime, selected, &observed)?;
        if observed.is_terminal() {
            return Ok(observed);
        }
        lease(runtime, &observed)?;
        if observed.status == TaskStatus::CancelRequested {
            return runtime
                .transition_if_current(&observed, TaskTransition::Cancelled)
                .await;
        }
        return Ok(observed);
    }
    let dispatch = if current.status == TaskStatus::CancelRequested {
        &current
    } else {
        selected
    };
    match runtime
        .transition_if_current(dispatch, selected_transition)
        .await
    {
        Ok(settled) => Ok(settled),
        Err(error @ (TaskError::Conflict(_) | TaskError::InvalidTransition { .. })) => {
            let observed = snapshot(runtime, selected.task_id).await?;
            admit(runtime, selected, &observed)?;
            if observed.is_terminal() {
                return Ok(observed);
            }
            lease(runtime, &observed)?;
            if observed.status != TaskStatus::CancelRequested {
                return Err(error);
            }
            runtime
                .transition_if_current(&observed, outcome(&observed, transition))
                .await
        }
        Err(error) => Err(error),
    }
}

async fn snapshot(runtime: &TaskRuntime, id: TaskId) -> Result<TaskSnapshot, TaskError> {
    runtime
        .get(id)
        .await?
        .ok_or_else(|| TaskError::NotFound(id.to_string()))
}

fn outcome(current: &TaskSnapshot, transition: TaskTransition) -> TaskTransition {
    if current.status == TaskStatus::CancelRequested
        && !matches!(transition, TaskTransition::Failed(_))
    {
        TaskTransition::Cancelled
    } else {
        transition
    }
}

fn stopped_success(transition: &TaskTransition, stop: Option<&CancellationToken>) -> bool {
    matches!(transition, TaskTransition::Succeeded { .. })
        && stop.is_some_and(CancellationToken::is_cancelled)
}

fn admit(
    runtime: &TaskRuntime,
    selected: &TaskSnapshot,
    current: &TaskSnapshot,
) -> Result<(), TaskError> {
    if current.server != runtime.server() {
        return Err(TaskError::WrongServer(current.task_id.to_string()));
    }
    if selected.recovery_class != RecoveryClass::InterruptedIndeterminate
        || current.recovery_class != RecoveryClass::InterruptedIndeterminate
        || selected.task_type != SumoTaskKind::RunBatch.name()
    {
        return Err(TaskError::InvalidRecord(
            "SUMO non-resumable settlement requires RunBatch InterruptedIndeterminate".into(),
        ));
    }
    let request: DurableTaskRequest =
        serde_json::from_value(selected.request.clone()).map_err(|_| {
            TaskError::InvalidRecord("retained SUMO mutation request is invalid".into())
        })?;
    if !matches!(request.operation, DurableOperation::RunBatch(_)) {
        return Err(TaskError::InvalidRecord(
            "SUMO non-resumable settlement requires RunBatch".into(),
        ));
    }
    if selected.task_id != current.task_id
        || selected.owner != current.owner
        || selected.server != current.server
        || selected.task_type != current.task_type
        || selected.request != current.request
        || selected.created_at != current.created_at
        || selected.ttl_ms != current.ttl_ms
        || selected.poll_interval_ms != current.poll_interval_ms
        || selected.idempotency_key != current.idempotency_key
    {
        return Err(TaskError::Conflict(selected.task_id.to_string()));
    }
    Ok(())
}

fn lease(runtime: &TaskRuntime, current: &TaskSnapshot) -> Result<(), TaskError> {
    if current.lease_owner.as_deref() != Some(runtime.worker_id())
        || current
            .lease_expires_at
            .is_none_or(|expiry| expiry <= chrono::Utc::now())
    {
        return Err(TaskError::LeaseHeld(current.task_id.to_string()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../../../testing/fixtures/store.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, time::Duration};
    use veoveo_task_runtime::{CreateTask, TaskFailure, TaskOwner};

    fn owner() -> TaskOwner {
        use veoveo_types::{
            AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
            TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
        };
        let principal = PrincipalId::parse("operator").unwrap();
        TaskOwner {
            principal_key: "operator".into(),
            principal_kind: veoveo_task_runtime::PrincipalKind::Service,
            issuer: "https://sumo.test".into(),
            subject: "operator".into(),
            profile: "operator".into(),
            tenant_key: Some("tenant-a".into()),
            data_labels: BTreeSet::new(),
            authority: InvocationAuthority {
                work_context: WorkContextId::parse("sumo-test").unwrap(),
                tenant: TenantId::parse("tenant-a").unwrap(),
                membership: WorkContextMembershipLevel::Contributor,
                policy_revision: PolicyVersion::parse("test-1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(principal.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: principal,
                },
            },
        }
    }

    async fn running(runtime: &TaskRuntime, operation: DurableOperation) -> TaskSnapshot {
        let task_type = super::super::operation_name(&operation);
        let recovery_class = if matches!(operation, DurableOperation::RunBatch(_)) {
            RecoveryClass::InterruptedIndeterminate
        } else {
            RecoveryClass::Resume
        };
        let owner = owner();
        let id = TaskId::new();
        runtime
            .create(CreateTask {
                task_id: id,
                owner,
                server: "sumo".into(),
                task_type,
                request: serde_json::to_value(DurableTaskRequest {
                    operation,
                    artifact_write_capability: None,
                    data_labels: BTreeSet::new(),
                })
                .unwrap(),
                recovery_class,
                idempotency_key: None,
                ttl_ms: None,
                poll_interval_ms: None,
                retention_pins: BTreeSet::new(),
            })
            .await
            .unwrap();
        runtime
            .claim(id, Duration::from_secs(60))
            .await
            .unwrap()
            .snapshot
    }

    fn batch() -> DurableOperation {
        DurableOperation::RunBatch(crate::contract::RunBatchRequest { steps: 1 })
    }
    fn success() -> TaskTransition {
        TaskTransition::Succeeded {
            message: "delivery-only native result".into(),
            result: serde_json::json!({"content":[]}),
            result_uri: None,
        }
    }

    #[tokio::test]
    async fn sumo_worker_admission_and_mutation_settlement_use_current_task_fences() {
        tokio::time::timeout(Duration::from_secs(90), async {
            let db = fixture::TestDb::new().await;
            let worker = TaskRuntime::new(db.a.clone(), "sumo", "executor");
            let remote = TaskRuntime::new(db.b.clone(), "sumo", "remote-cancel");
            let offline = crate::contract::OfflineOperationRequest {
                kind: "grid".into(),
                seed: 1,
            };
            for operation in [
                batch(),
                DurableOperation::GenerateNetwork(offline.clone()),
                DurableOperation::ComputeRoutes(offline.clone()),
                DurableOperation::OptimizeSignals(offline),
            ] {
                let selected = running(&worker, operation).await;
                remote.cancel(selected.task_id).await.unwrap();
                let mut entered = false;
                let admitted = update(
                    &worker,
                    selected.task_id,
                    TaskTransition::Running {
                        message: "initial owner checkpoint".into(),
                        progress: 0.1,
                    },
                    None,
                )
                .await
                .unwrap();
                if admitted.status == TaskStatus::Running {
                    async {
                        entered = true;
                    }
                    .await;
                }
                assert!(!entered);
                assert_eq!(admitted.status, TaskStatus::Cancelled);
            }
            // The production recovery claim stage proves a healthy handoff before
            // any owner worker, command or simulator continuation can be scheduled.
            let retained = running(
                &worker,
                DurableOperation::GenerateNetwork(crate::contract::OfflineOperationRequest {
                    kind: "grid".into(),
                    seed: 2,
                }),
            )
            .await;
            let mut mismatched = retained.clone();
            mismatched.task_type = SumoTaskKind::ComputeRoutes.name();
            let mut entered = false;
            if super::super::admit_resume(&mismatched).is_ok() {
                async {
                    entered = true;
                }
                .await;
            }
            assert!(!entered);
            assert_eq!(
                worker.get(retained.task_id).await.unwrap().unwrap(),
                retained
            );
            mismatched = retained.clone();
            mismatched.recovery_class = RecoveryClass::InterruptedIndeterminate;
            assert!(super::super::admit_resume(&mismatched).is_err());
            assert!(super::super::admit_resume(&retained).is_ok());
            let mut entered = false;
            if super::super::claim_recovered(&remote, &retained)
                .await
                .unwrap()
                .is_some()
            {
                async {
                    entered = true;
                }
                .await;
            }
            assert!(!entered);
            worker
                .transition_resumable(
                    retained.task_id,
                    success(),
                    ResumeCancellationPolicy::PreserveFailure,
                    None,
                )
                .await
                .unwrap();
            assert!(
                super::super::claim_recovered(&remote, &retained)
                    .await
                    .unwrap()
                    .is_none()
            );
            let mut forged = retained.clone();
            forged.request = serde_json::to_value(DurableTaskRequest {
                operation: batch(),
                artifact_write_capability: None,
                data_labels: BTreeSet::new(),
            })
            .unwrap();
            assert!(
                super::super::claim_recovered(&remote, &forged)
                    .await
                    .is_err()
            );

            // This is Task delivery control only: no simulator step or command ran.
            let selected = running(&worker, batch()).await;
            remote.cancel(selected.task_id).await.unwrap();
            assert!(matches!(
                worker.transition_if_current(&selected, success()).await,
                Err(TaskError::Conflict(_))
            ));
            let cancelled = settle(&worker, &selected, success(), None).await.unwrap();
            assert_eq!(cancelled.status, TaskStatus::Cancelled);
            assert!(cancelled.result.is_none());
            assert_eq!(
                settle(&worker, &selected, success(), None)
                    .await
                    .unwrap()
                    .status,
                TaskStatus::Cancelled
            );

            let mut forged = selected.clone();
            forged.ttl_ms = Some(1);
            assert!(matches!(
                settle(&worker, &forged, success(), None).await,
                Err(TaskError::Conflict(_))
            ));

            let selected = running(&worker, batch()).await;
            remote.cancel(selected.task_id).await.unwrap();
            let failure = TaskTransition::Failed(TaskFailure::new(
                "known_failure",
                "native failure before simulator dispatch",
            ));
            assert!(matches!(
                settle(&remote, &selected, failure.clone(), None).await,
                Err(TaskError::LeaseHeld(_))
            ));
            let failed = settle(&worker, &selected, failure, None).await.unwrap();
            assert_eq!(failed.status, TaskStatus::Failed);
            assert_eq!(failed.error.unwrap().code, "known_failure");
            assert_eq!(
                settle(&worker, &selected, success(), None)
                    .await
                    .unwrap()
                    .status,
                TaskStatus::Failed
            );

            let selected = running(&worker, batch()).await;
            let token = CancellationToken::new();
            token.cancel();
            let stopped = settle(&worker, &selected, success(), Some(&token))
                .await
                .unwrap();
            assert_eq!(stopped.status, TaskStatus::Running);
            assert!(stopped.result.is_none());
            remote.cancel(selected.task_id).await.unwrap();
            assert_eq!(
                settle(&worker, &selected, success(), Some(&token))
                    .await
                    .unwrap()
                    .status,
                TaskStatus::Cancelled
            );
        })
        .await
        .expect("SUMO Task-only native controls exceeded 90 seconds");
    }
}
