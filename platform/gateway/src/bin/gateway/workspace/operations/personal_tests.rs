use super::*;
use axum::response::Response;
use tokio::sync::mpsc;

struct Watch {
    received: mpsc::Receiver<wire::PersonalEvent>,
    reader: tokio::task::JoinHandle<()>,
}
impl Drop for Watch {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
impl Watch {
    async fn open(app: &Router) -> Self {
        let response: Response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/workspace-api/operator/events")
                    .header("authorization", "Bearer explicit-workspace-fixture")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let (send, received) = mpsc::channel(64);
        let reader = tokio::spawn(async move {
            let mut chunks = response.into_body().into_data_stream();
            let mut buffer = String::new();
            while let Some(Ok(bytes)) = chunks.next().await {
                buffer.push_str(std::str::from_utf8(&bytes).unwrap());
                assert!(!buffer.contains("Choose a count for the fixture"));
                assert!(!buffer.contains("PRIVATE TASK RESULT"));
                assert!(!buffer.contains("requestState"));
                while let Some(end) = buffer.find("\n\n") {
                    let frame: String = buffer.drain(..end + 2).collect();
                    if frame.starts_with("event: personal") {
                        let data = frame
                            .lines()
                            .find_map(|line| line.strip_prefix("data: "))
                            .unwrap();
                        if send
                            .send(serde_json::from_str(data).unwrap())
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        });
        Self { received, reader }
    }
    async fn until(&mut self, predicate: impl Fn(&wire::PersonalEvent) -> bool) {
        tokio::time::timeout(Duration::from_secs(8), async {
            while let Some(event) = self.received.recv().await {
                if predicate(&event) {
                    return;
                }
            }
            panic!("personal watch ended before expected update");
        })
        .await
        .expect("bounded personal update");
    }
}

#[tokio::test]
async fn personal_feeds_follow_native_tasks_on_two_replicas_without_private_payloads_or_replay() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::test_store::TestDb::new().await;
        super::super::super::tests::setup(&db.a).await;
        let subject = alice();
        let fixture = super::super::test_domain::Fixture::start(db.a.clone(), &subject).await;
        let state = new_state(db.a.clone(), fixture.port);
        let replica = new_state(db.b.clone(), fixture.port);
        let _stop = state.stop.clone().drop_guard();
        let _replica_stop = replica.stop.clone().drop_guard();
        let actor = state.authority(&subject, &GatewayProfileId::new("operator").unwrap()).await.unwrap();
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&actor, chat, "Private live work").await.unwrap();
        let app = new_app(state.clone());
        let other_replica = new_app(replica.clone());
        let id = Uuid::now_v7();
        assert_eq!(request(&app, "POST", &format!("/chats/{chat}/operations"), json!({"id":id,"tool":"fixture__task","arguments":{}})).await.0, StatusCode::OK);
        let task = detail(&app, id).await.task.unwrap();
        let mut first = Watch::open(&app).await;
        let mut second = Watch::open(&other_replica).await;
        let waiting = |event: &wire::PersonalEvent| matches!(event, wire::PersonalEvent::Task { operation, state: wire::TaskState::InputRequired, .. } if operation.0 == id);
        first.until(waiting).await;
        second.until(waiting).await;
        fixture.domain.runtime.transition(&task.id, TaskTransition::Succeeded { message: "Done".into(), result: serde_json::to_value(CallToolResult::success(vec![ContentBlock::text("PRIVATE TASK RESULT")])).unwrap() }).await.unwrap();
        let done = |event: &wire::PersonalEvent| matches!(event, wire::PersonalEvent::Task { operation, state: wire::TaskState::Completed, .. } if operation.0 == id);
        first.until(done).await;
        second.until(done).await;
        drop(first);
        let mut restored = Watch::open(&app).await;
        restored.until(done).await;
        let mut bob = super::super::super::tests::subject("Bob");
        bob.access_token.session_family = None;
        let bob_id = veoveo_platform_store::deterministic_principal_id("test", bob.principal.id.as_str()).unwrap();
        let mut private = Watch::open(&router(replica).layer(Extension(bob))).await;
        private.until(|event| matches!(event, wire::PersonalEvent::Inventory { operations, .. } if operations.is_empty())).await;
        let invitation = veoveo_platform_store::WorkspaceInvitationId::new();
        db.a.invite_workspace_member(&actor, chat, invitation, bob_id).await.unwrap();
        private.until(|event| matches!(event, wire::PersonalEvent::Inventory { operations, invitations: 1, .. } if operations.is_empty())).await;
        db.a.client().query("UPDATE $person SET enabled = false; UPDATE $invitation SET state = 'revoked';")
            .bind(("person", bob_id.record_id())).bind(("invitation", invitation.record_id())).await.unwrap().check().unwrap();
        tokio::time::timeout(Duration::from_secs(5), async { while let Some(event) = private.received.recv().await {
            assert!(!matches!(event, wire::PersonalEvent::Task { .. }), "unrelated or revoked people receive no Task facts");
        }}).await.expect("revocation ends ongoing personal observation");
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 1, "observing, reconnecting and changing replicas never dispatches");
        let next = Uuid::now_v7();
        assert_eq!(request(&app, "POST", &format!("/chats/{chat}/operations"), json!({"id":next,"tool":"fixture__task","arguments":{}})).await.0, StatusCode::OK);
        let created = |event: &wire::PersonalEvent| matches!(event, wire::PersonalEvent::Inventory { operations, .. } if operations.iter().any(|operation| operation.id.0 == next));
        restored.until(created).await;
        second.until(created).await;
        let _ = detail(&app, next).await;
        let mut renewed = Watch::open(&app).await;
        renewed.until(|event| matches!(event, wire::PersonalEvent::Task { operation, state: wire::TaskState::InputRequired, .. } if operation.0 == next)).await;
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 2, "renewing the filter cannot repeat either explicit dispatch");
    }).await.expect("bounded personal feed acceptance");
}
