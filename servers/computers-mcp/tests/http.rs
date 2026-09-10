//! Authenticated wire and replica evidence with a real store and synthetic
//! capacity health. Provider execution keeps its separate native fixture.
#[path = "support/application.rs"]
mod app_support;
#[path = "../../../platform/computers/tests/support/mod.rs"]
#[allow(dead_code)]
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;
use rmcp::model::{
    ClientCapabilities, Implementation, JsonObject, ProtocolVersion, RequestMetaObject,
    TASKS_EXTENSION_ID,
};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_mcp_contract::*;

#[path = "support/signing.rs"]
mod signing;
use signing::Signing;
struct Server {
    base: String,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn new(app: veoveo_computers_mcp::Application, signing: &Signing) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let stop = CancellationToken::new();
        let router = veoveo_computers_mcp::server::router(
            Arc::new(app),
            signing.verifier.clone(),
            vec![address.to_string()],
            stop.clone(),
        )
        .unwrap();
        let shutdown = stop.clone();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(shutdown.cancelled_owned())
                .await
                .unwrap();
        });
        Self {
            base: format!("http://{address}/computers"),
            stop,
            task,
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        self.task.abort();
    }
}
fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}
async fn rpc(
    client: &reqwest::Client,
    server: &Server,
    bearer: &str,
    method: &str,
    mut params: Value,
    tasks: bool,
) -> Value {
    let mut capabilities = ClientCapabilities::default();
    if tasks {
        capabilities.extensions = Some(std::collections::BTreeMap::from([(
            TASKS_EXTENSION_ID.into(),
            JsonObject::new(),
        )]));
    }
    params.as_object_mut().unwrap().insert(
        "_meta".into(),
        serde_json::to_value(RequestMetaObject::with_client_context(
            ProtocolVersion::V_2026_07_28,
            Implementation::new("computers-wire-fixture", "1"),
            capabilities,
        ))
        .unwrap(),
    );
    let mut request = client
        .post(format!("{}/mcp", server.base))
        .bearer_auth(bearer)
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2026-07-28")
        .header("mcp-method", method);
    for (parameter, header) in [
        ("name", "mcp-name"),
        ("uri", "mcp-name"),
        ("taskId", "mcp-name"),
    ] {
        if let Some(value) = params.get(parameter).and_then(Value::as_str) {
            request = request.header(header, value);
        }
    }
    let response = request
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .unwrap();
    assert!(!response.headers().contains_key("mcp-session-id"));
    let status = response.status();
    let body = response.text().await.unwrap();
    let reply: Value = serde_json::from_str(&body).unwrap();
    // The SDK maps transport/capability rejection to HTTP 400. Method-level
    // errors can still be delivered in a successful JSON-RPC HTTP exchange.
    assert!(
        status == reqwest::StatusCode::OK
            || (status == reqwest::StatusCode::BAD_REQUEST && reply.get("error").is_some()),
        "{method}: {status}: {body}"
    );
    reply
}

#[tokio::test]
async fn canonical_http_and_mcp_share_one_private_idempotent_task_across_replicas() {
    let db = support::TestDb::new().await;
    app_support::identities(&db).await;
    let signing = Signing::new();
    let a = Server::new(app_support::application(&db, false).await.0, &signing).await;
    let b = Server::new(
        app_support::application_on(db.b.clone(), false).await.0,
        &signing,
    )
    .await;
    let client = client();
    let alice = signing.bearer("alice", "computers");
    let bob = signing.bearer("bob", "computers");
    assert_eq!(
        client
            .get(format!("{}/healthz", a.base))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    for bearer in [None, Some(signing.bearer("alice", "media"))] {
        let mut request = client.get(format!("{}/admin/docs/llms.txt", a.base));
        if let Some(bearer) = bearer {
            request = request.bearer_auth(bearer);
        }
        assert_eq!(request.send().await.unwrap().status(), 401);
    }
    let discover = rpc(&client, &a, &alice, "server/discover", json!({}), true).await;
    assert_eq!(discover["result"]["supportedVersions"][0], "2026-07-28");
    assert!(discover["result"]["capabilities"]["extensions"][TASKS_EXTENSION_ID].is_object());
    for method in [
        "tools/list",
        "resources/list",
        "resources/templates/list",
        "prompts/list",
    ] {
        let reply = rpc(&client, &a, &alice, method, json!({}), true).await;
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        if method == "tools/list" {
            let tools: rmcp::model::ListToolsResult =
                serde_json::from_value(reply["result"].clone()).unwrap();
            for tool in tools.tools {
                veoveo_mcp_conformance::validate_tool_input_schema(&tool).unwrap();
                assert!(tool.output_schema.is_some());
            }
        }
    }
    let input = json!({"requestId":Uuid::now_v7()});
    let rejected = rpc(
        &client,
        &a,
        &alice,
        "tools/call",
        json!({"name":"create","arguments":input}),
        false,
    )
    .await;
    assert_eq!(rejected["error"]["code"], -32021);
    let empty: Value = client
        .get(format!("{}/admin/computers", a.base))
        .bearer_auth(&alice)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(empty["computers"].as_array().unwrap().len(), 0);
    let created = rpc(
        &client,
        &a,
        &alice,
        "tools/call",
        json!({"name":"create","arguments":input}),
        true,
    )
    .await;
    assert_eq!(created["result"]["resultType"], "task", "{created}");
    let task_id = created["result"]["taskId"].as_str().unwrap();
    let repeated: Value = client
        .post(format!("{}/admin/computers", b.base))
        .bearer_auth(&alice)
        .json(&input)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(repeated["taskId"], task_id);
    let computer_id = repeated["computerId"].as_str().unwrap();
    let task = rpc(
        &client,
        &b,
        &alice,
        "tasks/get",
        json!({"taskId":task_id}),
        true,
    )
    .await;
    assert!(task.get("error").is_none(), "{task}");
    let foreign = rpc(
        &client,
        &b,
        &bob,
        "tasks/get",
        json!({"taskId":task_id}),
        true,
    )
    .await;
    assert!(foreign.get("error").is_some());
    let uri = format!("computer://computers/{computer_id}");
    let read = rpc(
        &client,
        &b,
        &alice,
        "resources/read",
        json!({"uri":uri}),
        true,
    )
    .await;
    assert!(read.get("error").is_none(), "{read}");
    let foreign = rpc(
        &client,
        &a,
        &bob,
        "resources/read",
        json!({"uri":uri}),
        true,
    )
    .await;
    assert_eq!(foreign["error"]["code"], -32602);
    let completion = rpc(&client, &a, &alice, "completion/complete", json!({"ref":{"type":"ref/resource","uri":"computer://computers/{computer_id}"},"argument":{"name":"computer_id","value":computer_id}}), true).await;
    assert_eq!(
        completion["result"]["completion"]["values"][0], computer_id,
        "{completion}"
    );
    let mut read_only = app_support::control();
    read_only.policies[0].rules[0].effect = PolicyEffect::Deny;
    support::policy::install(&db.b, read_only).await;
    let refused = rpc(
        &client,
        &a,
        &alice,
        "tasks/cancel",
        json!({"taskId":task_id}),
        true,
    )
    .await;
    assert!(
        refused.get("error").is_some(),
        "read authority cannot cancel execution"
    );
    support::policy::install(&db.b, app_support::control()).await;
    let cancelled = rpc(
        &client,
        &b,
        &alice,
        "tasks/cancel",
        json!({"taskId":task_id}),
        true,
    )
    .await;
    assert!(cancelled.get("error").is_none(), "{cancelled}");
    let docs = rpc(
        &client,
        &a,
        &alice,
        "resources/read",
        json!({"uri":"computer://docs/design"}),
        true,
    )
    .await;
    assert!(docs.get("error").is_none());
    let manual = client
        .get(format!("{}/admin/docs/agents", b.base))
        .bearer_auth(&alice)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(manual.contains("## Contract Compliance"));
    for method in [reqwest::Method::GET, reqwest::Method::DELETE] {
        assert!(
            !client
                .request(method, format!("{}/mcp", a.base))
                .bearer_auth(&alice)
                .send()
                .await
                .unwrap()
                .status()
                .is_success()
        );
    }
}

async fn sdk(
    server: &Server,
    bearer: String,
) -> rmcp::service::RunningService<rmcp::RoleClient, rmcp::model::ClientInfo> {
    use rmcp::{
        ClientServiceExt,
        model::ClientInfo,
        transport::{
            StreamableHttpClientTransport,
            streamable_http_client::StreamableHttpClientTransportConfig,
        },
    };
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(2))
            .build()
            .unwrap(),
        StreamableHttpClientTransportConfig::with_uri(format!("{}/mcp", server.base))
            .auth_header(bearer),
    );
    tokio::time::timeout(
        Duration::from_secs(5),
        ClientInfo::new(
            ClientCapabilities::builder().enable_tasks().build(),
            Implementation::new("computers-subscription-fixture", "1"),
        )
        .serve_with_lifecycle(
            transport,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

#[tokio::test]
async fn subscription_baselines_cross_replicas_and_close_on_current_policy_revocation() {
    use rmcp::model::{ServerNotification, SubscriptionFilter};
    let db = support::TestDb::new().await;
    app_support::identities(&db).await;
    let signing = Signing::new();
    let a = Server::new(app_support::application(&db, false).await.0, &signing).await;
    let b = Server::new(
        app_support::application_on(db.b.clone(), false).await.0,
        &signing,
    )
    .await;
    let peer = sdk(&a, signing.bearer("alice", "computers")).await;
    let filter = SubscriptionFilter::builder()
        .resource_subscription("computer://computers")
        .build();
    let mut subscription =
        tokio::time::timeout(Duration::from_secs(5), peer.listen(filter.clone()))
            .await
            .unwrap()
            .unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), subscription.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(first, ServerNotification::ResourceUpdatedNotification(ref n) if n.params.uri == "computer://computers")
    );
    let response = client()
        .post(format!("{}/admin/computers", b.base))
        .bearer_auth(signing.bearer("alice", "computers"))
        .json(&json!({"requestId":Uuid::now_v7()}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let updated = tokio::time::timeout(Duration::from_secs(5), subscription.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(updated, ServerNotification::ResourceUpdatedNotification(ref n) if n.params.uri == "computer://computers")
    );
    subscription.cancel().await.unwrap();
    // Reconnect obtains a fresh invalidation baseline, without transport replay IDs.
    let mut resumed = tokio::time::timeout(Duration::from_secs(5), peer.listen(filter))
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(5), resumed.next())
            .await
            .unwrap()
            .unwrap()
            .is_some()
    );
    let mut revoked = app_support::control();
    revoked.policies[0].rules[1].effect = PolicyEffect::Deny;
    support::policy::install(&db.b, revoked).await;
    let ended = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match resumed.next().await {
                Ok(Some(_)) => continue, // Drain events authorized before the new revision.
                result => break result,
            }
        }
    })
    .await
    .expect("revoked subscription remained open");
    assert!(ended.is_err() || matches!(ended, Ok(None)));
    peer.cancel().await.unwrap();
}

#[tokio::test]
async fn subscription_assertion_expiry_and_foreign_targets_deliver_no_private_updates() {
    use rmcp::model::{ServerNotification, SubscriptionFilter};
    let db = support::TestDb::new().await;
    app_support::identities(&db).await;
    let signing = Signing::new();
    let a = Server::new(app_support::application(&db, false).await.0, &signing).await;
    let created: Value = client()
        .post(format!("{}/admin/computers", a.base))
        .bearer_auth(signing.bearer("alice", "computers"))
        .json(&json!({"requestId":Uuid::now_v7()}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let foreign_uri = format!(
        "computer://computers/{}",
        created["computerId"].as_str().unwrap()
    );
    let bob = sdk(&a, signing.bearer("bob", "computers")).await;
    for filter in [
        SubscriptionFilter::builder()
            .resource_subscription(foreign_uri)
            .build(),
        SubscriptionFilter::builder()
            .task_id(created["taskId"].as_str().unwrap())
            .build(),
    ] {
        // The SDK may acknowledge the syntactically supported filter before the
        // application authorizes it. No private notification may precede closure.
        if let Ok(mut rejected) = tokio::time::timeout(Duration::from_secs(5), bob.listen(filter))
            .await
            .unwrap()
        {
            let update = tokio::time::timeout(Duration::from_secs(5), rejected.next())
                .await
                .unwrap();
            assert!(update.is_err() || matches!(update, Ok(None)));
        }
    }
    bob.cancel().await.unwrap();
    let alice = sdk(
        &a,
        signing.bearer_until(
            "alice",
            "computers",
            chrono::Utc::now() + chrono::TimeDelta::seconds(3),
        ),
    )
    .await;
    let mut expires = tokio::time::timeout(
        Duration::from_secs(5),
        alice.listen(
            SubscriptionFilter::builder()
                .resource_subscription("computer://computers")
                .build(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), expires.next())
            .await
            .unwrap()
            .unwrap(),
        Some(ServerNotification::ResourceUpdatedNotification(_))
    ));
    let end = tokio::time::timeout(Duration::from_secs(5), expires.next())
        .await
        .expect("expired subscription remained open");
    assert!(end.is_err() || matches!(end, Ok(None)));
    alice.cancel().await.unwrap();
}
