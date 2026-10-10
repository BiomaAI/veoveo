use super::{CertificationClient, failed, knowledge};
use crate::{
    ConformanceCredentials, ConformanceReport, ConformanceReportSchema,
    HOSTED_MCP_CONTRACT_REVISION, KnowledgeSourceTarget, ObservedImplementation,
    knowledge_probes::KnowledgeProbes,
};
use anyhow::{Context, Result, ensure};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use std::time::Duration;

/// Execute K01–K08 for one source, including through a gateway's combined catalog.
/// This report is not full hosted-server certification; K09/K10 require owner review.
/// The owner supplies and cleans up every mutation, restart and restricted reader.
pub async fn run_knowledge_source_conformance(
    target: &KnowledgeSourceTarget,
    credentials: &ConformanceCredentials,
    probes: &KnowledgeProbes<'_>,
) -> Result<ConformanceReport> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(900), run(target, credentials, probes))
        .await
        .context("source conformance exceeded fifteen minutes")?
}

async fn run(
    target: &KnowledgeSourceTarget,
    credentials: &ConformanceCredentials,
    probes: &KnowledgeProbes<'_>,
) -> Result<ConformanceReport> {
    let started_at = chrono::Utc::now();
    let token = credentials
        .bearer_token()
        .context("source conformance requires a caller token")?;
    ensure!(!token.trim().is_empty(), "caller token is empty");
    let http = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(65))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let connection = CertificationClient
        .serve_with_lifecycle(
            StreamableHttpClientTransport::with_client(
                http,
                StreamableHttpClientTransportConfig::with_uri(target.endpoint().as_str())
                    .auth_header(token.to_owned()),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;
    let result = async {
        let info = connection
            .peer_info()
            .context("source discovery omitted implementation")?;
        let implementation = info
            .server_info
            .as_ref()
            .context("source discovery omitted serverInfo")?;
        let mut checks = Vec::new();
        let declares = info
            .capabilities
            .extensions
            .as_ref()
            .is_some_and(|e| e.contains_key(veoveo_mcp_knowledge_extension::EXTENSION_ID));
        if declares {
            let mut coverage = target
                .gateway_server()
                .map(crate::catalog::SourceCatalogCoverage::new);
            let templates = target.templates(match &mut coverage {
                Some(coverage) => coverage.templates(connection.peer()).await?,
                None => crate::catalog::templates(connection.peer()).await?,
            })?;
            let tools = if info.capabilities.tools.is_some() {
                target.tools(match &mut coverage {
                    Some(coverage) => coverage.tools(connection.peer()).await?,
                    None => crate::catalog::tools(connection.peer()).await?,
                })?
            } else {
                vec![]
            };
            knowledge::check(&connection, target, &templates, &tools, probes, &mut checks).await;
            if let Some(coverage) = coverage.filter(|coverage| coverage.is_limited()) {
                let check = checks
                    .iter_mut()
                    .find(|check| check.requirement_id == "K01")
                    .context("selected source omitted K01 coverage")?;
                let evidence = check.evidence.get_or_insert_with(|| serde_json::json!({}));
                evidence
                    .as_object_mut()
                    .context("K01 evidence must be an object")?
                    .insert(
                        "selectedSourceCatalog".into(),
                        serde_json::to_value(coverage)?,
                    );
            }
        } else {
            checks.push(failed(
                "K01",
                "endpoint does not declare the knowledge-source extension",
            ));
        }
        Ok::<_, anyhow::Error>(ConformanceReport {
            schema_version: ConformanceReportSchema::V1,
            profile_id: format!("knowledge-{}", target.server()),
            contract_revision: HOSTED_MCP_CONTRACT_REVISION.into(),
            started_at,
            completed_at: chrono::Utc::now(),
            implementation: Some(ObservedImplementation {
                name: implementation.name.clone(),
                version: implementation.version.clone(),
                protocol_version: info.protocol_version.to_string(),
            }),
            observed_capabilities: Some(serde_json::to_value(&info.capabilities)?),
            checks,
        })
    }
    .await;
    let closed = connection.cancel().await;
    let report = result?;
    closed?;
    Ok(report)
}
