//! Read-only installed acceptance over a completed GPU analysis supplied by the operator.
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use rmcp::{
    ClientServiceExt, Peer, RoleClient,
    model::*,
    service::PeerRequestOptions,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeSet, fs, io::Write, path::PathBuf, time::Duration};
use veoveo_embedding_contract::EmbeddingText;
use veoveo_knowledge_mcp::contract::{GenerationId, SearchRequest, SearchResponse};
use veoveo_mcp_knowledge_extension::{CollectionId, Observation, Revision, client};
use veoveo_reason_mcp::{contract::*, knowledge::summary};
use veoveo_types::{ResourceAddress, ResourceUri};

#[path = "installed/access.rs"]
mod access;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input {
    endpoint: String,
    analysis: AnalysisId,
    query: EmbeddingText,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FindingCheck {
    collection: CollectionId,
    uri: ResourceUri,
    revision: Revision,
    generation: GenerationId,
    rank: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    analysis: AnalysisId,
    query: EmbeddingText,
    findings: Vec<FindingCheck>,
}

fn input_path(name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os(name).with_context(
        || format!("{name} must name an installation-owned file"),
    )?))
}

#[tokio::test]
#[ignore = "requires deployed Reason, Knowledge, hardware embeddings and a completed GPU analysis"]
async fn completed_analysis_findings_are_retrievable_with_current_source_revisions() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(180), async {
        let input: Input =
            serde_json::from_slice(&fs::read(input_path("VEOVEO_REASON_KNOWLEDGE_INPUT")?)?)?;
        let output = input_path("VEOVEO_REASON_KNOWLEDGE_OUTPUT")?;
        ensure!(!output.exists(), "acceptance output already exists");
        let mut connection = connect(&input.endpoint, "VEOVEO_REASON_KNOWLEDGE_TOKEN_FILE").await?;
        let result = verify(connection.peer(), &input).await;
        let closed = connection.close().await;
        let findings = result?;
        closed?;
        let report = Report {
            schema: "veoveo.ai/reason-knowledge-installed-acceptance/v1",
            completed_at: Utc::now(),
            analysis: input.analysis,
            query: input.query,
            findings,
        };
        write_report(output, &report)
    })
    .await
    .context("installed Reason finding acceptance exceeded 180 seconds")?
}

async fn connect(
    endpoint: &str,
    token_variable: &str,
) -> Result<rmcp::service::RunningService<RoleClient, ClientConfig>> {
    let endpoint = reqwest::Url::parse(endpoint)?;
    ensure!(
        endpoint.scheme() == "https"
            && endpoint.username().is_empty()
            && endpoint.password().is_none(),
        "installed acceptance requires HTTPS without URL credentials"
    );
    let token = fs::read_to_string(input_path(token_variable)?)?;
    ensure!(!token.trim().is_empty(), "caller token file is empty");
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(65))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        StreamableHttpClientTransportConfig::with_uri(endpoint.as_str())
            .auth_header(token.trim().to_owned()),
    );
    let mut capabilities = ClientCapabilities::builder().enable_tasks().build();
    client::declare(&mut capabilities);
    Ok(ClientConfig::new(
        capabilities,
        Implementation::new("reason-knowledge-installed-acceptance", "1"),
    )
    .serve_with_lifecycle(
        transport,
        rmcp::ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await?)
}

fn write_report(output: PathBuf, report: &impl Serialize) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(output)?;
    serde_json::to_writer_pretty(&mut file, report)?;
    file.write_all(b"\n")?;
    Ok(())
}

async fn verify(peer: &Peer<RoleClient>, input: &Input) -> Result<Vec<FindingCheck>> {
    let analysis_uri = AnalysisUri::new(input.analysis).to_uri();
    let analysis: AnalysisView = decode_resource(
        &peer
            .read_resource(ReadResourceRequestParams::new(analysis_uri.as_str()))
            .await?,
        &analysis_uri,
    )?;
    let output = analysis
        .output()
        .context("analysis has no completed output")?;
    ensure!(analysis.error.is_none(), "analysis has a recorded failure");
    let mut checks = Vec::new();
    for kind in FindingCollection::ALL {
        let descriptor = summary::collection(kind);
        let member = FindingResource::Member {
            collection: kind,
            analysis: input.analysis,
        }
        .to_uri()?;
        let (read, observation) = read_observed(peer, &member, None).await?;
        let finding: FindingSummary = decode_resource(&read, &member)?;
        let expected = FindingSummary::new(
            kind,
            input.analysis,
            output.results_artifact.artifact_id(),
            analysis.details().created_at.parse()?,
            analysis.details().updated_at.parse()?,
            &output.finding,
        )?;
        ensure!(
            finding == expected && observation.collection() == descriptor.collection(),
            "finding {member} differs from its completed output or collection"
        );
        let (_, conditional) = read_observed(peer, &member, Some(observation.revision())).await?;
        ensure!(
            conditional.not_modified(),
            "unchanged finding was not conditional"
        );
        let request = SearchRequest::new(
            input.query.clone(),
            BTreeSet::from([descriptor.collection().clone()]),
            BTreeSet::new(),
            10,
        )?;
        let response = peer
            .call_tool(
                CallToolRequestParams::new("knowledge__search").with_arguments(
                    serde_json::to_value(&request)?
                        .as_object()
                        .cloned()
                        .context("search request must be an object")?,
                ),
            )
            .await?;
        ensure!(response.is_error != Some(true), "Knowledge search failed");
        let search: SearchResponse = serde_json::from_value(
            response
                .structured_content
                .context("search omitted structured content")?,
        )?;
        let links = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ResourceLink(link) => Some(link.uri.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let uris = search
            .results
            .iter()
            .map(|r| r.uri.as_str())
            .collect::<BTreeSet<_>>();
        ensure!(
            search.results.len() <= 10
                && uris.len() == search.results.len()
                && links.len() == uris.len()
                && links.into_iter().collect::<BTreeSet<_>>() == uris,
            "search resource links differ from unique result addresses"
        );
        for result in &search.results {
            ensure!(
                result.collection == *descriptor.collection()
                    && !result.freshness.stale
                    && result.freshness.observed_at <= Utc::now()
                    && result.score.is_finite()
                    && result.snippet.chars().count() <= 320,
                "invalid search result in {}",
                descriptor.collection()
            );
        }
        let (rank, result) = search.results.iter().enumerate().find(|(_, r)| r.uri == member)
            .with_context(|| format!("{member} is absent from the first ten results; confirm source indexing is complete"))?;
        let (_, current) = read_observed(peer, &member, None).await?;
        ensure!(
            result.freshness.revision == *observation.revision()
                && current.revision() == observation.revision(),
            "retrieved finding {member} has a different source revision"
        );
        checks.push(FindingCheck {
            collection: descriptor.collection().clone(),
            uri: member,
            revision: current.revision().clone(),
            generation: search.generation.context("search omitted its generation")?,
            rank: rank + 1,
        });
    }
    Ok(checks)
}

fn decode_resource<T: DeserializeOwned>(read: &ReadResourceResult, uri: &ResourceUri) -> Result<T> {
    let [
        ResourceContents::TextResourceContents {
            text,
            uri: returned,
            ..
        },
    ] = read.contents.as_slice()
    else {
        anyhow::bail!("{uri} requires exactly one text item");
    };
    ensure!(
        returned == uri.as_str(),
        "resource response has the wrong URI"
    );
    serde_json::from_str(text).context("resource differs from its owner contract")
}

async fn read_observed(
    peer: &Peer<RoleClient>,
    uri: &ResourceUri,
    revision: Option<&Revision>,
) -> Result<(ReadResourceResult, Observation)> {
    let (request, options) = client::read_request(
        ReadResourceRequestParams::new(uri.as_str()),
        ClientCapabilities::default(),
        revision,
        PeerRequestOptions::default(),
    );
    let response = peer
        .send_request_with_option(request, options)
        .await?
        .await_response()
        .await?;
    let ServerResult::ReadResourceResult(read) = response else {
        anyhow::bail!("{uri} did not return a terminal resource response");
    };
    let observation = client::validate_read(&read, uri, revision)?
        .with_context(|| format!("{uri} omitted its Knowledge observation"))?;
    Ok((read, observation))
}
