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
use serde::{Deserialize, Serialize, de::DeserializeOwned};
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

#[path = "installed/cleanup.rs"]
mod cleanup;
#[path = "installed/cold_start.rs"]
mod cold_start;
#[cfg(test)]
#[path = "../../../../testing/fixtures/knowledge_control.rs"]
mod control_fixture;
#[cfg(test)]
#[path = "installed/controls.rs"]
mod controls;
#[path = "installed/policy.rs"]
mod policy;
#[path = "installed/receipt.rs"]
mod receipt;

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

fn load_control(path: &Path) -> Result<GatewayControlPlane> {
    ensure!(
        fs::metadata(path)?.len() <= 2 * 1024 * 1024,
        "Knowledge control file exceeds2MiB"
    );
    let bytes = fs::read(path)?;
    ensure!(
        bytes.len() <= 2 * 1024 * 1024,
        "Knowledge control file exceeds2MiB"
    );
    let control: GatewayControlPlane = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid Knowledge control plane"))?;
    control
        .validate(&veoveo_gateway_catalog::registry()?)
        .map_err(|_| anyhow::anyhow!("invalid Knowledge control plane"))?;
    Ok(control)
}
fn endpoint(
    control: &GatewayControlPlane,
    profile: &GatewayProfileId,
) -> Result<veoveo_types::HttpsUrl> {
    let selected = control
        .profiles
        .iter()
        .find(|p| &p.id == profile)
        .context("Knowledge selected profile absent")?;
    Ok(veoveo_types::HttpsUrl::parse(
        selected.protected_resource.as_str(),
    )?)
}
async fn connect(
    endpoint: &veoveo_types::HttpsUrl,
    token: &Path,
) -> Result<rmcp::service::RunningService<RoleClient, ClientConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        veoveo_testing_support::installed::knowledge::bearer_header(token)?,
    );
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(65))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        StreamableHttpClientTransportConfig::with_uri(endpoint.as_str()),
    );
    connect_transport(transport).await
}
async fn connect_transport(
    transport: StreamableHttpClientTransport<reqwest::Client>,
) -> Result<rmcp::service::RunningService<RoleClient, ClientConfig>> {
    let mut capabilities = ClientCapabilities::default();
    client::declare(&mut capabilities);
    ClientConfig::new(
        capabilities,
        Implementation::new("knowledge-installed-acceptance", "1"),
    )
    .serve_with_lifecycle(
        transport,
        rmcp::ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await
    .map_err(connection_failure)
}
fn connection_failure(error: rmcp::service::ClientInitializeError) -> anyhow::Error {
    anyhow::Error::from(error).context("Knowledge connection failed")
}
#[tokio::test]
#[ignore = "requires deployed Knowledge, approved sources, hardware embeddings and a caller token"]
async fn catalog_search_and_source_revisions_agree_through_the_installed_gateway() -> Result<()> {
    let control = load_control(&env_path("VEOVEO_KNOWLEDGE_ACCEPTANCE_CONTROL_PLANE")?)?;
    let profile: GatewayProfileId =
        std::env::var("VEOVEO_KNOWLEDGE_ACCEPTANCE_PROFILE")?.parse()?;
    let endpoint = endpoint(&control, &profile)?;
    let token = env_path("VEOVEO_KNOWLEDGE_ACCEPTANCE_TOKEN_FILE")?;
    let journal = receipt::Journal::open(
        &env_path("VEOVEO_KNOWLEDGE_ACCEPTANCE_OUTPUT")?,
        profile.clone(),
    )?;
    let result = veoveo_testing_support::lifecycle::owner::run(async {
        let owned = cleanup::register(&journal)?;
        let result = tokio::time::timeout(Duration::from_secs(300), async {
            let mut handles = owned.lock().await;
            journal
                .acquire(&mut handles.caller, connect(&endpoint, &token))
                .await?;
            handles.opened_caller(&journal)?;
            let peer = handles.caller.as_ref().unwrap().peer().clone();
            verify(&peer, &control, profile, &mut handles, &journal).await
        })
        .await
        .context("Knowledge baseline300second deadline")
        .and_then(|value| value);
        journal.operation(&result)?;
        result
    })
    .await;
    match result {
        Ok(report) => journal.finish(true, Some(report)),
        Err(error) => {
            journal.failure(&error)?;
            journal.finish(false, None)?;
            anyhow::bail!("Knowledge baseline failed; inspect private outcome")
        }
    }
}

async fn verify(
    peer: &Peer<RoleClient>,
    control: &GatewayControlPlane,
    profile: GatewayProfileId,
    handles: &mut cleanup::Handles,
    journal: &receipt::Journal,
) -> Result<InstalledReport> {
    let indexers = control
        .oauth_clients
        .iter()
        .filter_map(|client| client.knowledge_indexing.as_ref())
        .collect::<Vec<_>>();
    let [indexer] = indexers.as_slice() else {
        anyhow::bail!(
            "installed acceptance requires one indexing client in the supplied control plane"
        );
    };
    let expected = &indexer.collections;
    ensure!(
        !expected.is_empty(),
        "installation selects no Knowledge collections"
    );
    let mut collections = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut after: Option<ServerSlug> = None;
    for page_number in 0..100 {
        let page: SourceCatalogPage = read_json(
            peer,
            journal,
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
        &collections == expected,
        "caller catalog differs from indexing-client selections: missing {:?}, unexpected {:?}",
        expected.difference(&collections).collect::<Vec<_>>(),
        collections.difference(expected).collect::<Vec<_>>()
    );
    let mut generation = None;
    let mut statistics = BTreeMap::new();
    for collection in &collections {
        let uri = KnowledgeResource::Collection(collection.clone()).to_uri()?;
        let entry: CollectionCatalogEntry = read_json(peer, journal, &uri).await?;
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
    let completed_sources = journal
        .request(
            receipt::Request::Completion {
                template: veoveo_types::ResourceTemplateUri::new("knowledge://source/{server}")?,
                argument: "server".into(),
                value: String::new(),
            },
            peer.complete(CompleteRequestParams::new(
                Reference::for_resource("knowledge://source/{server}"),
                ArgumentInfo::new("server", ""),
            )),
        )
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
        let result = journal
            .request(
                receipt::Request::Completion {
                    template: veoveo_types::ResourceTemplateUri::new(
                        "knowledge://collection/{collection}",
                    )?,
                    argument: "collection".into(),
                    value: format!("{source}."),
                },
                peer.complete(CompleteRequestParams::new(
                    Reference::for_resource("knowledge://collection/{collection}"),
                    ArgumentInfo::new("collection", format!("{source}.")),
                )),
            )
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
    let observed_catalog = observe_catalog(peer, &collections, handles, journal).await?;
    let mut verified_links = BTreeSet::new();
    for source in &control.servers {
        let docs = docs::collection(&source.slug, &source.uri_scheme);
        if !expected.contains(docs.collection())
            || !source
                .knowledge
                .iter()
                .any(|a| a.collection == *docs.collection() && a.mode == CollectionApproval::Index)
        {
            continue;
        }
        let page: docs::DocumentPage =
            read_json(peer, journal, &docs::index_uri(&source.uri_scheme)).await?;
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
        let response = call(peer, journal, "knowledge__search", &request).await?;
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
            let response = journal
                .request(
                    receipt::Request::Read {
                        uri: result.uri.clone(),
                    },
                    async {
                        peer.send_request_with_option(request, options)
                            .await?
                            .await_response()
                            .await
                    },
                )
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
            journal,
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
    handles: &mut cleanup::Handles,
    journal: &receipt::Journal,
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
    let filter = filter.build();
    journal
        .listen(&mut handles.listener, peer, filter.clone())
        .await?;
    handles.opened_listener(journal)?;
    let subscription = handles.listener.as_mut().unwrap();
    ensure!(
        subscription.acknowledged() == &filter,
        "Knowledge catalog filter changed"
    );
    let observed = tokio::time::timeout(Duration::from_secs(30), async {
        let mut observed = BTreeSet::new();
        let mut inventory = false;
        while observed != addresses || !inventory {
            match journal
                .request(receipt::Request::Notification, subscription.next())
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
    observed?
}

async fn read_json<T: DeserializeOwned>(
    peer: &Peer<RoleClient>,
    journal: &receipt::Journal,
    uri: &ResourceUri,
) -> Result<T> {
    let result = journal
        .request(
            receipt::Request::Read { uri: uri.clone() },
            peer.read_resource(ReadResourceRequestParams::new(uri.as_str())),
        )
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
    ensure!(
        text.len() <= 2 * 1024 * 1024,
        "Knowledge resource exceeds2MiB"
    );
    serde_json::from_str(text)
        .map_err(|_| anyhow::anyhow!("resource does not match owner contract"))
}

async fn call(
    peer: &Peer<RoleClient>,
    journal: &receipt::Journal,
    name: &'static str,
    input: &impl Serialize,
) -> Result<CallToolResult> {
    let arguments = serde_json::to_value(input)?
        .as_object()
        .cloned()
        .context("tool input object")?;
    let response = journal
        .request(
            receipt::Request::Tool {
                name: name.parse()?,
            },
            peer.call_tool(CallToolRequestParams::new(name).with_arguments(arguments)),
        )
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
    .map_err(|_| anyhow::anyhow!("tool output does not match owner contract"))
}
