use super::{CollectionDescriptor, Observation, Result, knowledge, read, text};
use crate::KnowledgeSourceTarget;
use crate::{
    CheckResult,
    knowledge_probes::{
        KnowledgeChange, KnowledgeChangeDriver, KnowledgeChangeProbe, KnowledgeProbes,
        KnowledgeSearchAccess, KnowledgeSearchProbe,
    },
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

#[path = "creation.rs"]
mod creation;

#[cfg(test)]
#[path = "probes_tests.rs"]
mod tests;

pub(super) async fn check(
    client: &Client,
    profile: &KnowledgeSourceTarget,
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
    let visible = super::enumerate(client, descriptor).await?;
    match &probe.change {
        KnowledgeChange::Update {
            members: [first, second],
            driver,
        } => {
            ensure!(
                visible.contains(first) && visible.contains(second),
                "change fixtures are not enumerated"
            );
            let before = observe(client, descriptor, first).await?;
            let survivor = observe(client, descriptor, second).await?;
            mutate(client, *driver, first, &enumeration).await?;
            let changed = observe(client, descriptor, first).await?;
            require_changed(&before, &changed)?;
            driver.restart().await.context("owner restart failed")?;
            let restored = observe(client, descriptor, first).await?;
            require_same(&changed, &restored)?;
            let next = if first == second {
                restored.clone()
            } else {
                let next = observe(client, descriptor, second).await?;
                require_same(&survivor, &next)?;
                next
            };
            mutate(client, *driver, second, &enumeration).await?;
            require_changed(&next, &observe(client, descriptor, second).await?)?;
            if first != second {
                require_same(&restored, &observe(client, descriptor, first).await?)?;
            }
        }
        KnowledgeChange::Remove {
            members: [first, second],
            driver,
        } => {
            ensure!(
                first != second,
                "removal requires two distinct fixture members"
            );
            ensure!(
                visible.contains(first) && visible.contains(second),
                "both removal fixtures must be enumerated before mutation"
            );
            let before = observe(client, descriptor, first).await?;
            let survivor = observe(client, descriptor, second).await?;
            mutate(client, *driver, first, &enumeration).await?;
            removed(client, descriptor, first, &before).await?;
            driver.restart().await.context("owner restart failed")?;
            removed(client, descriptor, first, &before).await?;
            require_same(&survivor, &observe(client, descriptor, second).await?)?;
            mutate(client, *driver, second, &enumeration).await?;
            removed(client, descriptor, second, &survivor).await?;
            removed(client, descriptor, first, &before).await?;
        }
        KnowledgeChange::Create { driver } => {
            creation::check(client, descriptor, &enumeration, visible, *driver).await?;
        }
    }
    Ok(())
}

fn require_changed(before: &Observation, after: &Observation) -> Result<()> {
    ensure!(
        before.revision() != after.revision(),
        "mutation did not change the revision"
    );
    ensure!(
        before.content_sha256() != after.content_sha256() || before.access() != after.access(),
        "mutation did not change text or access"
    );
    Ok(())
}

fn require_same(before: &Observation, after: &Observation) -> Result<()> {
    ensure!(
        before.revision() == after.revision()
            && before.content_sha256() == after.content_sha256()
            && before.access() == after.access(),
        "restart lost committed member state"
    );
    Ok(())
}

async fn removed(
    client: &Client,
    descriptor: &CollectionDescriptor,
    member: &ResourceUri,
    before: &Observation,
) -> Result<()> {
    ensure!(
        !super::enumerate(client, descriptor).await?.contains(member),
        "removed member is still enumerated"
    );
    denied(client, member, before).await
}

async fn denied(client: &Client, uri: &ResourceUri, before: &Observation) -> Result<()> {
    for condition in [None, Some(before.revision())] {
        match read(client, uri, condition).await {
            Err(error) if error.downcast_ref::<rmcp::ServiceError>().is_some_and(|error| matches!(error,
                rmcp::ServiceError::McpError(error) if matches!(error.code,
                    rmcp::model::ErrorCode::INVALID_REQUEST | rmcp::model::ErrorCode::INVALID_PARAMS))) => {}
            _ => anyhow::bail!("full/conditional read did not return a protocol denial"),
        }
    }
    Ok(())
}

async fn mutate(
    client: &Client,
    driver: &dyn KnowledgeChangeDriver,
    member: &ResourceUri,
    enumeration: &ResourceUri,
) -> Result<()> {
    notified(client, &[member, enumeration], async {
        driver.mutate().await.context("owner mutation failed")
    })
    .await
}

async fn notified<T>(
    client: &Client,
    resources: &[&ResourceUri],
    mutation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let mut filter = SubscriptionFilter::builder();
    for resource in resources {
        filter = filter.resource_subscription(resource.as_str());
    }
    let filter = filter.build();
    let mut subscription = client.listen(filter.clone()).await?;
    let result = async {
        ensure!(
            subscription.acknowledged() == &filter,
            "requested resource subscriptions were not all accepted"
        );
        tokio::time::timeout(
            Duration::from_secs(15),
            notifications(&mut subscription, resources),
        )
        .await
        .context("resource observation readiness exceeded 15 seconds")??;
        let value = mutation.await?;
        tokio::time::timeout(
            Duration::from_secs(15),
            notifications(&mut subscription, resources),
        )
        .await
        .context("resource change notification exceeded 15 seconds")??;
        Ok(value)
    }
    .await;
    // Explicit cancellation runs on success and failure; SDK Drop cancels if the
    // outer owner deadline interrupts this entire future.
    let cancellation = subscription.cancel().await;
    let value = result?;
    cancellation?;
    Ok(value)
}

async fn notifications(subscription: &mut Subscription, resources: &[&ResourceUri]) -> Result<()> {
    let requested = resources
        .iter()
        .map(|uri| uri.as_str())
        .collect::<BTreeSet<_>>();
    let mut pending = requested.clone();
    while let Some(notification) = subscription.next().await? {
        match notification {
            ServerNotification::ResourceUpdatedNotification(value) => {
                ensure!(
                    requested.contains(value.params.uri.as_str()),
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
    profile: &KnowledgeSourceTarget,
    descriptors: &[CollectionDescriptor],
    declaration: &SearchDeclaration,
    probe: &KnowledgeSearchProbe,
) -> Result<()> {
    ensure!(
        !probe.expected.is_empty() && probe.expected.len() <= 100,
        "search fixture requires 1..100 ordinary hits"
    );
    let restricted_expected = match &probe.restricted {
        KnowledgeSearchAccess::Results(expected) => expected.clone(),
        KnowledgeSearchAccess::Denied => BTreeSet::new(),
    };
    ensure!(
        restricted_expected.is_subset(&probe.expected)
            && restricted_expected.len() < probe.expected.len(),
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
    search_case(
        client,
        profile,
        descriptors,
        declaration,
        probe,
        &probe.expected,
    )
    .await?;
    let bearer = probe
        .restricted_credentials
        .bearer_token()
        .context("restricted reader needs an out-of-band credential")?;
    let restricted = CertificationClient
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(
                StreamableHttpClientTransportConfig::with_uri(profile.endpoint().as_str())
                    .auth_header(bearer),
            ),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .context("restricted reader discovery failed")?;
    let result = async {
        match &probe.restricted {
            KnowledgeSearchAccess::Results(_) => search_case(&restricted, profile, descriptors, declaration, probe, &restricted_expected).await?,
            KnowledgeSearchAccess::Denied => {
                let result = restricted.call_tool(CallToolRequestParams::new(profile.tool_name(&probe.tool)?)
                    .with_arguments(probe.arguments.clone())).await;
                ensure!(matches!(result, Err(rmcp::ServiceError::McpError(ref error)) if matches!(error.code,
                    rmcp::model::ErrorCode::INVALID_REQUEST | rmcp::model::ErrorCode::INVALID_PARAMS)),
                    "restricted tool did not return a protocol denial");
            }
        }
        for uri in probe.expected.difference(&restricted_expected) {
            let full = read(client, uri, None).await?;
            let observation = knowledge::client::validate_read(&full, uri, None)?.context("ordinary read omitted observation")?;
            denied(&restricted, uri, &observation).await?;
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
    profile: &KnowledgeSourceTarget,
    descriptors: &[CollectionDescriptor],
    declaration: &SearchDeclaration,
    probe: &KnowledgeSearchProbe,
    expected: &BTreeSet<ResourceUri>,
) -> Result<()> {
    let result: CallToolResult = client
        .call_tool(
            CallToolRequestParams::new(profile.tool_name(&probe.tool)?)
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
