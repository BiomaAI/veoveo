//! Production HTTP and Artifact authorization over an isolated Store. No GPU or
//! inference acceptance: completed results are explicit inert test fixtures.
mod auth;
mod data;
mod fixture;
mod probe;
#[path = "../../../../tests/support/finding.rs"]
mod result_fixture;
use fixture::Fixture;
use rmcp::{model::*, service::RunningService};
use std::time::Duration;
use veoveo_mcp_contract::ArtifactPlane;
use veoveo_mcp_knowledge_extension::{self as extension};
use veoveo_reason_mcp::contract::*;
use veoveo_types::{AccessLevel, AccessSubject, ResourceAddress, ResourceUri};

type Client = RunningService<rmcp::RoleClient, ClientConfig>;

#[tokio::test]
async fn unknown_tool_arguments_complete_before_analysis_admission() {
    tokio::time::timeout(Duration::from_secs(120), async {
        let db = crate::store_fixture::TestDb::new().await;
        crate::store_fixture::module_lanes::install(&db.a, vec![veoveo_reason_mcp::schema::module_setup(crate::store_fixture::module_lanes::execution("reason").unwrap()).unwrap()]).await.unwrap();
        let fixture = Fixture::new(db.a.clone()).await;
        let server = fixture.reason(None).await;
        let client = fixture.sdk(server.address, &fixture.owner).await;
        let mut arguments = serde_json::json!({
            "video": {"recording_uri":"recording://recordings/01983da0-0000-7000-8000-000000000000",
                "entity_path":"/camera/front", "timeline":"sensor_time", "range":{"start":10,"end":20}},
            "pipeline_id":"traffic-events",
            "task":{"kind":"detect_events","prompt":"Vehicles entering the intersection"}
        });
        let valid: AnalyzeRecordingRequest = serde_json::from_value(arguments.clone()).unwrap();
        validate_reasoning_task(&valid.task).unwrap();
        validate_sampling(valid.sampling).unwrap();
        validate_decode(valid.decode).unwrap();
        let tasks = veoveo_reason_mcp::task_lookup::bind(veoveo_task_runtime::TaskRuntime::new(db.a.clone(), "reason", "strict-input")).unwrap();
        assert!(tasks.list().await.unwrap().is_empty());
        arguments["undeclared"] = true.into();
        let response = client.call_tool_once(CallToolRequestParams::new("analyze_recording").with_arguments(arguments.as_object().unwrap().clone())).await.unwrap();
        let CallToolResponse::Complete(result) = response else { panic!("malformed input must complete"); };
        assert_eq!(result.is_error, Some(true));
        assert!(serde_json::to_string(&result.content).unwrap().contains("undeclared"));
        assert!(tasks.list().await.unwrap().is_empty());
        assert_eq!(fixture.content_reads.load(std::sync::atomic::Ordering::Relaxed), 0);
        client.cancel().await.unwrap();
        server.stop().await;
    }).await.expect("Reason malformed tool input exceeded 120 seconds");
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
        ServerResult::ReadResourceResult(value) => Ok(value),
        _ => panic!("unexpected resource response"),
    }
}
async fn updates(subscription: &mut rmcp::service::Subscription, expected: &[ResourceUri]) {
    let mut pending = expected
        .iter()
        .map(ResourceUri::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    tokio::time::timeout(Duration::from_secs(15), async {
        while !pending.is_empty() {
            let Some(notification) = subscription.next().await.unwrap() else {
                panic!("finding subscription ended early");
            };
            match notification {
                ServerNotification::ResourceUpdatedNotification(value) => {
                    pending.remove(value.params.uri.as_str());
                }
                other => panic!("resource-only listener received {other:?}"),
            }
        }
    })
    .await
    .expect("missing finding invalidation");
}

#[tokio::test]
async fn findings_conform_across_service_restarts_and_artifact_grant_revocation() {
    tokio::time::timeout(Duration::from_secs(300), async {
        let db = crate::store_fixture::TestDb::new().await;
        crate::store_fixture::module_lanes::install(&db.a, vec![veoveo_reason_mcp::schema::module_setup(crate::store_fixture::module_lanes::execution("reason").unwrap()).unwrap()]).await.unwrap();
        let fixture = Fixture::new(db.a.clone()).await;
        let finding = fixture.finding().await;
        let server = fixture.reason(None).await;
        let driver = probe::Driver::new(fixture.clone(), finding, server);
        probe::certify(&driver).await;
        let address = driver.server.lock().await.as_ref().unwrap().address;
        let reader = fixture.signing.caller("reader", "research");
        let outsider = fixture.signing.caller("outsider", "research");
        let client = fixture.sdk(address, &reader).await;
        let denied = fixture.sdk(address, &outsider).await;
        let root = FindingResource::root(FindingCollection::Results)
            .to_uri()
            .unwrap();
        let member = FindingResource::Member {
            collection: FindingCollection::Results,
            analysis: driver.finding.analysis,
        }
        .to_uri()
        .unwrap();
        assert!(read(&client, &member, None).await.is_err());
        fixture
            .grant(driver.finding.artifact, "reader")
            .await
            .unwrap();
        let result = read(&client, &member, None).await.unwrap();
        let observed = extension::client::validate_read(&result, &member, None)
            .unwrap()
            .unwrap();
        assert_eq!(observed.access().unwrap().work_context.as_str(), "mission");
        assert!(
            read(&denied, &member, Some(observed.revision()))
                .await
                .is_err()
        );
        let result = read(&client, &member, Some(observed.revision()))
            .await
            .unwrap();
        assert!(result.contents.is_empty());
        // A result grant cannot authorize Task controls, annotation bytes, or
        // the owner-scoped analysis resource.
        assert!(
            client
                .get_task(GetTaskParams::new(
                    driver.finding.analysis.task_id().to_string()
                ))
                .await
                .is_err()
        );
        assert!(
            read(
                &client,
                &AnalysisUri::new(driver.finding.analysis).to_uri(),
                None
            )
            .await
            .is_err()
        );
        assert!(
            fixture
                .artifacts
                .get(&reader, &driver.finding.annotation, AccessLevel::Read)
                .await
                .is_err()
        );
        let mut listener = client
            .listen(
                SubscriptionFilter::builder()
                    .resource_subscription(root.as_str())
                    .resource_subscription(member.as_str())
                    .build(),
            )
            .await
            .unwrap();
        updates(&mut listener, &[root.clone(), member.clone()]).await;
        fixture
            .artifacts
            .revoke(
                &fixture.owner,
                &driver.finding.artifact,
                &AccessSubject::Principal(reader.identity.actor.id.clone()),
            )
            .await
            .unwrap();
        updates(&mut listener, &[root.clone(), member.clone()]).await;
        assert!(
            read(&client, &member, Some(observed.revision()))
                .await
                .is_err()
        );
        let page = read(&client, &root, None).await.unwrap();
        let ResourceContents::TextResourceContents { text, .. } = &page.contents[0] else {
            panic!("finding page must be JSON text");
        };
        assert!(
            serde_json::from_str::<FindingPage>(text)
                .unwrap()
                .items
                .is_empty()
        );
        let _ = listener.cancel().await;
        // No database mutation occurs when the deadline passes. The source's
        // timer must invalidate the same admitted identities before closing.
        fixture.grant(driver.finding.artifact, "reader").await.unwrap();
        let artifact = veoveo_platform_store::ArtifactId::from_uuid(driver.finding.artifact.as_uuid()).record_id();
        let principal = veoveo_platform_store::deterministic_principal_id("reason-fixture", "reader").unwrap().record_id();
        let expiry = chrono::Utc::now() + chrono::TimeDelta::seconds(8);
        db.b.client().query(include_str!("../../../../queries/bin/server/hosted_tests/mod/findings_conform_across_service_restarts_and_artifact_grant_revocation.surql"))
            .bind(("expiry", expiry)).bind(("artifact", artifact)).bind(("principal", principal)).await.unwrap().check().unwrap();
        let mut expiring = client.listen(SubscriptionFilter::builder().resource_subscription(root.as_str()).resource_subscription(member.as_str()).build()).await.unwrap();
        updates(&mut expiring, &[root.clone(),member.clone()]).await;
        updates(&mut expiring, &[root.clone(),member.clone()]).await;
        assert!(chrono::Utc::now() >= expiry, "expiry notification arrived before the deadline");
        assert!(read(&client, &member, Some(observed.revision())).await.is_err());
        let _ = expiring.cancel().await;
        let mut large_results = result_fixture::results();
        large_results.answer = ReasoningAnswer::Answer { text: "交通🚘".repeat(120_000) };
        assert!(serde_json::to_vec(&large_results).unwrap().len() > 1024 * 1024);
        let large = fixture.finding_with_results(large_results).await;
        fixture.grant(large.artifact, "reader").await.unwrap();
        for collection in FindingCollection::ALL {
            let uri = FindingResource::Member { collection, analysis: large.analysis }.to_uri().unwrap();
            let response = read(&client, &uri, None).await.unwrap();
            extension::client::validate_read(&response, &uri, None).unwrap().unwrap();
            let ResourceContents::TextResourceContents { text, .. } = &response.contents[0] else { panic!("finding text") };
            assert!(text.len() <= FINDING_SUMMARY_BYTES);
            let summary: FindingSummary = serde_json::from_str(text).unwrap();
            assert_eq!(summary.result_artifact().artifact_id(), large.artifact);
            if collection == FindingCollection::Results {
                let FindingContent::Answer { excerpt } = summary.content() else { panic!("answer excerpt") };
                assert!(excerpt.truncated());
                assert!(excerpt.text().len() <= 4096);
            }
        }
        assert_eq!(fixture.content_reads.load(std::sync::atomic::Ordering::Relaxed), 0,
            "finding reads must not download full Artifact contents");
        client.cancel().await.unwrap();
        denied.cancel().await.unwrap();
        driver.server.lock().await.take().unwrap().stop().await;
    })
    .await
    .expect("Reason hosted source qualification exceeded 300 seconds");
}
