mod support;

use std::time::Duration;
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, ComputersStore, ObservationAdmission, Operation, OperationStage,
    ReachedPhase, ReachedState, UndispatchedOutcome, api::*,
};
use veoveo_task_runtime::TaskRuntime;

async fn grant(
    store: &ComputersStore,
    owner: &ComputerActor,
    computer: Uuid,
) -> AutomationGrantView {
    let mut input = support::automation::input(computer);
    input.permissions = [
        AutomationPermission::Read,
        AutomationPermission::Start,
        AutomationPermission::Stop,
    ]
    .into();
    input.execution_limits = None;
    store.issue_automation_grant(owner, &input).await.unwrap()
}

async fn queue(
    store: &ComputersStore,
    agent: &ComputerActor,
    computer: Uuid,
    grant: Uuid,
    request: Uuid,
    action: Action,
) -> Result<Operation, ComputerError> {
    let permission = match action {
        Action::Start => AutomationPermission::Start,
        Action::Stop => AutomationPermission::Stop,
        Action::Create => AutomationPermission::Read,
    };
    let authority = store
        .authorize_automation_grant(agent, computer, grant, permission)
        .await?;
    store
        .queue_automation_operation(agent, authority, request, action)
        .await
}

fn reached(operation: &Operation, phase: ReachedPhase, process: &str) -> ReachedState {
    ReachedState {
        provider_instance_id: operation.provider_instance_id,
        computer_id: operation.computer_id,
        replacement_instance_id: operation.replacement_instance_id,
        template_fingerprint: operation.template_fingerprint.clone(),
        resource_id: operation.previous_resource_id.clone().unwrap(),
        process_id: process.into(),
        phase,
    }
}

#[tokio::test]
async fn accepted_lifecycle_outlives_the_source_token_but_not_the_named_grant() {
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    let granted = grant(&store, &owner, computer).await;
    let mut identity = support::identity(agent.owner());
    identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .expires_at = chrono::Utc::now() + chrono::TimeDelta::seconds(2);
    let short = ComputerActor::from_verified(&identity).unwrap();
    let operation = queue(
        &store,
        &short,
        computer,
        granted.grant_id,
        Uuid::now_v7(),
        Action::Stop,
    )
    .await
    .unwrap();
    store
        .ensure_automation_operation_task(&short, operation.operation_id)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(2100)).await;
    assert!(
        store
            .authorize_operation_task(&short, operation.operation_id, false)
            .await
            .is_err()
    );
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "independent-worker");
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(store.begin_dispatch(&claim).await.is_ok());
}

#[tokio::test]
async fn prior_owner_only_rows_migrate_without_inventing_a_delegation() {
    let db = support::TestDb::new().await;
    let (store, _, owner, _, computer) = support::automation::setup(&db).await;
    let operation = store
        .queue_operation(
            support::authenticated(owner.owner()),
            computer,
            Uuid::now_v7(),
            Action::Stop,
        )
        .await
        .unwrap();
    // Reconstruct the prior private schema in this disposable database only.
    db.a.client().query("REMOVE FIELD owner_context ON computer_operation; REMOVE FIELD automation_grant_id ON computer_operation; UPDATE computer_operation UNSET owner_context, automation_grant_id;").await.unwrap().check().unwrap();
    db.a.client()
        .query(include_str!(
            "../../store/migrations/0076_computer_lifecycle_delegation.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    let migrated = store
        .operation(owner.owner(), operation.operation_id)
        .await
        .unwrap();
    assert_eq!(migrated.actor, operation.actor);
    assert_eq!(migrated.owner, operation.actor);
    assert_eq!(migrated.automation_grant_id, None);
    store
        .ensure_operation_task(owner.owner(), operation.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "migrated-worker");
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(store.begin_dispatch(&claim).await.is_ok());
}

#[tokio::test]
async fn an_owner_cannot_turn_a_granted_retry_into_an_ungranted_request() {
    let db = support::TestDb::new().await;
    let (store, _, owner, _, computer) = support::automation::setup(&db).await;
    let mut input = support::automation::input(computer);
    input.principal_id = owner.owner().principal_key.clone();
    input.oauth_client_id = "console".into();
    input.permissions = [AutomationPermission::Stop].into();
    input.execution_limits = None;
    let granted = store.issue_automation_grant(&owner, &input).await.unwrap();
    let request = Uuid::now_v7();
    queue(
        &store,
        &owner,
        computer,
        granted.grant_id,
        request,
        Action::Stop,
    )
    .await
    .unwrap();
    assert!(matches!(
        store
            .operation_for_request(owner.owner(), computer, request, Action::Stop)
            .await,
        Err(ComputerError::RequestConflict)
    ));
    assert!(matches!(
        store
            .queue_operation(
                support::authenticated(owner.owner()),
                computer,
                request,
                Action::Stop
            )
            .await,
        Err(ComputerError::RequestConflict)
    ));
}

#[tokio::test]
async fn queued_agent_work_rechecks_both_principals_and_policy() {
    for mode in 0..3 {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
        let granted = grant(&store, &owner, computer).await;
        let operation = queue(
            &store,
            &agent,
            computer,
            granted.grant_id,
            Uuid::now_v7(),
            Action::Stop,
        )
        .await
        .unwrap();
        store
            .ensure_automation_operation_task(&agent, operation.operation_id)
            .await
            .unwrap();
        if mode < 2 {
            let principal = if mode == 0 {
                owner.owner()
            } else {
                agent.owner()
            };
            let record = veoveo_platform_store::deterministic_principal_id(
                principal.tenant_key(),
                &principal.principal_key,
            )
            .unwrap()
            .record_id();
            db.a.client()
                .query("UPDATE ONLY $principal SET enabled = false;")
                .bind(("principal", record))
                .await
                .unwrap()
                .check()
                .unwrap();
        } else {
            let mut policy = support::automation::control();
            policy.policies[0].rules[0].effect = veoveo_mcp_contract::PolicyEffect::Deny;
            support::policy::install(&db.a, policy).await;
        }
        let tasks = TaskRuntime::new(db.a.clone(), "computers", "denied-worker");
        let claim = tasks
            .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        assert!(
            matches!(
                store.begin_dispatch(&claim).await,
                Err(ComputerError::Forbidden)
            ),
            "mode {mode}"
        );
        assert_eq!(
            store.operation_for_claim(&claim).await.unwrap().stage,
            OperationStage::Queued
        );
    }
}

#[tokio::test]
async fn agent_stop_start_preserve_owner_and_current_named_dispatch_evidence() {
    let db = support::TestDb::new().await;
    let (store, replica, owner, agent, computer) = support::automation::setup(&db).await;
    let granted = grant(&store, &owner, computer).await;
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "lifecycle-worker");
    for action in [Action::Stop, Action::Start] {
        let request = Uuid::now_v7();
        let (left, right) = futures::join!(
            queue(&store, &agent, computer, granted.grant_id, request, action),
            queue(
                &replica,
                &agent,
                computer,
                granted.grant_id,
                request,
                action
            ),
        );
        let operation = left.unwrap();
        assert_eq!(operation.operation_id, right.unwrap().operation_id);
        assert_eq!(operation.actor, *agent.owner());
        assert_eq!(operation.owner, *owner.owner());
        assert_eq!(operation.automation_grant_id, Some(granted.grant_id));
        assert!(
            store
                .operation(agent.owner(), operation.operation_id)
                .await
                .is_err()
        );
        assert!(
            store
                .operation(owner.owner(), operation.operation_id)
                .await
                .is_ok()
        );
        store
            .ensure_automation_operation_task(&agent, operation.operation_id)
            .await
            .unwrap();
        let task = tasks
            .get(&operation.task_id().to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.owner, *agent.owner());
        let claim = tasks
            .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        let ticket = store.begin_dispatch(&claim).await.unwrap();
        let decision = ticket.operation().dispatch_authority.as_ref().unwrap();
        assert_eq!(
            decision.decision.principal.as_ref().unwrap().as_str(),
            agent.owner().principal_key
        );
        let delegation = decision.automation.as_ref().unwrap();
        assert_eq!(delegation.grant_id, granted.grant_id);
        assert_eq!(
            delegation.owner.principal.as_ref().unwrap().as_str(),
            owner.owner().principal_key
        );
        assert!(replica.begin_dispatch(&claim).await.is_err());
        let phase = if action == Action::Stop {
            ReachedPhase::Stopped
        } else {
            ReachedPhase::Ready
        };
        let process = if action == Action::Stop {
            "fixture-process"
        } else {
            "restarted-process"
        };
        let completed = store
            .complete_dispatch(&claim, ticket, reached(&operation, phase, process))
            .await
            .unwrap();
        assert_eq!(completed.stage, OperationStage::Succeeded);
        let retained = store.get(owner.owner(), computer).await.unwrap();
        assert_eq!(retained.owner, *owner.owner());
        assert!(retained.active_operation.is_none());
        let retry = queue(
            &replica,
            &agent,
            computer,
            granted.grant_id,
            request,
            action,
        )
        .await
        .unwrap();
        assert_eq!(retry.operation_id, operation.operation_id);
    }
}

#[tokio::test]
async fn revocation_fences_admission_and_dispatch_but_preserves_owner_recovery() {
    let db = support::TestDb::new().await;
    let (store, replica, owner, agent, computer) = support::automation::setup(&db).await;
    let granted = grant(&store, &owner, computer).await;
    let authority = store
        .authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Stop,
        )
        .await
        .unwrap();
    store
        .revoke_automation_grant(
            &owner,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: granted.grant_id,
            },
        )
        .await
        .unwrap();
    assert!(
        store
            .queue_automation_operation(&agent, authority, Uuid::now_v7(), Action::Stop)
            .await
            .is_err()
    );
    assert!(
        store
            .get(owner.owner(), computer)
            .await
            .unwrap()
            .active_operation
            .is_none()
    );

    let granted = grant(&store, &owner, computer).await;
    let request = Uuid::now_v7();
    let operation = queue(
        &store,
        &agent,
        computer,
        granted.grant_id,
        request,
        Action::Stop,
    )
    .await
    .unwrap();
    store
        .revoke_automation_grant(
            &owner,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: granted.grant_id,
            },
        )
        .await
        .unwrap();
    assert!(
        store
            .automation_operation(&agent, operation.operation_id)
            .await
            .is_err()
    );
    assert!(
        store
            .authorize_operation_task(&agent, operation.operation_id, false)
            .await
            .is_err()
    );
    assert!(
        store
            .authorize_operation_task(&agent, operation.operation_id, true)
            .await
            .is_err()
    );
    let owner_access = store
        .authorize_operation_task(&owner, operation.operation_id, true)
        .await
        .unwrap();
    assert_eq!(owner_access.operation().unwrap().actor, *agent.owner());
    assert!(
        queue(
            &store,
            &agent,
            computer,
            granted.grant_id,
            request,
            Action::Stop
        )
        .await
        .is_err()
    );
    // Worker repair and owner inspection do not depend on a revoked agent grant.
    replica
        .ensure_operation_task(owner.owner(), operation.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "lifecycle-worker");
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(matches!(
        store.begin_dispatch(&claim).await,
        Err(ComputerError::Forbidden)
    ));
    let aborted = replica
        .abort_undispatched(&claim, UndispatchedOutcome::AuthorityDenied)
        .await
        .unwrap();
    assert_eq!(aborted.stage, OperationStage::Failed);
    assert_eq!(
        store.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::Ready
    );
}

#[tokio::test]
async fn lost_agent_dispatch_can_settle_after_revocation_without_repeating_the_effect() {
    let db = support::TestDb::new().await;
    let (store, replica, owner, agent, computer) = support::automation::setup(&db).await;
    let granted = grant(&store, &owner, computer).await;
    let operation = queue(
        &store,
        &agent,
        computer,
        granted.grant_id,
        Uuid::now_v7(),
        Action::Stop,
    )
    .await
    .unwrap();
    store
        .ensure_automation_operation_task(&agent, operation.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "lifecycle-worker");
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    drop(store.begin_dispatch(&claim).await.unwrap());
    store
        .revoke_automation_grant(
            &owner,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: granted.grant_id,
            },
        )
        .await
        .unwrap();
    assert!(replica.begin_dispatch(&claim).await.is_err());
    let ObservationAdmission::Read(ticket) = replica.admit_observation(&claim).await.unwrap()
    else {
        panic!("expected bounded observation");
    };
    replica
        .complete_observation(
            &claim,
            ticket,
            reached(&operation, ReachedPhase::Stopped, "fixture-process"),
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .operation(owner.owner(), operation.operation_id)
            .await
            .unwrap()
            .stage,
        OperationStage::Succeeded
    );
    assert_eq!(
        store.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::Stopped
    );
}

#[tokio::test]
async fn a_grant_does_not_admit_create_other_computers_other_clients_or_changed_retries() {
    let db = support::TestDb::new().await;
    let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
    let first = grant(&store, &owner, computer).await;
    assert!(
        queue(
            &store,
            &agent,
            computer,
            first.grant_id,
            Uuid::now_v7(),
            Action::Create
        )
        .await
        .is_err()
    );
    assert!(
        queue(
            &store,
            &agent,
            Uuid::now_v7(),
            first.grant_id,
            Uuid::now_v7(),
            Action::Stop
        )
        .await
        .is_err()
    );
    let request = Uuid::now_v7();
    let operation = queue(
        &store,
        &agent,
        computer,
        first.grant_id,
        request,
        Action::Stop,
    )
    .await
    .unwrap();
    let second = grant(&store, &owner, computer).await;
    assert!(matches!(
        queue(
            &store,
            &agent,
            computer,
            second.grant_id,
            request,
            Action::Stop
        )
        .await,
        Err(ComputerError::RequestConflict)
    ));
    assert!(matches!(
        queue(
            &store,
            &agent,
            computer,
            first.grant_id,
            request,
            Action::Start
        )
        .await,
        Err(ComputerError::RequestConflict)
    ));
    let mut wrong = support::identity(agent.owner());
    wrong
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .oauth_client_id = veoveo_mcp_contract::OAuthClientId::new("console").unwrap();
    assert!(ComputerActor::from_verified(&wrong).is_err());
    assert!(
        store
            .automation_operation(&owner, operation.operation_id)
            .await
            .is_err()
    );
}
