//! A temporary result-only grant exercises installed source and index authorization.
use super::*;
use veoveo_artifact_mcp::contract::{
    ArtifactGrantsOutput, ArtifactResource, GrantArtifactRequest, RevokeArtifactGrantRequest,
};
use veoveo_knowledge_mcp::contract::KnowledgeResource;
use veoveo_types::{AccessLevel, AccessSubject, CanonicalTaskId};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AccessInput {
    source: Input,
    admin_endpoint: String,
    grantee: AccessSubject,
    task: CanonicalTaskId,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AccessReport {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    analysis: AnalysisId,
    grantee: AccessSubject,
    task: CanonicalTaskId,
    changed_collections: Vec<CollectionId>,
}

#[tokio::test]
#[ignore = "requires a disposable completed analysis, Artifact administrator and independently scoped reviewer"]
async fn result_grants_change_source_and_search_access_without_granting_task_control() -> Result<()>
{
    let _ = rustls::crypto::ring::default_provider().install_default();
    let input: AccessInput = serde_json::from_slice(&fs::read(input_path(
        "VEOVEO_REASON_KNOWLEDGE_ACCESS_INPUT",
    )?)?)?;
    let output_path = input_path("VEOVEO_REASON_KNOWLEDGE_OUTPUT")?;
    ensure!(!output_path.exists(), "acceptance output already exists");
    let mut owner = connect(&input.source.endpoint, "VEOVEO_REASON_KNOWLEDGE_TOKEN_FILE").await?;
    let mut admin = connect(
        &input.admin_endpoint,
        "VEOVEO_REASON_KNOWLEDGE_ADMIN_TOKEN_FILE",
    )
    .await?;
    let mut reviewer = connect(
        &input.source.endpoint,
        "VEOVEO_REASON_KNOWLEDGE_REVIEWER_TOKEN_FILE",
    )
    .await?;
    // Each mutation has one dispatch. Once the absent-grant precondition is proven,
    // cleanup runs even when dispatch acknowledgement or a later assertion fails.
    let result = cycle(owner.peer(), admin.peer(), reviewer.peer(), &input).await;
    let closed = tokio::join!(owner.close(), admin.close(), reviewer.close());
    let changed_collections = result?;
    closed.0?;
    closed.1?;
    closed.2?;
    write_report(
        output_path,
        &AccessReport {
            schema: "veoveo.ai/reason-knowledge-access-acceptance/v1",
            completed_at: Utc::now(),
            analysis: input.source.analysis,
            grantee: input.grantee,
            task: input.task,
            changed_collections,
        },
    )
}

async fn cycle(
    owner: &Peer<RoleClient>,
    admin: &Peer<RoleClient>,
    reviewer: &Peer<RoleClient>,
    input: &AccessInput,
) -> Result<Vec<CollectionId>> {
    let analysis_uri = AnalysisUri::new(input.source.analysis).to_uri();
    let analysis: AnalysisView = decode_resource(
        &owner
            .read_resource(ReadResourceRequestParams::new(analysis_uri.as_str()))
            .await?,
        &analysis_uri,
    )?;
    let output = analysis
        .output()
        .context("analysis has no completed output")?;
    let task = owner
        .get_task(GetTaskParams::new(input.task.as_str()))
        .await?;
    let TaskPayload::Completed { result } = task.task.payload else {
        anyhow::bail!("acceptance requires a completed Task");
    };
    let result: CallToolResult = serde_json::from_value(serde_json::to_value(result)?)?;
    let task_output: AnalyzeRecordingOutput = serde_json::from_value(
        result
            .structured_content
            .context("completed Task omitted its output")?,
    )?;
    ensure!(
        task_output.analysis_id() == input.source.analysis,
        "Task route belongs to another analysis"
    );
    // The reference fixture changes the selected Work Context of one machine
    // principal. Reason analysis resources use principal/profile ownership;
    // public Task routes also require the creating Work Context.
    require_denial(
        reviewer
            .get_task(GetTaskParams::new(input.task.as_str()))
            .await,
        "reviewer tasks/get before the grant",
    )?;
    let artifact = output.results_artifact.artifact_id();
    let grants_uri = ArtifactResource::Grants(artifact).to_uri();
    let existing: ArtifactGrantsOutput = decode_resource(
        &admin
            .read_resource(ReadResourceRequestParams::new(grants_uri.as_str()))
            .await
            .context("administrator could not read the existing result grants")?,
        &grants_uri,
    )?;
    ensure!(
        existing.artifact_id == artifact
            && existing.grants.iter().all(|g| g.subject != input.grantee),
        "test requires an absent grantee; an existing grant must not be altered"
    );
    let mut before = Vec::new();
    for kind in FindingCollection::ALL.iter().copied() {
        let member = FindingResource::Member {
            collection: kind,
            analysis: input.source.analysis,
        }
        .to_uri()?;
        let (_, observation) = read_observed(owner, &member, None).await?;
        deny(reviewer, &member, None).await?;
        wait_search(reviewer, &input.source, kind, None).await?;
        before.push((kind, member, observation));
    }
    let grant = async {
        let receipt: ArtifactGrantsOutput = call(
            admin,
            "artifact__grant_access",
            &GrantArtifactRequest {
                artifact_id: artifact,
                subject: input.grantee.clone(),
                level: AccessLevel::Read,
            },
        )
        .await?;
        ensure!(
            receipt.artifact_id == artifact
                && receipt
                    .grants
                    .iter()
                    .any(|g| g.subject == input.grantee && g.level == AccessLevel::Read),
            "grant receipt omitted the requested read grant"
        );
        require_denial(
            reviewer
                .get_task(GetTaskParams::new(input.task.as_str()))
                .await,
            "reviewer tasks/get",
        )?;
        require_denial(
            reviewer
                .cancel_task(CancelTaskParams::new(input.task.as_str()))
                .await,
            "reviewer tasks/cancel",
        )?;
        deny(
            reviewer,
            &ArtifactResource::Metadata(output.annotations_artifact.artifact_id()).to_uri(),
            None,
        )
        .await?;
        let result_uri = ArtifactResource::Metadata(artifact).to_uri();
        let metadata: veoveo_artifact_contract::ArtifactMetadata = decode_resource(
            &reviewer
                .read_resource(ReadResourceRequestParams::new(result_uri.as_str()))
                .await
                .context("reviewer could not read result metadata after the grant")?,
            &result_uri,
        )?;
        ensure!(
            metadata.artifact_id() == artifact,
            "grant exposed a different Artifact"
        );
        let mut granted = Vec::new();
        for (kind, member, previous) in &before {
            let (read, observation) =
                read_observed(reviewer, member, Some(previous.revision())).await?;
            ensure!(
                !observation.not_modified() && observation.revision() != previous.revision(),
                "grant did not change {member}'s revision"
            );
            let finding: FindingSummary = decode_resource(&read, member)?;
            ensure!(
                finding.analysis_id() == input.source.analysis
                    && finding.result_artifact().artifact_id() == artifact,
                "grant exposed a different finding"
            );
            wait_search(reviewer, &input.source, *kind, Some(observation.revision())).await?;
            granted.push((*kind, member.clone(), observation));
        }
        Ok::<_, anyhow::Error>(granted)
    };
    let granted = tokio::time::timeout(Duration::from_secs(120), grant).await;
    let cleanup: Result<ArtifactGrantsOutput> = call(
        admin,
        "artifact__revoke_access",
        &RevokeArtifactGrantRequest {
            artifact_id: artifact,
            subject: input.grantee.clone(),
        },
    )
    .await;
    let receipt = cleanup
        .context("temporary grant cleanup failed; reconcile this Artifact before retrying")?;
    ensure!(
        receipt.artifact_id == artifact
            && receipt.grants.iter().all(|g| g.subject != input.grantee),
        "temporary grant remains after cleanup"
    );
    let granted = granted
        .context("granted source/index checks exceeded 120 seconds; temporary grant removed")??;
    let mut changed = Vec::new();
    for (kind, member, observation) in granted {
        deny(reviewer, &member, Some(observation.revision())).await?;
        let (_, current) = read_observed(owner, &member, Some(observation.revision())).await?;
        ensure!(
            !current.not_modified() && current.revision() != observation.revision(),
            "revocation did not change {member}'s revision"
        );
        wait_search(owner, &input.source, kind, Some(current.revision())).await?;
        wait_search(reviewer, &input.source, kind, None).await?;
        changed.push(current.collection().clone());
    }
    Ok(changed)
}

async fn call<T: DeserializeOwned>(
    peer: &Peer<RoleClient>,
    name: &'static str,
    input: &impl Serialize,
) -> Result<T> {
    let response = tool_result(peer, name, input).await?;
    serde_json::from_value(
        response
            .structured_content
            .context("tool omitted structured content")?,
    )
    .context("tool output differs from its owner contract")
}

async fn tool_result(
    peer: &Peer<RoleClient>,
    name: &'static str,
    input: &impl Serialize,
) -> Result<CallToolResult> {
    let arguments = serde_json::to_value(input)?
        .as_object()
        .cloned()
        .context("tool input object")?;
    let response = peer
        .call_tool(CallToolRequestParams::new(name).with_arguments(arguments))
        .await?;
    ensure!(
        response.is_error != Some(true),
        "{name} returned a tool error"
    );
    Ok(response)
}

async fn deny(
    peer: &Peer<RoleClient>,
    uri: &ResourceUri,
    revision: Option<&Revision>,
) -> Result<()> {
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
        .await;
    require_denial(response, uri.as_str())
}

fn require_denial<T>(response: Result<T, rmcp::ServiceError>, operation: &str) -> Result<()> {
    match response {
        Err(rmcp::ServiceError::McpError(error))
            if error.code == ErrorCode::INVALID_PARAMS
                || error.code == ErrorCode::INVALID_REQUEST =>
        {
            Ok(())
        }
        Err(error) => Err(error)
            .with_context(|| format!("{operation} failed without an authorization denial")),
        Ok(_) => anyhow::bail!("reviewer was admitted to {operation}"),
    }
}

async fn wait_search(
    peer: &Peer<RoleClient>,
    input: &Input,
    kind: FindingCollection,
    expected: Option<&Revision>,
) -> Result<()> {
    let descriptor = kind.descriptor();
    let collection = KnowledgeResource::Collection(descriptor.collection().clone()).to_uri()?;
    let member = FindingResource::Member {
        collection: kind,
        analysis: input.analysis,
    }
    .to_uri()?;
    let mut subscription = peer
        .listen(
            SubscriptionFilter::builder()
                .resource_subscription(collection.as_str())
                .build(),
        )
        .await?;
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            let request = SearchRequest::new(
                input.query.clone(),
                BTreeSet::from([descriptor.collection().clone()]),
                BTreeSet::new(),
                20,
            )?;
            let tool = tool_result(peer, "knowledge__search", &request).await?;
            let response: SearchResponse = serde_json::from_value(
                tool.structured_content
                    .context("search omitted structured output")?,
            )?;
            let links = tool
                .content
                .iter()
                .filter_map(|item| match item {
                    ContentBlock::ResourceLink(link) => Some(link.uri.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let uris = response
                .results
                .iter()
                .map(|r| r.uri.as_str())
                .collect::<BTreeSet<_>>();
            ensure!(
                uris.len() == response.results.len()
                    && links.len() == uris.len()
                    && links.into_iter().collect::<BTreeSet<_>>() == uris,
                "search links disagree with its admitted results"
            );
            let result = response.results.iter().find(|r| r.uri == member);
            match (expected, result) {
                (None, None) => return Ok::<_, anyhow::Error>(()),
                (Some(revision), Some(result))
                    if result.freshness.revision == *revision && !result.freshness.stale =>
                {
                    return Ok(());
                }
                _ => {}
            }
            subscription
                .next()
                .await?
                .context("catalog observation ended before search access changed")?;
        }
    })
    .await
    .context("search did not reflect the Artifact access change within 45 seconds");
    let closed = subscription.cancel().await;
    result??;
    closed?;
    Ok(())
}
