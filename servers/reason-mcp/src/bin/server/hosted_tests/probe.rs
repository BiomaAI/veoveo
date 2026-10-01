use super::{
    data::Finding,
    fixture::{Fixture, HttpServer},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use veoveo_mcp_conformance::{knowledge_probes::*, *};
use veoveo_reason_mcp::contract::{FindingCollection, FindingResource};
use veoveo_types::ResourceAddress;

pub struct Driver {
    pub fixture: Arc<Fixture>,
    pub finding: Finding,
    pub server: tokio::sync::Mutex<Option<HttpServer>>,
    change: AtomicUsize,
}
impl Driver {
    pub fn new(fixture: Arc<Fixture>, finding: Finding, server: HttpServer) -> Self {
        Self {
            fixture,
            finding,
            server: tokio::sync::Mutex::new(Some(server)),
            change: AtomicUsize::new(0),
        }
    }
}
impl KnowledgeChangeDriver for Driver {
    fn mutate(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let change = self.change.fetch_add(1, Ordering::SeqCst);
            self.fixture
                .grant(self.finding.artifact, &format!("probe-reviewer-{change}"))
                .await
        })
    }
    fn restart(&self) -> KnowledgeProbeFuture<'_> {
        Box::pin(async move {
            let mut guard = self.server.lock().await;
            let old = guard.take().unwrap();
            let address = old.address;
            old.stop().await;
            *guard = Some(self.fixture.reason(Some(address)).await);
            // Binding the same endpoint and completing HTTP health establishes
            // readiness without borrowing any pre-restart application state.
            reqwest::Client::new()
                .get(format!("http://{address}/reason/healthz"))
                .timeout(Duration::from_secs(5))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        })
    }
}

pub async fn certify(driver: &Driver) {
    let address = driver.server.lock().await.as_ref().unwrap().address;
    let base = format!("http://{address}/reason");
    let profile = HostedServerConformanceProfile {
        schema_version: HostedServerProfileSchema::V1,
        profile_id: "reason-native-findings".into(),
        contract_revision: HOSTED_MCP_CONTRACT_REVISION.into(),
        endpoint: format!("{base}/mcp"),
        server_slug: "reason".into(),
        owned_resource_schemes: ["reason".into(), "ui".into()].into(),
        http: HttpBoundaryProfile {
            require_authentication_rejection: true,
            rejected_host: Some("untrusted.invalid".into()),
            health_url: Some(format!("{base}/healthz")),
            readiness_url: None,
            docs_llms_url: format!("{base}/admin/docs/llms.txt"),
        },
        surfaces: SurfaceProfile {
            tools: SurfaceExpectation::Required,
            resources: SurfaceExpectation::Required,
            resource_templates: SurfaceExpectation::Required,
            prompts: SurfaceExpectation::Required,
            completions: SurfaceExpectation::Required,
            tasks: SurfaceExpectation::Required,
            subscriptions: SurfaceExpectation::Required,
            required_tools: ["analyze_recording".into()].into(),
            required_resources: [
                "reason://docs".into(),
                "reason://contract".into(),
                "reason://knowledge/analyses".into(),
                "reason://knowledge/results".into(),
            ]
            .into(),
            required_resource_templates: FindingCollection::ALL
                .iter()
                .map(|c| c.member_template().to_owned())
                .collect(),
            required_prompts: [
                "reason-analyze-recording".into(),
                "reason-answer-question".into(),
            ]
            .into(),
        },
    };
    let probes = KnowledgeProbes {
        changes: FindingCollection::ALL
            .into_iter()
            .map(|collection| KnowledgeChangeProbe {
                collection: veoveo_reason_mcp::knowledge::summary::collection(collection)
                    .collection()
                    .clone(),
                member: FindingResource::Member {
                    collection,
                    analysis: driver.finding.analysis,
                }
                .to_uri()
                .unwrap(),
                driver,
            })
            .collect(),
        searches: vec![],
    };
    let report = run_hosted_server_conformance_with_probes(
        &profile,
        &ConformanceCredentials::bearer(driver.fixture.owner.bearer_token.clone()),
        &probes,
    )
    .await
    .unwrap();
    println!("{}", serde_json::to_string(&report).unwrap());
    assert!(report.passed(), "{:#?}", report.checks);
    for id in ["K01", "K02", "K03", "K04", "K05", "K06", "K07"] {
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.requirement_id == id && c.status == CheckStatus::Passed),
            "missing {id}"
        );
    }
    assert_eq!(
        report
            .checks
            .iter()
            .filter(|c| c.requirement_id == "K07" && c.status == CheckStatus::Passed)
            .count(),
        2
    );
}
