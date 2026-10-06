//! Real Artifact service and MCP transport over an isolated SurrealDB fixture.
mod auth;
mod fixture;
#[path = "../../../../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;
use fixture::{Client, Fixture};
use rmcp::{model::*, service::Subscription};
use std::{collections::BTreeSet, time::Duration};
use veoveo_artifact_contract::ArtifactId;
use veoveo_artifact_mcp::contract::{ArtifactIndexCursor, ArtifactResource};
use veoveo_mcp_contract::{ArtifactPlane, PlaneCaller, PutArtifactRequest};
use veoveo_mcp_knowledge_extension as extension;
use veoveo_types::{AccessLevel, AccessSubject, ResourceUri};

#[tokio::test]
async fn unknown_tool_arguments_complete_without_changing_artifact_metadata() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::store_fixture::TestDb::new().await;
        let f = Fixture::new(db.a.clone()).await;
        let artifact = f
            .artifacts
            .put(
                &f.owner,
                PutArtifactRequest::default(),
                b"strict input fixture".to_vec(),
            )
            .await
            .unwrap();
        let client = f.sdk(&f.owner).await;
        let before = f
            .artifacts
            .head(&f.owner, &artifact.artifact_id())
            .await
            .unwrap();
        let arguments = serde_json::json!({"artifact_id":artifact.artifact_id()});
        let _: veoveo_artifact_mcp::contract::ArtifactReference =
            serde_json::from_value(arguments.clone()).unwrap();
        let valid = client
            .call_tool(
                CallToolRequestParams::new("metadata")
                    .with_arguments(arguments.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        assert_ne!(valid.is_error, Some(true));
        let mut invalid = arguments;
        invalid["undeclared"] = true.into();
        let response = client
            .call_tool_once(
                CallToolRequestParams::new("metadata")
                    .with_arguments(invalid.as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        let CallToolResponse::Complete(result) = response else {
            panic!("malformed input must complete");
        };
        assert_eq!(result.is_error, Some(true));
        assert!(
            serde_json::to_string(&result.content)
                .unwrap()
                .contains("undeclared")
        );
        let cases = input_fixture::ToolInputCase::load(include_bytes!(
            "../../../../testdata/controlled-inputs.json"
        ));
        assert_eq!(cases.len(), 8);
        for case in cases {
            match case.tool.as_str() {
                "grant_access" => {
                    let _: veoveo_artifact_mcp::contract::GrantArtifactRequest = case.decode();
                }
                "set_release_state" => {
                    let _: veoveo_artifact_mcp::contract::SetArtifactReleaseRequest = case.decode();
                }
                _ => panic!("unexpected fixture tool"),
            }
            for (location, arguments) in case
                .unknown_fields()
                .into_iter()
                .chain(case.invalid_values())
            {
                let response = client
                    .call_tool_once(
                        rmcp::model::CallToolRequestParams::new(case.tool.clone())
                            .with_arguments(arguments.as_object().unwrap().clone()),
                    )
                    .await
                    .unwrap();
                let rmcp::model::CallToolResponse::Complete(result) = response else {
                    panic!("{} {location}: malformed input must complete", case.branch);
                };
                assert_eq!(
                    result.is_error,
                    Some(true),
                    "{} {location}: {result:?}",
                    case.branch
                );
                case.assert_error(&location, &serde_json::to_string(&result.content).unwrap());
            }
        }
        let after = f
            .artifacts
            .head(&f.owner, &artifact.artifact_id())
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
        client.cancel().await.unwrap();
    })
    .await
    .expect("Artifact malformed tool input exceeded 120 seconds");
}

#[tokio::test]
async fn readiness_detects_plane_loss_without_failing_liveness() {
    let db = crate::store_fixture::TestDb::new().await;
    let mut fixture = Fixture::new(db.a.clone()).await;
    assert_eq!(
        fixture.probe("/artifact/readyz").await,
        reqwest::StatusCode::OK
    );
    fixture.stop_artifact_service().await;
    assert_eq!(
        fixture.probe("/artifact/readyz").await,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fixture.probe("/artifact/healthz").await,
        reqwest::StatusCode::OK
    );
}

#[tokio::test]
async fn readiness_detects_store_loss_without_failing_liveness() {
    let db = crate::store_fixture::TestDb::new().await;
    let fixture = Fixture::new(db.a.clone()).await;
    assert_eq!(
        fixture.probe("/artifact/readyz").await,
        reqwest::StatusCode::OK
    );
    drop(db);
    assert_eq!(
        fixture.probe("/artifact/readyz").await,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        fixture.probe("/artifact/healthz").await,
        reqwest::StatusCode::OK
    );
}

async fn read(
    client: &Client,
    uri: &ResourceUri,
    revision: Option<&extension::Revision>,
) -> Result<ReadResourceResult, rmcp::ServiceError> {
    let (request, options) = extension::client::read_request(
        ReadResourceRequestParams::new(uri.as_str()),
        ClientCapabilities::default(),
        revision,
        rmcp::service::PeerRequestOptions::default(),
    );
    let result = client
        .peer()
        .send_request_with_option(request, options)
        .await?
        .await_response()
        .await?;
    match result {
        ServerResult::ReadResourceResult(result) => Ok(result),
        _ => panic!("resource response"),
    }
}
async fn updates(
    listener: &mut Subscription,
    expected: &[&ResourceUri],
) -> chrono::DateTime<chrono::Utc> {
    let mut pending = expected
        .iter()
        .map(|uri| uri.as_str())
        .collect::<BTreeSet<_>>();
    tokio::time::timeout(Duration::from_secs(15), async {
        while !pending.is_empty() {
            let event = listener
                .next()
                .await
                .unwrap()
                .expect("stream closed before invalidating admitted resources");
            if let ServerNotification::ResourceUpdatedNotification(value) = event {
                pending.remove(value.params.uri.as_str());
            }
        }
    })
    .await
    .expect("missing Artifact invalidation");
    chrono::Utc::now()
}
fn denied(result: Result<ReadResourceResult, rmcp::ServiceError>) -> bool {
    match result {
        Ok(_) => false,
        Err(rmcp::ServiceError::McpError(error)) => {
            assert_eq!(
                error.code,
                ErrorCode::INVALID_PARAMS,
                "expected source access denial"
            );
            true
        }
        Err(error) => panic!("transport failure is not access denial: {error}"),
    }
}
async fn grant(f: &Fixture, id: ArtifactId, reader: &PlaneCaller) {
    f.artifacts
        .grant(
            &f.owner,
            &id,
            AccessSubject::Principal(reader.identity.actor.id.clone()),
            AccessLevel::Read,
        )
        .await
        .unwrap();
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    items: Vec<Entry>,
    next_cursor: Option<ArtifactIndexCursor>,
}
#[derive(serde::Deserialize)]
struct Entry {
    uri: ResourceUri,
}
async fn page(client: &Client, root: &ResourceUri) -> Page {
    let result = read(client, root, None).await.unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &result.contents[0] else {
        panic!("index text")
    };
    serde_json::from_str(text).unwrap()
}

#[tokio::test]
async fn access_loss_invalidates_members_and_roots_for_revocation_and_expiry() {
    tokio::time::timeout(Duration::from_secs(240), async {
        let db = crate::store_fixture::TestDb::new().await;
        let f = Fixture::new(db.a.clone()).await;
        let artifact = f.artifacts.put(&f.owner, PutArtifactRequest::default(), b"source fixture".to_vec()).await.unwrap();
        let id = artifact.artifact_id();
        let reader = f.signing.caller("reader", "research");
        let client = f.sdk(&reader).await;
        let root = ArtifactResource::Index { cursor: None }.to_uri();
        let member = ArtifactResource::Metadata(id).to_uri();
        assert!(denied(read(&client, &member, None).await));
        let hidden_author = f.signing.caller("hidden-author", "private");
        let hidden = f.artifacts.put(&hidden_author, PutArtifactRequest::default(), b"hidden".to_vec()).await.unwrap();
        let hidden_deadline = chrono::Utc::now() + chrono::TimeDelta::minutes(1);
        db.b.client().query(include_str!("../../../../queries/bin/server/subscription_tests/mod/access_loss_invalidates_members_and_roots_for_revocation_and_expiry.surql"))
            .bind(("expiry", hidden_deadline)).bind(("artifact", veoveo_platform_store::ArtifactId::from_uuid(hidden.artifact_id().as_uuid()).record_id()))
            .await.unwrap().check().unwrap();
        assert_eq!(f.subscriptions.deadline(&reader, None).await.unwrap(), reader.identity.expires_at,
            "unreadable occurrences must not select a listener deadline");
        assert_eq!(f.subscriptions.deadline(&f.owner, Some(&[id])).await.unwrap(), f.owner.identity.expires_at);
        grant(&f, id, &reader).await;
        let response = read(&client, &member, None).await.unwrap();
        let observed = extension::client::validate_read(&response, &member, None).unwrap().unwrap();
        for expire in [false, true] {
            if expire { grant(&f, id, &reader).await; }
            let expiry = chrono::Utc::now() + chrono::TimeDelta::seconds(8);
            if expire {
                db.b.client().query(include_str!("../../../../queries/bin/server/subscription_tests/mod/access_loss_invalidates_members_and_roots_for_revocation_and_expiry_2.surql"))
                    .bind(("expiry", expiry)).bind(("artifact", veoveo_platform_store::ArtifactId::from_uuid(id.as_uuid()).record_id()))
                    .await.unwrap().check().unwrap();
            }
            let mut listener = client.listen(SubscriptionFilter::builder()
                // Member first proves its denial cannot prevent the root signal.
                .resource_subscription(member.as_str()).resource_subscription(root.as_str()).build()).await.unwrap();
            updates(&mut listener, &[&root, &member]).await;
            if !expire { f.artifacts.revoke(&f.owner, &id, &AccessSubject::Principal(reader.identity.actor.id.clone())).await.unwrap(); }
            // Changefeed replay may produce harmless invalidations before expiry.
            // Continue until source reads prove denial; the deadline must then send
            // both signals even though no writer mutates the expired grant.
            loop {
                let notified_at = updates(&mut listener, &[&root, &member]).await;
                if (!expire || notified_at >= expiry) && denied(read(&client, &member, Some(observed.revision())).await) { break; }
            }
            if expire { assert!(chrono::Utc::now() >= expiry); }
            assert!(page(&client, &root).await.items.is_empty());
            let _ = listener.cancel().await;
        }
        // SQL deadline selection covers the entire visible set, not just page one.
        let owner_client = f.sdk(&f.owner).await;
        for _ in 0..101 { f.artifacts.put(&f.owner, PutArtifactRequest::default(), b"page".to_vec()).await.unwrap(); }
        let first = page(&owner_client, &root).await;
        assert_eq!(first.items.len(), 100);
        assert!(first.next_cursor.is_some());
        assert!(!first.items.iter().any(|item| item.uri == member));
        let retention = chrono::Utc::now() + chrono::TimeDelta::seconds(8);
        db.b.client().query(include_str!("../../../../queries/bin/server/subscription_tests/mod/access_loss_invalidates_members_and_roots_for_revocation_and_expiry_3.surql"))
            .bind(("expiry", retention)).bind(("artifact", veoveo_platform_store::ArtifactId::from_uuid(id.as_uuid()).record_id()))
            .await.unwrap().check().unwrap();
        assert_eq!(f.subscriptions.deadline(&f.owner, None).await.unwrap(), retention);
        assert_eq!(f.subscriptions.deadline(&f.owner, Some(&[])).await.unwrap(), f.owner.identity.expires_at,
            "an empty selection has no stored access deadline");
        assert_eq!(f.subscriptions.deadline(&f.owner, Some(&[id])).await.unwrap(), retention);
        let mut listener = owner_client.listen(SubscriptionFilter::builder().resource_subscription(root.as_str()).build()).await.unwrap();
        updates(&mut listener, &[&root]).await;
        loop {
            updates(&mut listener, &[&root]).await;
            if chrono::Utc::now() >= retention { break; }
        }
        assert!(denied(read(&owner_client, &member, None).await));
        let _ = listener.cancel().await;
        client.cancel().await.unwrap();
        owner_client.cancel().await.unwrap();
        drop(f);
        drop(db);
    }).await.expect("Artifact source fixture exceeded 240 seconds");
}
