//! The hosted server, tested in-process through the test gateway.

use serde_json::json;
use veoveo_mcp_contract::hosting::{
    Hosted,
    testing::{self, TestGateway},
};

use crate::handler::GlossaryMcp;

fn gateway() -> TestGateway {
    TestGateway::new(
        testing::for_domain::<GlossaryMcp>()
            .handler(|| Hosted::new(GlossaryMcp::new()))
            .build(),
    )
}

#[tokio::test]
async fn discovery_lists_tools_resources_templates_and_prompts() {
    let gateway = gateway();
    let tools = gateway.rpc("tools/list", json!({})).await;
    assert_eq!(tools["result"]["tools"][0]["name"], "define");
    let resources = gateway.rpc("resources/list", json!({})).await;
    assert_eq!(
        resources["result"]["resources"].as_array().unwrap().len(),
        5
    );
    let templates = gateway.rpc("resources/templates/list", json!({})).await;
    assert_eq!(
        templates["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let prompts = gateway.rpc("prompts/list", json!({})).await;
    assert_eq!(prompts["result"]["prompts"][0]["name"], "explain_term");
}

#[tokio::test]
async fn define_returns_typed_structured_content() {
    let gateway = gateway();
    let body = gateway
        .rpc(
            "tools/call",
            json!({"name": "define", "arguments": {"term": "hosted-server"}}),
        )
        .await;
    let definition = &body["result"]["structuredContent"];
    assert_eq!(definition["term"], "hosted-server");
    assert_eq!(
        definition["see_also"],
        json!(["domain-read", "durable-task"])
    );

    let body = gateway
        .rpc(
            "tools/call",
            json!({"name": "define", "arguments": {"term": "Not A Term"}}),
        )
        .await;
    // Invalid tool arguments are a tool error the model can correct.
    assert_eq!(body["result"]["isError"], true);
}

#[tokio::test]
async fn reads_admit_only_glossary_addresses() {
    let gateway = gateway();
    let body = gateway
        .rpc(
            "resources/read",
            json!({"uri": "glossary://term/domain-read"}),
        )
        .await;
    let entry: serde_json::Value =
        serde_json::from_str(body["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(entry["title"], "Domain read");
    assert_eq!(body["result"]["cacheScope"], "private");

    let body = gateway
        .rpc("resources/read", json!({"uri": "glossary://contract"}))
        .await;
    assert!(body["result"]["contents"][0]["text"].is_string());

    let body = gateway
        .rpc("resources/read", json!({"uri": "glossary://term/Bad"}))
        .await;
    assert_eq!(body["error"]["code"], -32602);
}

#[tokio::test]
async fn completion_ranks_term_ids_and_documents() {
    let gateway = gateway();
    let body = gateway
        .rpc(
            "completion/complete",
            json!({
                "ref": {"type": "ref/resource", "uri": "glossary://term/{term_id}"},
                "argument": {"name": "term_id", "value": "re"}
            }),
        )
        .await;
    // Prefix matches rank before substring matches.
    assert_eq!(
        body["result"]["completion"]["values"],
        json!(["resource-address", "domain-read"])
    );
    let body = gateway
        .rpc(
            "completion/complete",
            json!({
                "ref": {"type": "ref/resource", "uri": "glossary://docs/{doc_id}"},
                "argument": {"name": "doc_id", "value": "ag"}
            }),
        )
        .await;
    assert_eq!(body["result"]["completion"]["values"], json!(["agents"]));
}

#[tokio::test]
async fn prompts_render_with_typed_arguments() {
    let gateway = gateway();
    let body = gateway
        .rpc(
            "prompts/get",
            json!({"name": "explain_term", "arguments": {"term": "durable-task"}}),
        )
        .await;
    let text = body["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("glossary://term/durable-task"));

    let body = gateway
        .rpc(
            "prompts/get",
            json!({"name": "explain_term", "arguments": {"term": "Bad"}}),
        )
        .await;
    assert_eq!(body["error"]["code"], -32602);
}
