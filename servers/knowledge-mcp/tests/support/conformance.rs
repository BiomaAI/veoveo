//! The shared checker owns protocol and schema assertions; domain tests own access.
use super::*;
use std::collections::BTreeSet;
use veoveo_mcp_conformance::*;

#[tokio::test]
async fn hosted_contract_and_immutable_knowledge_documents_conform() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = native_database().await;
        let control = plane(&[]);
        install(&db.a, &control).await;
        let caller = identity(&control);
        directory(&db.a, &caller).await;
        let signing = Signing::new();
        let server =
            Server::new(db.a.clone(), Arc::new(SyntheticEmbeddings::new()), &signing).await;
        let profile = HostedServerConformanceProfile {
            schema_version: HostedServerProfileSchema::V2,
            profile_id: "knowledge-native".into(),
            contract_revision: HOSTED_MCP_CONTRACT_REVISION.into(),
            endpoint: format!("{}/mcp", server.base),
            server_slug: "knowledge".into(),
            owned_resource_schemes: BTreeSet::from(["knowledge".into()]),
            http: HttpBoundaryProfile {
                require_authentication_rejection: true,
                rejected_host: Some("untrusted.invalid".into()),
                health_url: Some(format!("{}/healthz", server.base)),
                readiness_url: Some(format!("{}/readyz", server.base)),
                docs_llms_url: format!("{}/admin/docs/llms.txt", server.base),
            },
            surfaces: SurfaceProfile {
                tools: SurfaceExpectation::Required,
                resources: SurfaceExpectation::Required,
                resource_templates: SurfaceExpectation::Required,
                prompts: SurfaceExpectation::Forbidden,
                completions: SurfaceExpectation::Required,
                tasks: SurfaceExpectation::Forbidden,
                subscriptions: SurfaceExpectation::Required,
                required_tools: BTreeSet::from(["search".into(), "embed".into()]),
                required_resources: BTreeSet::from([
                    "knowledge://sources".into(),
                    "knowledge://docs".into(),
                    "knowledge://contract".into(),
                ]),
                required_resource_templates: BTreeSet::from([
                    "knowledge://source/{server}".into(),
                    "knowledge://collection/{collection}".into(),
                    "knowledge://docs/{doc_id}".into(),
                ]),
                required_prompts: BTreeSet::new(),
            },
        };
        let credentials = ConformanceCredentials::bearer(signing.issue(caller).bearer_token);
        let report = run_hosted_server_conformance(&profile, &credentials)
            .await
            .unwrap();
        println!("{}", serde_json::to_string(&report).unwrap());
        assert!(report.passed(), "{:#?}", report.checks);
        for id in ["K01", "K02", "K03", "K04", "K05", "K06"] {
            assert!(
                report
                    .checks
                    .iter()
                    .any(|check| check.requirement_id == id && check.status == CheckStatus::Passed),
                "missing {id}"
            );
        }
    })
    .await
    .expect("Knowledge hosted conformance exceeded 180 seconds");
}
