//! Native admission qualification with disposable Store and a deterministic adapter.
//! The fake executes no simulation or GPU work.
use crate::server::test_support::context;
use crate::{
    adapter::{Adapter, FakeAdapter},
    contract::UavScope,
    server::{service::fake_state, task_worker::await_result, test_support},
};
use rmcp::{
    ServerHandler,
    model::{CallToolRequestParams, CallToolResponse},
    service::serve_directly,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;

fn request(tool: &str) -> CallToolRequestParams {
    let arguments = match tool {
        "run_scenario" => {
            serde_json::json!({"session_id":"native-session", "duration_seconds":1.0})
        }
        "capture_dataset" => {
            serde_json::json!({"session_id":"native-session", "duration_seconds":1.0, "sensors":["test-sensor"]})
        }
        "execute_vehicle_mission_plan" => {
            serde_json::json!({"plan_id":"absent-plan", "expected_revision":1})
        }
        _ => unreachable!(),
    };
    CallToolRequestParams::new(tool.to_owned())
        .with_arguments(arguments.as_object().unwrap().clone())
}

#[tokio::test]
async fn call_paths_require_the_tool_scope_before_persistence_or_execution() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let db = test_support::fixture::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let mut simulation = fake_state().unwrap();
        simulation.session_id = crate::contract::SessionId::parse("native-session").unwrap();
        let adapter = Arc::new(Mutex::new(FakeAdapter::new(simulation.clone())));
        let state = test_support::state(
            &db.a,
            Arc::new(Adapter::Fake(adapter.clone())),
            "scope-test",
        );
        // The SDK owns the peer lifetime; requests invoke the real ServerHandler router.
        // Authentication is supplied by the fixture, independently of HTTP/JWT qualification.
        let mut running = serve_directly(
            crate::server::service::hosted(state.clone()),
            (futures::sink::drain(), futures::stream::pending()),
            None,
        );
        for tasks in [false, true] {
            for scopes in [
                vec![],
                vec![UavScope::Read],
                vec![UavScope::Control],
                vec![UavScope::Stream],
            ] {
                for tool in ["run_scenario", "capture_dataset"] {
                    let error = running
                        .service()
                        .call_tool(request(tool), context(running.peer(), &scopes, tasks))
                        .await
                        .unwrap_err();
                    assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_REQUEST);
                    assert!(error.message.contains("uav-sim:admin"));
                }
            }
            // Administrative and read scopes never imply vehicle control.
            for scopes in [
                vec![],
                vec![UavScope::Admin],
                vec![UavScope::Read, UavScope::Stream],
            ] {
                let error = running
                    .service()
                    .call_tool(
                        request("execute_vehicle_mission_plan"),
                        context(running.peer(), &scopes, tasks),
                    )
                    .await
                    .unwrap_err();
                assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_REQUEST);
                assert!(error.message.contains("uav-sim:control"));
            }
        }
        assert!(state.tasks.list().await.unwrap().is_empty());
        assert_eq!(adapter.lock().await.state(), simulation);

        for tasks in [false, true] {
            for tool in ["run_scenario", "capture_dataset"] {
                let response = running
                    .service()
                    .call_tool(
                        request(tool),
                        context(running.peer(), &[UavScope::Admin], tasks),
                    )
                    .await
                    .unwrap();
                let result = match response {
                    CallToolResponse::Task(created) => {
                        assert!(tasks);
                        await_result(&state, created.task.task_id.parse().unwrap())
                            .await
                            .unwrap()
                    }
                    CallToolResponse::Complete(result) => {
                        assert!(!tasks);
                        result
                    }
                    _ => panic!("unexpected input-required response"),
                };
                assert_ne!(result.is_error, Some(true));
            }
        }
        assert_eq!(state.tasks.list().await.unwrap().len(), 4);
        assert_eq!(
            adapter.lock().await.state().simulation_time_s,
            simulation.simulation_time_s + 2.0
        );

        // Possession of the tool scope still requires a current UAV plan and grant.
        for tasks in [false, true] {
            let error = running
                .service()
                .call_tool(
                    request("execute_vehicle_mission_plan"),
                    context(running.peer(), &[UavScope::Control], tasks),
                )
                .await
                .unwrap_err();
            assert!(!error.message.contains("Missing scope"));
        }
        assert_eq!(state.tasks.list().await.unwrap().len(), 4);
        running.close().await.unwrap();
    })
    .await
    .expect("native scope admission exceeded 90 seconds");
}
