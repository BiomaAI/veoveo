use super::{CollectionDescriptor, Observation, Result, knowledge, read, text};
use crate::{
    CheckResult, HostedServerConformanceProfile,
    knowledge_probes::{KnowledgeChangeProbe, KnowledgeProbes, KnowledgeSearchProbe},
    runner::{CertificationClient, Client, failed, passed, skipped},
};
use anyhow::{Context, ensure};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ContentBlock, ServerNotification,
        SubscriptionFilter, Tool,
    },
    service::Subscription,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_mcp_knowledge_extension::{ChangeSignal, SearchDeclaration, SearchResults};
use veoveo_types::ResourceUri;

#[cfg(test)]
#[path = "probes_tests.rs"]
mod tests;

pub(super) async fn check(
    client: &Client,
    profile: &HostedServerConformanceProfile,
    descriptors: &[CollectionDescriptor],
    tools: &[Tool],
    probes: &KnowledgeProbes<'_>,
    checks: &mut Vec<CheckResult>,
) {
    let listening: Vec<_> = descriptors
        .iter()
        .filter(|d| d.change_signal() == ChangeSignal::Listen)
        .collect();
    if listening.is_empty() && probes.changes.is_empty() {
        checks.push(skipped("K07", "no listen collections are declared"));
    } else {
        let selection = probes
            .changes
            .iter()
            .map(|p| &p.collection)
            .collect::<BTreeSet<_>>();
        if selection.len() != probes.changes.len()
            || selection != listening.iter().map(|d| d.collection()).collect()
        {
            checks.push(failed(
                "K07",
                "each listen collection requires exactly one owner change/restart probe",
            ));
        } else {
            for probe in &probes.changes {
                let descriptor = listening
                    .iter()
                    .find(|d| d.collection() == &probe.collection)
                    .unwrap();
                let result = tokio::time::timeout(
                    Duration::from_secs(90),
                    change(client, descriptor, probe),
                )
                .await;
                checks.push(outcome("K07", &probe.collection.to_string(), result));
            }
        }
    }

    let declarations = tools
        .iter()
        .filter_map(|tool| {
            tool.meta
                .as_ref()?
                .get(knowledge::EXTENSION_ID)
                .map(|value| {
                    serde_json::from_value::<SearchDeclaration>(value.clone())
                        .map(|declaration| (tool.name.as_ref(), declaration))
                })
        })
        .collect::<std::result::Result<BTreeMap<_, _>, _>>();
    let declarations = match declarations {
        Ok(declarations) => declarations,
        Err(_) => {
            checks.push(failed("K08", "invalid knowledge search declaration"));
            return;
        }
    };
    if declarations.is_empty() && probes.searches.is_empty() {
        checks.push(skipped("K08", "no knowledge search tools are declared"));
        return;
    }
    let selection = probes
        .searches
        .iter()
        .map(|p| p.tool.as_str())
        .collect::<BTreeSet<_>>();
    if selection.len() != probes.searches.len()
        || selection != declarations.keys().copied().collect()
    {
        checks.push(failed(
            "K08",
            "each declared search requires exactly one owner search/denial probe",
        ));
        return;
    }
    for probe in &probes.searches {
        let declaration = &declarations[probe.tool.as_str()];
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            search(client, profile, descriptors, declaration, probe),
        )
        .await;
        checks.push(outcome("K08", probe.tool.as_str(), result));
    }
}

fn outcome(
    id: &str,
    name: &str,
    result: std::result::Result<Result<()>, tokio::time::error::Elapsed>,
) -> CheckResult {
    match result {
        Ok(Ok(())) => passed(
            id,
            format!("{name}: owner probe qualified"),
            Some(json!({"probe": name})),
        ),
        Ok(Err(error)) => failed(id, format!("{name}: {error}")),
        Err(_) => failed(id, format!("{name}: owner probe exceeded its deadline")),
    }
}

async fn observe(
    client: &Client,
    descriptor: &CollectionDescriptor,
    uri: &ResourceUri,
) -> Result<Observation> {
    let result = read(client, uri, None).await?;
    text(&result)?;
    let observation = knowledge::client::validate_read(&result, uri, None)?
        .context("probe member omitted observation")?;
    observation.validate_collection(descriptor)?;
    Ok(observation)
}

async fn change(
    client: &Client,
    descriptor: &CollectionDescriptor,
    probe: &KnowledgeChangeProbe<'_>,
) -> Result<()> {
    let enumeration = descriptor.enumerate().expand_scalars(&BTreeMap::new())?;
    ensure!(
        super::enumerate(client, descriptor)
            .await?
            .contains(&probe.member),
        "change fixture is not enumerated"
    );
    let before = observe(client, descriptor, &probe.member).await?;
    let changed = mutate(client, descriptor, probe, &enumeration, &before).await?;
    // No open listen request is carried across a service restart. Public reads
    // must work through the same endpoint without cached results.
    probe
        .driver
        .restart()
        .await
        .context("owner restart failed")?;
    let restored = observe(client, descriptor, &probe.member).await?;
    ensure!(
        restored.revision() == changed.revision()
            && restored.content_sha256() == changed.content_sha256()
            && restored.access() == changed.access(),
        "restart lost committed member state"
    );
    mutate(client, descriptor, probe, &enumeration, &restored).await?;
    Ok(())
}

async fn mutate(
    client: &Client,
    descriptor: &CollectionDescriptor,
    probe: &KnowledgeChangeProbe<'_>,
    enumeration: &ResourceUri,
    before: &Observation,
) -> Result<Observation> {
    let filter = SubscriptionFilter::builder()
        .resource_subscription(probe.member.as_str())
        .resource_subscription(enumeration.as_str())
        .build();
    let mut subscription = client.listen(filter.clone()).await?;
    let result = async {
        ensure!(
            subscription.acknowledged() == &filter,
            "member and collection subscriptions were not both accepted"
        );
        tokio::time::timeout(
            Duration::from_secs(15),
            notifications(&mut subscription, &probe.member, enumeration),
        )
        .await
        .context("member/collection observation readiness exceeded 15 seconds")??;
        probe
            .driver
            .mutate()
            .await
            .context("owner mutation failed")?;
        tokio::time::timeout(
            Duration::from_secs(15),
            notifications(&mut subscription, &probe.member, enumeration),
        )
        .await
        .context("member/collection change notification exceeded 15 seconds")??;
        let after = observe(client, descriptor, &probe.member).await?;
        ensure!(
            before.revision() != after.revision(),
            "mutation did not change the revision"
        );
        ensure!(
            before.content_sha256() != after.content_sha256() || before.access() != after.access(),
            "mutation did not change text or access"
        );
        Ok(after)
    }
    .await;
    // Explicit cancellation runs on success and failure; SDK Drop cancels if the
    // outer owner deadline interrupts this entire future.
    let cancellation = subscription.cancel().await;
    let after = result?;
    cancellation?;
    Ok(after)
}

async fn notifications(
    subscription: &mut Subscription,
    member: &ResourceUri,
    enumeration: &ResourceUri,
) -> Result<()> {
    let mut pending = BTreeSet::from([member.as_str(), enumeration.as_str()]);
    while let Some(notification) = subscription.next().await? {
        match notification {
            ServerNotification::ResourceUpdatedNotification(value) => {
                ensure!(
                    value.params.uri == member.as_str() || value.params.uri == enumeration.as_str(),
                    "source notified an unrequested resource"
                );
                pending.remove(value.params.uri.as_str());
            }
            _ => anyhow::bail!("unexpected notification during knowledge change probe"),
        }
        if pending.is_empty() {
            return Ok(());
        }
    }
    anyhow::bail!("change subscription ended before notification")
}

async fn search(
    client: &Client,
    profile: &HostedServerConformanceProfile,
    descriptors: &[CollectionDescriptor],
    declaration: &SearchDeclaration,
    probe: &KnowledgeSearchProbe,
) -> Result<()> {
    ensure!(
        !probe.expected.is_empty() && probe.expected.len() <= 100,
        "search fixture requires 1..100 ordinary hits"
    );
    ensure!(
        probe.restricted_expected.is_subset(&probe.expected)
            && probe.restricted_expected.len() < probe.expected.len(),
        "search fixture must deny at least one ordinary hit to its restricted reader"
    );
    ensure!(
        serde_json::to_vec(&probe.arguments)?.len() <= 64 * 1024,
        "search fixture arguments exceed 64 KiB"
    );
    for collection in declaration.collections() {
        ensure!(
            descriptors.iter().any(|d| d.collection() == collection),
            "search names an undeclared collection"
        );
    }
    search_case(client, descriptors, declaration, probe, &probe.expected).await?;
    let bearer = probe
        .restricted_credentials
        .bearer_token()
        .context("restricted reader needs an out-of-band credential")?;
    let restricted = CertificationClient
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(profile.endpoint.clone())
                    .auth_header(bearer),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .context("restricted reader discovery failed")?;
    let result = async {
        search_case(&restricted, descriptors, declaration, probe, &probe.restricted_expected).await?;
        for uri in probe.expected.difference(&probe.restricted_expected) {
            let full = read(client, uri, None).await?;
            let observation = knowledge::client::validate_read(&full, uri, None)?.context("ordinary read omitted observation")?;
            for condition in [None, Some(observation.revision())] {
                match read(&restricted, uri, condition).await {
                    Err(error) if error.downcast_ref::<rmcp::ServiceError>().is_some_and(|error| matches!(error,
                        rmcp::ServiceError::McpError(error) if matches!(error.code,
                            rmcp::model::ErrorCode::INVALID_REQUEST | rmcp::model::ErrorCode::INVALID_PARAMS))) => {}
                    _ => anyhow::bail!("restricted full/conditional read did not return a protocol denial"),
                }
            }
        }
        Ok(())
    }.await;
    let cancellation = restricted.cancel().await;
    result?;
    cancellation?;
    Ok(())
}

async fn search_case(
    client: &Client,
    descriptors: &[CollectionDescriptor],
    declaration: &SearchDeclaration,
    probe: &KnowledgeSearchProbe,
    expected: &BTreeSet<ResourceUri>,
) -> Result<()> {
    let result: CallToolResult = client
        .call_tool(
            CallToolRequestParams::new(probe.tool.as_str().to_owned())
                .with_arguments(probe.arguments.clone()),
        )
        .await?;
    ensure!(
        result.is_error != Some(true),
        "search returned a tool error"
    );
    ensure!(
        serde_json::to_vec(&result)?.len() <= 128 * 1024,
        "search response exceeds 128 KiB"
    );
    let structured: SearchResults = serde_json::from_value(
        result
            .structured_content
            .context("search omitted structured results")?,
    )?;
    let uris = structured
        .results()
        .iter()
        .map(|hit| hit.uri().clone())
        .collect::<BTreeSet<_>>();
    ensure!(
        &uris == expected,
        "search returned unexpected, duplicate or inaccessible members"
    );
    ensure!(
        result.content.len() == structured.results().len(),
        "search requires one resource link per hit"
    );
    let mut links = BTreeSet::new();
    for content in result.content {
        let ContentBlock::ResourceLink(link) = content else {
            anyhow::bail!("search content must contain resource links only");
        };
        let uri = ResourceUri::new(link.uri)?;
        let hit = structured
            .results()
            .iter()
            .find(|hit| hit.uri() == &uri)
            .context("search link has no structured hit")?;
        ensure!(
            links.insert(uri)
                && link.title.as_deref() == hit.title()
                && link.description.as_deref() == hit.snippet(),
            "search resource links repeat or disagree with hit titles/snippets"
        );
    }
    for uri in expected {
        let result = read(client, uri, None).await?;
        text(&result)?;
        let observation = knowledge::client::validate_read(&result, uri, None)?
            .context("search hit has no observation")?;
        ensure!(
            declaration.collections().contains(observation.collection()),
            "search hit belongs to an undeclared search collection"
        );
        let descriptor = descriptors
            .iter()
            .find(|d| d.collection() == observation.collection())
            .context("unknown search collection")?;
        observation.validate_collection(descriptor)?;
    }
    Ok(())
}
