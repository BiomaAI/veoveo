//! Read-only installed acceptance. Requires a running hardware embedding runtime.
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
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use veoveo_embedding_contract::{EmbeddingBatch, EmbeddingSpace, EmbeddingText};
use veoveo_knowledge_mcp::contract::*;
use veoveo_mcp_contract::{GatewayControlPlane, GatewayProfileId};
use veoveo_mcp_knowledge_extension::{CollectionId, client, docs};
use veoveo_types::{ResourceAddress, ResourceUri, ServerSlug};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledReport {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    profile: GatewayProfileId,
    generation: GenerationId,
    collections: BTreeSet<CollectionId>,
    statistics: BTreeMap<CollectionId, CollectionStatistics>,
    completed_sources: BTreeSet<ServerSlug>,
    completed_collections: BTreeSet<CollectionId>,
    observed_catalog: BTreeSet<ResourceUri>,
    verified_links: BTreeSet<ResourceUri>,
    embedding_space: EmbeddingSpace,
}

fn env_path(name: &str) -> Result<PathBuf> {
    let path = std::env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("{name} must name an installation-owned file"))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("Knowledge crate must be under servers/")?;
    Ok(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

#[tokio::test]
#[ignore = "requires deployed Knowledge, approved sources, hardware embeddings and a caller token"]
async fn catalog_search_and_source_revisions_agree_through_the_installed_gateway() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(300), async {
        let control: GatewayControlPlane = serde_json::from_slice(&fs::read(env_path(
            "VEOVEO_KNOWLEDGE_ACCEPTANCE_CONTROL_PLANE",
        )?)?)?;
        control.validate()?;
        let profile: GatewayProfileId =
            std::env::var("VEOVEO_KNOWLEDGE_ACCEPTANCE_PROFILE")?.parse()?;
        let selected = control
            .profiles
            .iter()
            .find(|p| p.id == profile)
            .context("acceptance profile is absent from the control plane")?;
        let endpoint = url::Url::parse(selected.protected_resource.as_str())?;
        ensure!(
            endpoint.scheme() == "https",
            "installed acceptance requires HTTPS"
        );
        let output = env_path("VEOVEO_KNOWLEDGE_ACCEPTANCE_OUTPUT")?;
        ensure!(!output.exists(), "acceptance output already exists");
        let token = fs::read_to_string(env_path("VEOVEO_KNOWLEDGE_ACCEPTANCE_TOKEN_FILE")?)?;
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
        let mut capabilities = ClientCapabilities::default();
        client::declare(&mut capabilities);
        let mut connection = ClientConfig::new(
            capabilities,
            Implementation::new("knowledge-installed-acceptance", "1"),
        )
        .serve_with_lifecycle(
            transport,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await?;
        let result = verify(connection.peer(), &control, profile).await;
        let closed = connection.close().await;
        let report = result?;
        closed?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(output)?;
        serde_json::to_writer_pretty(&mut file, &report)?;
        file.write_all(b"\n")?;
        Ok(())
    })
    .await
    .context("installed Knowledge acceptance exceeded 300 seconds")?
}

async fn verify(
    peer: &Peer<RoleClient>,
    control: &GatewayControlPlane,
    profile: GatewayProfileId,
) -> Result<InstalledReport> {
    let expected: BTreeSet<_> = control
        .servers
        .iter()
        .flat_map(|s| s.knowledge.iter().map(|a| a.collection.clone()))
        .collect();
    ensure!(
        !expected.is_empty(),
        "installation approves no Knowledge collections"
    );
    let mut collections = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut after: Option<ServerSlug> = None;
    for page_number in 0..100 {
        let page: SourceCatalogPage = read_json(
            peer,
            &KnowledgeResource::Sources {
                after: after.clone(),
            }
            .to_uri()?,
        )
        .await?;
        for source in page.items {
            ensure!(
                source.entity == CatalogEntity::DataService && source.contract_revision > 0,
                "source catalog declaration is invalid"
            );
            ensure!(
                after.as_ref().is_none_or(|a| source.server > *a)
                    && sources.insert(source.server.clone()),
                "source cursor repeated or regressed"
            );
            ensure!(
                source.uri == KnowledgeResource::Source(source.server).to_uri()?,
                "source catalog returned a non-owner URI"
            );
            for collection in source.collections {
                ensure!(
                    collections.insert(collection),
                    "collection appears under multiple sources"
                );
            }
        }
        match page.next_cursor {
            None => break,
            Some(cursor) => {
                ensure!(
                    page_number < 99 && sources.last() == Some(&cursor),
                    "source catalog did not terminate with a valid cursor"
                );
                after = Some(cursor);
            }
        }
    }
    ensure!(
        collections == expected,
        "caller catalog differs from installation approvals"
    );
    let mut generation = None;
    let mut statistics = BTreeMap::new();
    for collection in &collections {
        let uri = KnowledgeResource::Collection(collection.clone()).to_uri()?;
        let entry: CollectionCatalogEntry = read_json(peer, &uri).await?;
        ensure!(
            entry.entity == CatalogEntity::Dataset
                && entry.uri == uri
                && entry.descriptor.collection() == collection
                && entry.approval.collection == *collection,
            "collection catalog identity differs from its resource"
        );
        if entry.approval.mode == CollectionApproval::Index {
            let active = entry
                .generation
                .context("approved collection has no active index")?;
            ensure!(
                generation.as_ref().is_none_or(|g| *g == active),
                "approved collections disagree on the active generation"
            );
            generation = Some(active);
            statistics.insert(
                collection.clone(),
                entry
                    .statistics
                    .context("active collection omitted caller-visible statistics")?,
            );
        }
    }
    let generation = generation.context("installation has no indexed collections")?;
    let completed_sources = peer
        .complete(CompleteRequestParams::new(
            Reference::for_resource("knowledge://source/{server}"),
            ArgumentInfo::new("server", ""),
        ))
        .await?
        .completion
        .values
        .into_iter()
        .map(|s| s.parse())
        .collect::<Result<BTreeSet<ServerSlug>, _>>()?;
    ensure!(
        completed_sources == sources,
        "source completion differs from visible catalog"
    );
    let mut completed_collections = BTreeSet::new();
    for source in &sources {
        let result = peer
            .complete(CompleteRequestParams::new(
                Reference::for_resource("knowledge://collection/{collection}"),
                ArgumentInfo::new("collection", format!("{source}.")),
            ))
            .await?;
        ensure!(
            result.completion.has_more != Some(true),
            "acceptance requires complete source collection vocabulary"
        );
        for value in result.completion.values {
            completed_collections.insert(value.parse::<CollectionId>()?);
        }
    }
    ensure!(
        completed_collections == collections,
        "collection completion differs from visible catalog"
    );
    let observed_catalog = observe_catalog(peer, &collections).await?;
    let mut verified_links = BTreeSet::new();
    for source in &control.servers {
        let docs = docs::collection(&source.slug, &source.uri_scheme);
        if !source
            .knowledge
            .iter()
            .any(|a| a.collection == *docs.collection() && a.mode == CollectionApproval::Index)
        {
            continue;
        }
        let page: docs::DocumentPage =
            read_json(peer, &docs::index_uri(&source.uri_scheme)).await?;
        let document = page
            .items
            .first()
            .context("approved docs collection is empty")?;
        let request = SearchRequest::new(
            EmbeddingText::new(document.title.clone())?,
            BTreeSet::from([docs.collection().clone()]),
            BTreeSet::new(),
            5,
        )?;
        let response = call(peer, "knowledge__search", &request).await?;
        let search: SearchResponse = decode_tool(&response)?;
        ensure!(
            search.generation == Some(generation)
                && search.results.iter().any(|r| r.uri == document.uri),
            "document title search for {} in {} expected generation {}, got {:?} with members {:?}",
            document.uri,
            docs.collection(),
            generation,
            search.generation,
            search.results.iter().map(|r| &r.uri).collect::<Vec<_>>()
        );
        let links: BTreeSet<_> = response
            .content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::ResourceLink(link) => Some(link.uri.as_str()),
                _ => None,
            })
            .collect();
        let result_uris: BTreeSet<_> = search.results.iter().map(|r| r.uri.as_str()).collect();
        ensure!(
            search.results.len() <= 5
                && result_uris.len() == search.results.len()
                && links == result_uris,
            "search links for {} do not match unique result URIs: results={:?}, links={:?}",
            source.slug,
            result_uris,
            links
        );
        for result in search.results {
            ensure!(
                result.collection == *docs.collection()
                    && !result.freshness.stale
                    && result.freshness.observed_at <= Utc::now()
                    && result.score.is_finite()
                    && result.snippet.chars().count() <= 320,
                "search result contract is invalid"
            );
            let (request, options) = client::read_request(
                ReadResourceRequestParams::new(result.uri.as_str()),
                ClientCapabilities::default(),
                None,
                PeerRequestOptions::default(),
            );
            let response = peer
                .send_request_with_option(request, options)
                .await?
                .await_response()
                .await?;
            let ServerResult::ReadResourceResult(read) = response else {
                anyhow::bail!("source did not return a terminal resource response");
            };
            let observation = client::validate_read(&read, &result.uri, None)?
                .context("source omitted its Knowledge observation")?;
            ensure!(
                observation.collection() == &result.collection
                    && observation.revision() == &result.freshness.revision,
                "indexed revision differs from its immutable source document"
            );
            verified_links.insert(result.uri);
        }
    }
    ensure!(
        !verified_links.is_empty(),
        "no indexed documentation was verified"
    );
    let embed: EmbedResponse = decode_tool(
        &call(
            peer,
            "knowledge__embed",
            &EmbedRequest::Document {
                texts: EmbeddingBatch::new(vec![EmbeddingText::new(
                    "Knowledge installation acceptance",
                )?])?,
            },
        )
        .await?,
    )?;
    ensure!(
        embed.vectors.len() == 1 && embed.vectors[0].space() == &embed.space,
        "embedding output differs from its declared space"
    );
    Ok(InstalledReport {
        schema: "veoveo.ai/knowledge-installed-acceptance/v2",
        completed_at: Utc::now(),
        profile,
        generation,
        collections,
        statistics,
        completed_sources,
        completed_collections,
        observed_catalog,
        verified_links,
        embedding_space: embed.space,
    })
}

async fn observe_catalog(
    peer: &Peer<RoleClient>,
    collections: &BTreeSet<CollectionId>,
) -> Result<BTreeSet<ResourceUri>> {
    let addresses = collections
        .iter()
        .map(|c| KnowledgeResource::Collection(c.clone()).to_uri())
        .collect::<Result<BTreeSet<_>, _>>()?;
    ensure!(
        addresses.len() <= 32,
        "acceptance needs at most 32 catalog subscriptions"
    );
    let mut filter = SubscriptionFilter::builder().resources_list_changed();
    for address in &addresses {
        filter = filter.resource_subscription(address.as_str());
    }
    let mut subscription = peer.listen(filter.build()).await?;
    let observed = tokio::time::timeout(Duration::from_secs(30), async {
        let mut observed = BTreeSet::new();
        let mut inventory = false;
        while observed != addresses || !inventory {
            match subscription
                .next()
                .await?
                .context("catalog stream ended before initial observations")?
            {
                ServerNotification::ResourceUpdatedNotification(update) => {
                    let uri = ResourceUri::new(update.params.uri)?;
                    ensure!(
                        addresses.contains(&uri),
                        "catalog stream returned an unrequested resource"
                    );
                    observed.insert(uri);
                }
                ServerNotification::ResourceListChangedNotification(_) => inventory = true,
                _ => anyhow::bail!("unexpected catalog subscription notification"),
            }
        }
        Ok::<_, anyhow::Error>(observed)
    })
    .await
    .context("catalog initial observations exceeded thirty seconds");
    let closed = subscription.cancel().await;
    let observed = observed??;
    closed?;
    Ok(observed)
}

async fn read_json<T: DeserializeOwned>(peer: &Peer<RoleClient>, uri: &ResourceUri) -> Result<T> {
    let result = peer
        .read_resource(ReadResourceRequestParams::new(uri.as_str()))
        .await?;
    let [
        ResourceContents::TextResourceContents {
            text,
            uri: returned,
            ..
        },
    ] = result.contents.as_slice()
    else {
        anyhow::bail!("resource must contain exactly one text item");
    };
    ensure!(
        returned == uri.as_str(),
        "resource response has the wrong URI"
    );
    serde_json::from_str(text).context("resource does not match its owner contract")
}

async fn call(
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
        "installed {name} returned a tool error"
    );
    Ok(response)
}

fn decode_tool<T: DeserializeOwned>(response: &CallToolResult) -> Result<T> {
    serde_json::from_value(
        response
            .structured_content
            .clone()
            .context("tool omitted structured output")?,
    )
    .context("tool output does not match its owner contract")
}
