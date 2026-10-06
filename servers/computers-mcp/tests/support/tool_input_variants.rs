//! Argument admission through the existing authenticated HTTP owner fixture.
use super::*;

#[tokio::test]
async fn file_transfer_branches_reject_before_domain_effects() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = support::database().await;
        app_support::identities(&db).await;
        let signing = Signing::new();
        let (app, _health) = app_support::application(&db, false).await;
        let tasks = app.task_runtime().clone();
        assert!(tasks.list().await.unwrap().is_empty());
        let server = Server::new(app, &signing).await;
        let client = client();
        let alice = signing.bearer("alice", "computers");
        let discover = rpc(&client, &server, &alice, "server/discover", json!({}), true).await;
        assert_eq!(discover["result"]["supportedVersions"][0], "2026-07-28");
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 7);
        for case in cases {
            match case.tool.as_str() {
                "transfer_file" => {
                    let _: veoveo_computers_mcp::contract::TransferFileInput = case.decode();
                }
                "grant_automation" => {
                    let _: veoveo_computers_mcp::contract::IssueAutomationGrantInput =
                        case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for (location, arguments) in case
                .unknown_fields()
                .into_iter()
                .chain(case.invalid_values())
            {
                let body = rpc(
                    &client,
                    &server,
                    &alice,
                    "tools/call",
                    json!({"name":case.tool,"arguments":arguments}),
                    true,
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
        let empty: Value = client
            .get(format!("{}/admin/computers", server.base))
            .bearer_auth(&alice)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(empty["computers"].as_array().unwrap().len(), 0);
        assert!(tasks.list().await.unwrap().is_empty());
    })
    .await
    .expect("Computers variant admission exceeded 180 seconds");
}
