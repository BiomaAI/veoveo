//! Real-store wire authorization with a synthetic Ready row, without provider I/O.
use super::{Duration, Server, Signing, Uuid, Value, app_support, client, json, rpc, support};
use rmcp::model::{ServerNotification, SubscriptionFilter};
use veoveo_computers::{
    ComputerActor, ComputersStore, Reservation, session_grants::SessionGrantPolicy,
};
use veoveo_mcp_contract::{GatewayAction, PolicyRuleId};

#[tokio::test]
async fn access_inventory_and_revocation_share_owned_state_and_resource_invalidations() {
    let db = support::TestDb::new().await;
    app_support::identities(&db).await;
    let identity = support::browser::identity(&db, "alice").await;
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let signing = Signing::new();
    let a = Server::new(app_support::application(&db, false).await.0, &signing).await;
    let b = Server::new(
        app_support::application_on(db.b.clone(), false).await.0,
        &signing,
    )
    .await;
    let mut policy = app_support::control();
    let mut attach = policy.policies[0].rules[1].clone();
    attach.id = PolicyRuleId::new("attach").unwrap();
    attach.actions = [GatewayAction::ComputerAttach].into_iter().collect();
    policy.policies[0].rules.push(attach);
    support::policy::install(&db.a, policy).await;
    let store = ComputersStore::new(db.a.clone(), Uuid::from_u128(100)).unwrap();
    store
        .install_session_grant_policy(
            None,
            SessionGrantPolicy {
                max_grants: 2,
                absolute_seconds: 120,
                idle_seconds: 60,
            },
        )
        .await
        .unwrap();
    let computer = store
        .reserve(
            actor.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap()
        .computer_id;
    db.a.client().query("UPDATE ONLY $computer SET phase = 'ready', provider_resource_id = 'fixture-resource', process_id = 'fixture-process';")
        .bind(("computer", surrealdb::types::RecordId::new("computer", surrealdb::types::Uuid::from(computer))))
        .await.unwrap().check().unwrap();
    let ticket = store.issue_browser_grant(&actor, computer).await.unwrap();
    let handle = store
        .redeem_browser_grant(&actor, &ticket.token)
        .await
        .unwrap();
    let grant = handle.grant_id();
    let alice = signing.identity(
        identity,
        "computers",
        chrono::Utc::now() + chrono::TimeDelta::minutes(2),
    );
    let http = client();
    let inventory_url = format!("{}/admin/computers/{computer}/access", a.base);
    let inventory: Value = http
        .get(&inventory_url)
        .bearer_auth(&alice)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(inventory["grants"][0]["grantId"], grant.to_string());
    assert_eq!(inventory["grants"][0]["currentSession"], true);
    let uri = format!("computer://computers/{computer}/access");
    let resource = rpc(
        &http,
        &b,
        &alice,
        "resources/read",
        json!({"uri":uri}),
        false,
    )
    .await;
    let projected: Value =
        serde_json::from_str(resource["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(inventory, projected);
    assert_eq!(
        http.get(&inventory_url)
            .bearer_auth(signing.bearer("bob", "computers"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let peer = super::sdk(&a, alice.clone()).await;
    let mut updates = peer
        .listen(
            SubscriptionFilter::builder()
                .resource_subscription(uri.clone())
                .build(),
        )
        .await
        .unwrap();
    updates.next().await.unwrap().unwrap();
    let revoke = format!(
        "{}/admin/computers/{computer}/access/{grant}/revoke",
        b.base
    );
    assert_eq!(
        http.post(&revoke)
            .bearer_auth(&alice)
            .json(&json!({"owner":"bob"}))
            .send()
            .await
            .unwrap()
            .status(),
        422
    );
    assert_eq!(
        http.post(&revoke)
            .bearer_auth(signing.bearer("bob", "computers"))
            .json(&json!({}))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    assert!(store.renew_browser_grant(&handle, false).await.is_ok());
    let revoked: Value = http
        .post(&revoke)
        .bearer_auth(&alice)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        revoked,
        json!({"computerId":computer,"grantId":grant,"revoked":true})
    );
    let notification = tokio::time::timeout(Duration::from_secs(5), updates.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        matches!(notification, ServerNotification::ResourceUpdatedNotification(ref n) if n.params.uri == uri)
    );
    assert!(store.renew_browser_grant(&handle, false).await.is_err());
    let repeated = rpc(
        &http,
        &a,
        &alice,
        "tools/call",
        json!({
            "name":"revoke_access", "arguments":{"computerId":computer,"grantId":grant},
        }),
        false,
    )
    .await;
    assert_eq!(repeated["result"]["structuredContent"], revoked);
    assert!(
        store
            .access_grants(&actor, computer)
            .await
            .unwrap()
            .grants
            .is_empty()
    );
    assert_eq!(
        store.get(actor.owner(), computer).await.unwrap().phase,
        veoveo_computers::api::ComputerPhase::Ready
    );
    updates.cancel().await.unwrap();
    peer.cancel().await.unwrap();
}
