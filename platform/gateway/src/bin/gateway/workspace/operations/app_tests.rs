//! Real native App calls share the durable journal; no production fixture routes.
use super::*;

async fn app_detail(app: &Router, id: Uuid) -> Value {
    loop {
        let (status, view) = request(
            app,
            "POST",
            &format!("/app-operations/{id}"),
            json!({"appUri":"ui://fixture/task.html"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{view}");
        if !view["native"].is_null() {
            return view;
        }
        assert_eq!(view["operation"]["phase"], "dispatching", "{view}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn app_tasks_recover_with_exact_origin_and_native_input_without_replay() {
    tokio::time::timeout(Duration::from_secs(45), async {
        let db = crate::test_store::TestDb::new().await;
        super::super::super::tests::setup(&db.a).await;
        let subject = alice();
        let fixture = super::super::test_domain::Fixture::start(db.a.clone(), &subject).await;
        let state = new_state(db.a.clone(), fixture.port);
        let authority = state.authority(&subject, &GatewayProfileId::new("operator").unwrap()).await.unwrap();
        let chat = WorkspaceChatId::new();
        db.a.create_workspace_chat(&authority, chat, "Native App Tasks").await.unwrap();
        let app = new_app(state);
        let id = Uuid::now_v7();
        let path = format!("/chats/{}/app-operations", chat.as_uuid());
        let start = json!({"id":id,"appUri":"ui://fixture/task.html","tool":"task","arguments":{}});
        assert_eq!(request(&app, "POST", &path, start.clone()).await.0, StatusCode::OK);
        assert_eq!(request(&app, "POST", &path, start.clone()).await.0, StatusCode::OK);
        let accepted = app_detail(&app, id).await;
        assert_eq!(accepted["native"]["resultType"], "task");
        let task = accepted["native"]["taskId"].as_str().unwrap();
        let own = json!({"appUri":"ui://fixture/task.html","taskId":task});
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 1);
        let restored = new_app(new_state(db.b.clone(), fixture.port));
        assert_eq!(app_detail(&restored, id).await["native"]["taskId"], task);
        let (status, current) = request(&restored, "POST", "/app-tasks/get", own.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(current["status"], "input_required");
        let forged = json!({"appUri":"ui://fixture/other.html","taskId":task});
        for endpoint in ["get", "cancel"] {
            assert_eq!(request(&restored, "POST", &format!("/app-tasks/{endpoint}"), forged.clone()).await.0, StatusCode::NOT_FOUND);
        }
        assert_eq!(request(&restored, "POST", &format!("/app-operations/{id}"), json!({"appUri":"ui://fixture/other.html"})).await.0, StatusCode::NOT_FOUND);
        let answer = |count| json!({"appUri":"ui://fixture/task.html","taskId":task,"inputResponses":{"approval-1":{"action":"accept","content":{"count":count}}}});
        assert_eq!(request(&restored, "POST", "/app-tasks/update", answer(8)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(request(&restored, "POST", "/app-tasks/update", answer(2)).await.0, StatusCode::OK);
        assert_eq!(request(&restored, "POST", "/app-tasks/update", answer(2)).await.0, StatusCode::CONFLICT);
        assert_eq!(request(&restored, "POST", "/app-tasks/cancel", own.clone()).await.0, StatusCode::OK);
        assert_eq!(request(&restored, "POST", "/app-tasks/get", own.clone()).await.1["status"], "working", "an acknowledgement is not terminal");
        fixture.domain.runtime.transition(task, TaskTransition::Cancelled).await.unwrap();
        assert_eq!(request(&restored, "POST", "/app-tasks/get", own.clone()).await.1["status"], "cancelled");
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 1);
        // Current App catalog admission is rechecked after restart, independent of Task ownership.
        fixture.domain.app_visible.store(false, Ordering::SeqCst);
        assert_eq!(request(&restored, "POST", "/app-tasks/get", own).await.0, StatusCode::NOT_FOUND);
        fixture.domain.app_visible.store(true, Ordering::SeqCst);
        let mut global_alias = start.clone(); global_alias["tool"] = json!("fixture__task");
        assert_eq!(request(&restored, "POST", &path, global_alias).await.0, StatusCode::FORBIDDEN);
        // Multi-round state stays upstream. The frame gets only the journal revision reference.
        let mrtr_id = Uuid::now_v7();
        let mut mrtr = json!({"id":mrtr_id,"appUri":"ui://fixture/task.html","tool":"task","arguments":{"mode":"mrtr"}});
        assert_eq!(request(&restored, "POST", &path, mrtr.clone()).await.0, StatusCode::OK);
        let pending = app_detail(&restored, mrtr_id).await;
        assert_eq!(pending["native"]["resultType"], "input_required");
        assert!(!pending.to_string().contains("protected-fixture-continuation"));
        mrtr["id"] = json!(Uuid::now_v7());
        mrtr["requestState"] = pending["native"]["requestState"].clone();
        mrtr["inputResponses"] = json!({"confirm":{"action":"accept","content":{"approved":true}}});
        assert_eq!(request(&restored, "POST", &path, mrtr.clone()).await.0, StatusCode::OK);
        let completed = app_detail(&restored, mrtr_id).await;
        assert_eq!(completed["native"]["resultType"], "complete");
        assert_eq!(completed["native"]["content"][0]["text"], "Confirmed continuation");
        assert_eq!(request(&restored, "POST", &path, mrtr).await.0, StatusCode::CONFLICT);
        assert_eq!(fixture.domain.calls.load(Ordering::SeqCst), 3);
    }).await.expect("bounded App Task acceptance");
}
