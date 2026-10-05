use super::*;
use std::time::Duration;
use uuid::Uuid;
use veoveo_workspace::persistence::WorkspaceRepository;
use veoveo_workspace::persistence::{
    WorkspaceOperationId, WorkspaceOperationIntent, WorkspaceOperationOutcome as Outcome,
    WorkspaceOperationPhase as Phase,
};

fn intent(chat: WorkspaceChatId) -> WorkspaceOperationIntent {
    WorkspaceOperationIntent {
        chat,
        run: None,
        profile: "workspace".into(),
        app_uri: None,
        tool: "computers_create".into(),
        arguments: r#"{"name":"Research","template":"python"}"#.into(),
    }
}

#[tokio::test]
async fn app_task_origin_is_durable_private_and_part_of_idempotency() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
            veoveo_workspace::schema::module_setup(
                fixture::module_lanes::execution("workspace").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "alice").await;
        let bob = identity(&db.a, "bob").await;
        context(&db.a, &alice, "operations").await;
        let a = authority(&db.a, &alice, "operations").await;
        let b = authority(&db.b, &bob, "operations").await;
        let chat = WorkspaceChatId::new();
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "App tasks")
            .await
            .unwrap();
        join(&db.a, &a, &b, &bob, chat).await;
        let app = "ui://media/create.html";
        let id = WorkspaceOperationId::new();
        let started = WorkspaceRepository::new(db.a.clone())
            .start_workspace_operation(
                &a,
                id,
                WorkspaceOperationIntent {
                    app_uri: Some(app.into()),
                    ..intent(chat)
                },
            )
            .await
            .unwrap();
        let settled = WorkspaceRepository::new(db.a.clone())
            .settle_workspace_operation(
                id,
                started.operation.fence,
                Outcome::Task("opaque-app-task".into()),
            )
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_app_task(&a, "workspace", app, "opaque-app-task")
                .await
                .unwrap(),
            settled
        );
        for (actor, profile, uri, task) in [
            (&b, "workspace", app, "opaque-app-task"),
            (&a, "operator", app, "opaque-app-task"),
            (&a, "workspace", "ui://media/other.html", "opaque-app-task"),
            (&a, "workspace", app, "another-task"),
        ] {
            assert_eq!(
                WorkspaceRepository::new(db.b.clone())
                    .workspace_app_task(actor, profile, uri, task)
                    .await,
                Err(WorkspaceError::NotFound)
            );
        }
        assert!(matches!(
            WorkspaceRepository::new(db.b.clone())
                .start_workspace_operation(&a, id, intent(chat))
                .await,
            Err(WorkspaceError::Conflict)
        ));
        let replay = WorkspaceRepository::new(db.b.clone())
            .start_workspace_operation(
                &a,
                id,
                WorkspaceOperationIntent {
                    app_uri: Some(app.into()),
                    ..intent(chat)
                },
            )
            .await
            .unwrap();
        assert!(!replay.dispatch);
        let plain_id = WorkspaceOperationId::new();
        let plain = WorkspaceRepository::new(db.a.clone())
            .start_workspace_operation(&a, plain_id, intent(chat))
            .await
            .unwrap();
        WorkspaceRepository::new(db.a.clone())
            .settle_workspace_operation(
                plain_id,
                plain.operation.fence,
                Outcome::Task("human-task".into()),
            )
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_app_task(&a, "workspace", app, "human-task")
                .await,
            Err(WorkspaceError::NotFound)
        );
        // No second App-specific Task store: the normal personal activity sees it.
        assert!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operations(&a, Some(chat), None)
                .await
                .unwrap()
                .iter()
                .any(|operation| operation == &settled)
        );
    })
    .await
    .expect("bounded App receipt acceptance");
}

#[tokio::test]
async fn operation_claims_are_private_durable_and_never_replay_unknown_dispatch() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
            veoveo_workspace::schema::module_setup(
                fixture::module_lanes::execution("workspace").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "alice").await;
        let bob = identity(&db.a, "bob").await;
        context(&db.a, &alice, "operations").await;
        let a = authority(&db.a, &alice, "operations").await;
        let b = authority(&db.b, &bob, "operations").await;
        let chat = WorkspaceChatId::new();
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "Private work in a shared chat")
            .await
            .unwrap();
        join(&db.a, &a, &b, &bob, chat).await;
        let id = WorkspaceOperationId::new();
        let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
        let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
        let (left, right) = tokio::join!(
            workspace_repository_a.start_workspace_operation(&a, id, intent(chat)),
            workspace_repository_b.start_workspace_operation(&a, id, intent(chat)),
        );
        let left = left.unwrap();
        let right = right.unwrap();
        assert_ne!(
            left.dispatch, right.dispatch,
            "only one caller may send tools/call"
        );
        assert_eq!(left.operation, right.operation);
        WorkspaceRepository::new(db.a.clone())
            .check_workspace_operation_dispatch(&a, id, left.operation.fence, None)
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .check_workspace_operation_dispatch(&a, id, Uuid::new_v4(), None)
                .await,
            Err(WorkspaceError::Conflict)
        );
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operation(&b, id)
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operations(&b, Some(chat), None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            WorkspaceRepository::new(db.b.clone())
                .start_workspace_operation(&b, id, intent(chat))
                .await,
            Err(WorkspaceError::NotFound)
        ));
        assert!(matches!(
            WorkspaceRepository::new(db.a.clone())
                .start_workspace_operation(
                    &a,
                    id,
                    WorkspaceOperationIntent {
                        arguments: "{}".into(),
                        ..intent(chat)
                    }
                )
                .await,
            Err(WorkspaceError::Conflict)
        ));
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .settle_workspace_operation(id, Uuid::new_v4(), Outcome::Task("wrong".into()))
                .await,
            Err(WorkspaceError::Conflict)
        );

        // This is a timed-out client wait, not an assertion that the MCP action failed.
        db.a.client()
            .query(include_str!("../queries/workspace/operations/operation_claims_are_private_durable_and_never_replay_unknown_dispatch/statement_1.surql"))
            .bind(("id", id.record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operation(&a, id)
                .await
                .unwrap()
                .phase,
            Phase::Unconfirmed
        );
        let repeated = WorkspaceRepository::new(db.b.clone())
            .start_workspace_operation(&a, id, intent(chat))
            .await
            .unwrap();
        assert!(!repeated.dispatch);
        assert_eq!(repeated.operation.phase, Phase::Unconfirmed);

        // A late definitive response remains recordable after the wait expired.
        let accepted = WorkspaceRepository::new(db.a.clone())
            .settle_workspace_operation(
                id,
                left.operation.fence,
                Outcome::Task("opaque/native/task?key=example".into()),
            )
            .await
            .unwrap();
        assert_eq!(accepted.phase, Phase::Task);
        assert!(accepted.agent.is_none());
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operation(&a, id)
                .await
                .unwrap(),
            accepted
        );
        assert!(
            !WorkspaceRepository::new(db.b.clone())
                .start_workspace_operation(&a, id, intent(chat))
                .await
                .unwrap()
                .dispatch
        );
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .settle_workspace_operation(id, left.operation.fence, Outcome::Failed)
                .await,
            Err(WorkspaceError::Conflict)
        );

        // Leaving the room removes history, but does not orphan one's private Task receipt.
        let agent = WorkspaceRepository::new(db.a.clone())
            .add_workspace_agent(
                &a,
                chat,
                uuid::Uuid::now_v7(),
                runs::admission("Researcher"),
            )
            .await
            .unwrap();
        let agent_id = runs::agent_id(&agent.id);
        let deadline = chrono::Utc::now() + chrono::TimeDelta::seconds(120);
        let turn = WorkspaceRepository::new(db.b.clone())
            .send_workspace_turn(
                &b,
                chat,
                veoveo_workspace::persistence::WorkspaceTurnRequest {
                    id: WorkspaceMessageId::new(),
                    text: "Research this".into(),
                    reply_to: None,
                    attachments: vec![],
                    addressed_agents: vec![agent_id],
                    deadline,
                },
            )
            .await
            .unwrap();
        let run_id = runs::run_id(&turn.runs[0]);
        let fence = Uuid::new_v4();
        WorkspaceRepository::new(db.b.clone())
            .claim_workspace_run(&b, chat, run_id, fence)
            .await
            .unwrap();
        let own = WorkspaceOperationId::new();
        let mut agent_intent = intent(chat);
        agent_intent.run = Some((run_id, fence));
        let admitted = WorkspaceRepository::new(db.b.clone())
            .start_workspace_operation(&b, own, agent_intent)
            .await
            .unwrap();
        let attribution = admitted.operation.agent.clone().unwrap();
        assert_eq!(attribution.id, agent.id);
        assert_eq!(attribution.name, "Researcher");
        WorkspaceRepository::new(db.a.clone())
            .remove_workspace_agent(&a, chat, agent_id)
            .await
            .unwrap();
        WorkspaceRepository::new(db.a.clone())
            .remove_workspace_member(&a, chat, bob.principal_id)
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .check_workspace_operation_dispatch(&b, own, admitted.operation.fence, None)
                .await,
            Err(WorkspaceError::NotFound)
        );
        WorkspaceRepository::new(db.a.clone())
            .settle_workspace_operation(
                own,
                admitted.operation.fence,
                Outcome::Task("bob-task".into()),
            )
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_snapshot(&b, chat, 0, 100)
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operation(&b, own)
                .await
                .unwrap()
                .agent,
            Some(attribution.clone())
        );
        let personal = WorkspaceRepository::new(db.b.clone())
            .workspace_operations(&b, None, None)
            .await
            .unwrap();
        assert_eq!(personal.len(), 1);
        assert_eq!(personal[0].agent, Some(attribution));
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .workspace_operation(&a, own)
                .await,
            Err(WorkspaceError::NotFound)
        );
        assert!(matches!(
            WorkspaceRepository::new(db.b.clone())
                .start_workspace_operation(&b, WorkspaceOperationId::new(), intent(chat))
                .await,
            Err(WorkspaceError::NotFound)
        ));

        // Revocation prevents reads, while the original receipt writer can still
        // retain an already observed Task ID for later authorized recovery.
        let pending = WorkspaceOperationId::new();
        let admitted = WorkspaceRepository::new(db.a.clone())
            .start_workspace_operation(&a, pending, intent(chat))
            .await
            .unwrap();
        let older = WorkspaceRepository::new(db.b.clone())
            .workspace_operations(&a, None, Some(pending))
            .await
            .unwrap();
        assert_eq!(older.len(), 1);
        assert_eq!(older[0].id, id.record_id());
        assert_eq!(
            WorkspaceRepository::new(db.b.clone())
                .workspace_operations(&b, None, Some(pending))
                .await,
            Err(WorkspaceError::NotFound)
        );
        db.a.client()
            .query(include_str!("../queries/workspace/operations/operation_claims_are_private_durable_and_never_replay_unknown_dispatch/statement_2.surql"))
            .bind(("context", a.work_context.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        WorkspaceRepository::new(db.b.clone())
            .settle_workspace_operation(
                pending,
                admitted.operation.fence,
                Outcome::Task("retained-after-revocation".into()),
            )
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .workspace_operation(&a, pending)
                .await,
            Err(WorkspaceError::Forbidden)
        );
    })
    .await
    .expect("bounded native operation receipt acceptance");
}

#[tokio::test]
async fn input_rounds_claim_one_revision_and_stale_forms_cannot_advance_them() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = TestDb::with_modules(vec![
            veoveo_agent_runtime::schema::module_setup(
                fixture::module_lanes::execution("agents").unwrap(),
            )
            .unwrap(),
            veoveo_workspace::schema::module_setup(
                fixture::module_lanes::execution("workspace").unwrap(),
            )
            .unwrap(),
        ])
        .await;
        let alice = identity(&db.a, "alice").await;
        context(&db.a, &alice, "operations").await;
        let a = authority(&db.a, &alice, "operations").await;
        let chat = WorkspaceChatId::new();
        WorkspaceRepository::new(db.a.clone())
            .create_workspace_chat(&a, chat, "Input rounds")
            .await
            .unwrap();
        let id = WorkspaceOperationId::new();
        let started = WorkspaceRepository::new(db.a.clone())
            .start_workspace_operation(&a, id, intent(chat))
            .await
            .unwrap();
        let input = WorkspaceRepository::new(db.a.clone())
            .settle_workspace_operation(
                id,
                started.operation.fence,
                Outcome::InputRequired(r#"{"requestState":"opaque","inputRequests":{}}"#.into()),
            )
            .await
            .unwrap();
        let workspace_repository_a = WorkspaceRepository::new(db.a.clone());
        let workspace_repository_b = WorkspaceRepository::new(db.b.clone());
        let (left, right) = tokio::join!(
            workspace_repository_a.resume_workspace_operation(&a, id, input.revision),
            workspace_repository_b.resume_workspace_operation(&a, id, input.revision),
        );
        assert_ne!(left.is_ok(), right.is_ok());
        let resumed = left.or(right).unwrap();
        assert_eq!(resumed.phase, Phase::Dispatching);
        assert_eq!(resumed.round, 1);
        assert_ne!(resumed.fence, input.fence);
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .settle_workspace_operation(id, input.fence, Outcome::Failed)
                .await,
            Err(WorkspaceError::Conflict)
        );
        let next = WorkspaceRepository::new(db.a.clone())
            .settle_workspace_operation(
                id,
                resumed.fence,
                Outcome::InputRequired(
                    r#"{"requestState":"new opaque state","inputRequests":{}}"#.into(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            WorkspaceRepository::new(db.a.clone())
                .resume_workspace_operation(&a, id, input.revision)
                .await,
            Err(WorkspaceError::Conflict)
        );
        let resumed = WorkspaceRepository::new(db.b.clone())
            .resume_workspace_operation(&a, id, next.revision)
            .await
            .unwrap();
        let completed = WorkspaceRepository::new(db.b.clone())
            .settle_workspace_operation(
                id,
                resumed.fence,
                Outcome::Completed(r#"{"content":[],"isError":true}"#.into()),
            )
            .await
            .unwrap();
        assert_eq!(
            completed.phase,
            Phase::Completed,
            "a domain rejection is a completed MCP response"
        );
        assert!(
            !WorkspaceRepository::new(db.a.clone())
                .start_workspace_operation(&a, id, intent(chat))
                .await
                .unwrap()
                .dispatch
        );
    })
    .await
    .expect("bounded native MRTR fencing acceptance");
}

#[tokio::test]
async fn progress_is_monotonic_and_cannot_cross_dispatch_fences_or_settlement() {
    use veoveo_workspace::persistence::WorkspaceOperationProgress;
    let db = TestDb::with_modules(vec![
        veoveo_agent_runtime::schema::module_setup(
            fixture::module_lanes::execution("agents").unwrap(),
        )
        .unwrap(),
        veoveo_workspace::schema::module_setup(
            fixture::module_lanes::execution("workspace").unwrap(),
        )
        .unwrap(),
    ])
    .await;
    let alice = identity(&db.a, "alice").await;
    context(&db.a, &alice, "operations").await;
    let a = authority(&db.a, &alice, "operations").await;
    let chat = WorkspaceChatId::new();
    WorkspaceRepository::new(db.a.clone())
        .create_workspace_chat(&a, chat, "Measured progress")
        .await
        .unwrap();
    let id = WorkspaceOperationId::new();
    let op = WorkspaceRepository::new(db.a.clone())
        .start_workspace_operation(&a, id, intent(chat))
        .await
        .unwrap()
        .operation;
    let measured = |completed| WorkspaceOperationProgress {
        completed,
        total: Some(10.0),
        message: Some("Measured work".into()),
    };
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .record_workspace_progress(id, Uuid::new_v4(), measured(1.0))
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.a.clone())
        .record_workspace_progress(id, op.fence, measured(4.0))
        .await
        .unwrap();
    WorkspaceRepository::new(db.b.clone())
        .record_workspace_progress(id, op.fence, measured(2.0))
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_operation(&a, id)
            .await
            .unwrap()
            .progress,
        Some(measured(4.0))
    );
    let settled = WorkspaceRepository::new(db.a.clone())
        .settle_workspace_operation(id, op.fence, Outcome::InputRequired("{}".into()))
        .await
        .unwrap();
    assert!(settled.progress.is_none());
    let resumed = WorkspaceRepository::new(db.b.clone())
        .resume_workspace_operation(&a, id, settled.revision)
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.a.clone())
            .record_workspace_progress(id, op.fence, measured(5.0))
            .await,
        Err(WorkspaceError::Conflict)
    );
    WorkspaceRepository::new(db.b.clone())
        .record_workspace_progress(id, resumed.fence, measured(1.0))
        .await
        .unwrap();
    WorkspaceRepository::new(db.a.clone())
        .settle_workspace_operation(id, resumed.fence, Outcome::Failed)
        .await
        .unwrap();
    assert_eq!(
        WorkspaceRepository::new(db.b.clone())
            .record_workspace_progress(id, resumed.fence, measured(6.0))
            .await,
        Err(WorkspaceError::Conflict)
    );
    assert!(
        WorkspaceRepository::new(db.b.clone())
            .workspace_operation(&a, id)
            .await
            .unwrap()
            .progress
            .is_none()
    );
}
