//! Hosted argument admission using the existing isolated authority fixture.
#[path = "../../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use super::*;
use serde_json::json;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

#[tokio::test]
async fn unknown_tool_arguments_complete_before_time_resolution() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.a.clone());
        let acquisitions = crate::acquisition::AcquisitionService::new(
            crate::acquisition::AcquisitionServiceConfig {
                scratch_root: files.root.path().join("scratch"),
                release_root: files.root.path().join("releases"),
                zic_executable: files.root.path().join("unavailable-zic"),
                maximum_source_bytes: 1024,
                maximum_expanded_bytes: 4096,
                timeout: Duration::from_secs(1),
            },
            catalog.clone(),
        )
        .unwrap();
        let state = Arc::new(crate::state::TimeApplication {
            tasks: veoveo_task_runtime::TaskRuntime::new(db.a.clone(), "time", "strict-input"),
            catalog,
            authorities: files.registry(),
            clock: crate::clock::ClockMonitor::new(
                crate::clock::ClockSource::System,
                Duration::from_secs(1),
            ),
            acquisitions: Arc::new(acquisitions),
            subscriptions: Arc::new(veoveo_mcp_contract::SubscriptionHub::new()),
            event_watchers: Arc::default(),
        });
        let handler = state.clone();
        let gateway = TestGateway::new(
            testing::for_domain::<crate::mcp::TimeMcp>()
                .handler(move || Hosted::new(crate::mcp::TimeMcp::new(handler.clone())))
                .build(),
        );
        let discover = gateway.rpc("server/discover", json!({})).await;
        assert!(discover.get("error").is_none(), "{discover}");
        let mut arguments =
            json!({"expression":{"format":"rfc3339","value":"2026-01-01T00:00:00Z"}});
        let _: ResolveTimeRequest = serde_json::from_value(arguments.clone()).unwrap();
        arguments["undeclared"] = true.into();
        let body = gateway
            .rpc(
                "tools/call",
                json!({"name":"resolve_time","arguments":arguments}),
            )
            .await;
        assert!(body.get("error").is_none(), "{body}");
        assert_eq!(body["result"]["resultType"], "complete", "{body}");
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(body["result"].clone()).unwrap();
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 30);
        for case in cases {
            match case.tool.as_str() {
                "resolve_time" => {
                    let _: crate::contract::ResolveTimeRequest = case.decode();
                }
                "convert_time" => {
                    let _: crate::contract::ConvertTimeRequest = case.decode();
                }
                "evaluate_windows" => {
                    let _: crate::contract::EvaluateWindowsRequest = case.decode();
                }
                "expand_schedule" => {
                    let _: crate::contract::ExpandScheduleRequest = case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for (location, arguments) in case
                .unknown_fields()
                .into_iter()
                .chain(case.invalid_values())
            {
                let body = gateway
                    .rpc(
                        "tools/call",
                        json!({"name":case.tool,"arguments":arguments}),
                    )
                    .await;
                assert!(
                    body.get("error").is_none(),
                    "{} {location}: {body}",
                    case.branch
                );
                assert_eq!(
                    body["result"]["resultType"], "complete",
                    "{} {location}: {body}",
                    case.branch
                );
                let result: rmcp::model::CallToolResult =
                    serde_json::from_value(body["result"].clone()).unwrap();
                assert_eq!(
                    result.is_error,
                    Some(true),
                    "{} {location}: {body}",
                    case.branch
                );
                case.assert_error(&location, &serde_json::to_string(&result.content).unwrap());
            }
        }
        assert!(state.tasks.list().await.unwrap().is_empty());
        assert!(state.event_watchers.lock().await.is_empty());
    })
    .await
    .expect("Time argument admission exceeded 120 seconds");
}
