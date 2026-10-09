use super::*;
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;
use crate::{CreateTask, PrincipalKind, TaskOwner};
use std::{collections::BTreeSet, time::Duration};
use veoveo_types::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

async fn selected(runtime: &TaskRuntime) -> TaskSnapshot {
    let principal = PrincipalId::parse("resume-owner").unwrap();
    let task = runtime
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: TaskOwner {
                principal_key: "resume-owner".into(),
                principal_kind: PrincipalKind::User,
                issuer: "https://resume.test".into(),
                subject: "resume-owner".into(),
                profile: "operator".into(),
                tenant_key: Some("resume-tenant".into()),
                data_labels: BTreeSet::new(),
                authority: InvocationAuthority {
                    work_context: WorkContextId::parse("resume-work").unwrap(),
                    tenant: TenantId::parse("resume-tenant").unwrap(),
                    membership: WorkContextMembershipLevel::Owner,
                    policy_revision: PolicyVersion::parse("r1").unwrap(),
                    output_policy: WorkContextOutputPolicy {
                        owner: AccessSubject::Principal(principal.clone()),
                        initial_grants: vec![],
                        classification: None,
                        data_labels: BTreeSet::new(),
                    },
                    provenance: InvocationProvenance::Direct {
                        initiator: principal,
                    },
                },
            },
            server: "resume-test".into(),
            task_type: "calculation".parse().unwrap(),
            request: serde_json::json!({"input": 7}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(task.task_id, Duration::from_secs(60))
        .await
        .unwrap()
        .snapshot
}
fn success() -> TaskTransition {
    TaskTransition::Succeeded {
        message: "complete".into(),
        result: serde_json::json!({"answer":7}),
        result_uri: None,
    }
}

#[tokio::test]
async fn final_dispatch_stop_uses_admitted_cancellation_and_current_progress_without_advancing_cas()
{
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let a = TaskRuntime::new(db.a.clone(), "resume-test", "worker-a");
        let b = TaskRuntime::new(db.b.clone(), "resume-test", "worker-b");
        let selected = selected(&a).await;
        b.cancel(selected.task_id).await.unwrap();
        let admitted = a.get(selected.task_id).await.unwrap().unwrap();
        // Exact final-dispatch seam: stop arrives after durable admission.
        let stop = CancellationToken::new();
        stop.cancel();
        let cancelled = a
            .resume_dispatch(
                &selected,
                &admitted,
                success(),
                ResumeCancellationPolicy::PreserveFailure,
                Some(&stop),
            )
            .await
            .unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        assert_eq!(cancelled.request, selected.request);
        assert!(cancelled.result.is_none());

        let selected = self::selected(&a).await;
        let admitted = a
            .transition(
                selected.task_id,
                TaskTransition::Running {
                    message: "new progress".into(),
                    progress: 0.5,
                },
            )
            .await
            .unwrap();
        // Normal execution retains the selected version and must lose this CAS.
        assert!(matches!(
            a.resume_dispatch(
                &selected,
                &admitted,
                success(),
                ResumeCancellationPolicy::CancellationWins,
                None
            )
            .await,
            Err(TaskError::Conflict(_))
        ));
        let unchanged = a
            .resume_dispatch(
                &selected,
                &admitted,
                success(),
                ResumeCancellationPolicy::CancellationWins,
                Some(&stop),
            )
            .await
            .unwrap();
        assert_eq!(unchanged, admitted);
        assert_eq!(a.get(selected.task_id).await.unwrap().unwrap(), admitted);
    })
    .await
    .expect("Resume final dispatch seam exceeded 90 seconds");
}
