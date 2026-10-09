//! Installed federated discovery and the selected owners' well-known documents.
use super::{Evidence, Method, Phase, SelectedServer, Target};
use anyhow::{Result, ensure};
use rmcp::model::*;
use std::collections::BTreeSet;
use veoveo_mcp_conformance::client::failure::ObservedFailure;
use veoveo_mcp_contract::docs::{ContractDeclaration, knowledge_extension};
use veoveo_testing_support::SmokeMcpClient;
use veoveo_types::Vocabulary as _;
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceUri, ResourceUriBuilder, ServerSlug};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, veoveo_types::Vocabulary)]
pub(super) enum SelectedOwner {
    #[vocabulary(rename = "duckdb")]
    DuckDb,
    Timeseries,
    Frames,
    Media,
}
impl SelectedOwner {
    pub(super) fn slug(self) -> ServerSlug {
        ServerSlug::parse(self.as_str()).expect("selected owner declaration")
    }
    pub(super) fn scheme(self) -> ResourceScheme {
        match self {
            Self::DuckDb => veoveo_duckdb_mcp::uris::SCHEME.clone(),
            Self::Timeseries => veoveo_timeseries_mcp::uris::SCHEME.clone(),
            Self::Frames => veoveo_frames_mcp::uris::SCHEME.clone(),
            Self::Media => veoveo_media_mcp::uris::SCHEME.clone(),
        }
    }
    fn docs(self) -> &'static str {
        match self {
            Self::DuckDb => veoveo_duckdb_mcp::uris::DOCS_URI,
            Self::Timeseries => veoveo_timeseries_mcp::uris::DOCS_URI,
            Self::Frames => veoveo_frames_mcp::uris::DOCS_URI,
            Self::Media => veoveo_media_mcp::uris::DOCS_URI,
        }
    }
    fn contract(self) -> &'static str {
        match self {
            Self::DuckDb => veoveo_duckdb_mcp::uris::CONTRACT_URI,
            Self::Timeseries => veoveo_timeseries_mcp::uris::CONTRACT_URI,
            Self::Frames => veoveo_frames_mcp::uris::CONTRACT_URI,
            Self::Media => veoveo_media_mcp::uris::CONTRACT_URI,
        }
    }
    fn template(self) -> &'static str {
        match self {
            Self::DuckDb => veoveo_duckdb_mcp::uris::DOC_TEMPLATE,
            Self::Timeseries => veoveo_timeseries_mcp::uris::DOC_TEMPLATE,
            Self::Frames => veoveo_frames_mcp::uris::DOC_TEMPLATE,
            Self::Media => veoveo_media_mcp::uris::DOC_TEMPLATE,
        }
    }
    fn design(self) -> Result<ResourceUri> {
        use veoveo_duckdb_mcp::contract::{DuckDbDocument, DuckDbResource};
        use veoveo_frames_mcp::contract::{FramesDocument, FramesResource};
        use veoveo_media_mcp::contract::{MediaDocument, MediaResource};
        use veoveo_timeseries_mcp::contract::{TimeseriesDocument, TimeseriesResource};
        Ok(match self {
            Self::DuckDb => DuckDbResource::Document(DuckDbDocument::Design).to_uri()?,
            Self::Timeseries => {
                TimeseriesResource::Document(TimeseriesDocument::Design).to_uri()?
            }
            Self::Frames => FramesResource::Document(FramesDocument::Design).to_uri()?,
            Self::Media => MediaResource::Document(MediaDocument::Design).to_uri()?,
        })
    }
    fn nonexistent(self) -> &'static str {
        // Fixed negative routes, deliberately outside each owner's admitted vocabulary.
        match self {
            Self::DuckDb => "duckdb://nonexistent",
            Self::Timeseries => "timeseries://nonexistent",
            Self::Frames => "frames://nonexistent",
            Self::Media => "media://nonexistent",
        }
    }
}

fn resource_text<'a>(response: &'a ReadResourceResult, target: &ResourceUri) -> Result<&'a str> {
    let [ResourceContents::TextResourceContents { uri, text, .. }] = response.contents.as_slice()
    else {
        anyhow::bail!("well-known resource did not return one text member");
    };
    ensure!(
        uri == target.as_str(),
        "well-known resource returned another target"
    );
    ensure!(
        !text.trim().is_empty(),
        "well-known resource returned an empty body"
    );
    Ok(text)
}
fn require_contract(owner: SelectedOwner, declaration: &ContractDeclaration) -> Result<()> {
    ensure!(
        declaration.server() == &owner.slug(),
        "contract declaration has the wrong owner"
    );
    ensure!(
        declaration.contract_revision() == veoveo_mcp_contract::docs::CONTRACT_REVISION,
        "contract declaration has another normative revision"
    );
    Ok(())
}
fn admit_document_page(
    owner: SelectedOwner,
    page: knowledge_extension::docs::DocumentPage,
    after: &mut Option<knowledge_extension::DocumentId>,
    seen: &mut BTreeSet<knowledge_extension::DocumentId>,
) -> Result<Option<knowledge_extension::DocumentId>> {
    ensure!(
        page.items.len() <= knowledge_extension::docs::DOC_PAGE_SIZE,
        "document page exceeds its owner's page size"
    );
    for item in &page.items {
        ensure!(
            after.as_ref().is_none_or(|prior| &item.id > prior),
            "document pages repeat or reorder identities"
        );
        ensure!(
            item.uri == knowledge_extension::docs::member_uri(&owner.scheme(), &item.id),
            "document index returned a foreign owner or member"
        );
        ensure!(
            !item.title.trim().is_empty(),
            "document index returned an empty title"
        );
        ensure!(
            seen.insert(item.id.clone()),
            "document index repeated an identity"
        );
        *after = Some(item.id.clone());
    }
    if let Some(next) = &page.next_cursor {
        ensure!(
            page.items.len() == knowledge_extension::docs::DOC_PAGE_SIZE
                && after.as_ref() == Some(next),
            "document continuation does not identify the final full-page member"
        );
    }
    Ok(page.next_cursor)
}

#[derive(Clone, serde::Serialize)]
#[cfg_attr(test, derive(serde::Deserialize))]
#[serde(rename_all = "camelCase")]
pub(super) struct ToolCatalogObservation {
    owner: SelectedOwner,
    server: ServerSlug,
    expected: BTreeSet<veoveo_gateway_contract::GatewayToolName>,
    actual: Vec<veoveo_gateway_contract::GatewayToolName>,
    missing: BTreeSet<veoveo_gateway_contract::GatewayToolName>,
    unexpected: BTreeSet<veoveo_gateway_contract::GatewayToolName>,
    matched: bool,
}

fn tool_catalog_observations(
    selected: &[SelectedServer],
    tools: &[Tool],
) -> Result<Vec<ToolCatalogObservation>> {
    let mut admitted = tools
        .iter()
        .map(|tool| {
            let name = veoveo_gateway_contract::GatewayToolName::parse(tool.name.as_ref())?;
            let (owner, _) = name.parts()?;
            Ok((owner, name))
        })
        .collect::<Result<Vec<_>>>()?;
    admitted.sort();
    Ok(selected
        .iter()
        .map(|server| {
            let actual = admitted
                .iter()
                .filter(|(owner, _)| owner == &server.slug)
                .map(|(_, name)| name.clone())
                .collect::<Vec<_>>();
            let names = actual.iter().cloned().collect::<BTreeSet<_>>();
            ToolCatalogObservation {
                owner: server.owner,
                server: server.slug.clone(),
                expected: server.expected_tools.clone(),
                matched: names == server.expected_tools && actual.len() == names.len(),
                missing: server.expected_tools.difference(&names).cloned().collect(),
                unexpected: names.difference(&server.expected_tools).cloned().collect(),
                actual,
            }
        })
        .collect())
}

pub(super) async fn run(
    client: &SmokeMcpClient,
    selected: &[SelectedServer],
    evidence: &mut Evidence,
) -> Result<()> {
    use veoveo_mcp_conformance::catalog;
    let tools = evidence
        .success(
            Phase::Discovery,
            Method::ToolsList,
            Target::Gateway,
            catalog::tools(client.peer()),
        )
        .await?;
    // Retain every owner's result before any catalog assertion can stop the case.
    // The complete name vector preserves duplicates as well as missing/extra names.
    evidence.catalog_observations(&tool_catalog_observations(selected, &tools)?)?;
    let resources = evidence
        .success(
            Phase::Discovery,
            Method::ResourcesList,
            Target::Gateway,
            catalog::resources(client.peer()),
        )
        .await?;
    let templates = evidence
        .success(
            Phase::Discovery,
            Method::ResourceTemplatesList,
            Target::Gateway,
            catalog::templates(client.peer()),
        )
        .await?;
    // Empty domain prompt catalogs are legitimate; complete traversal and absence
    // of degradation establish discovery, not a fabricated minimum prompt count.
    let prompts = evidence
        .success(
            Phase::Discovery,
            Method::PromptsList,
            Target::Gateway,
            catalog::prompts(client.peer()),
        )
        .await?;
    require_catalogs(selected, &tools, &resources, &templates, &prompts)?;
    for server in selected {
        let owner = server.owner;
        let mut after = None;
        let mut seen = BTreeSet::new();
        let mut complete = false;
        for _ in 0..32 {
            let mut builder = ResourceUriBuilder::new(owner.docs())?;
            if let Some(cursor) = &after {
                builder = builder
                    .query_pair("cursor", knowledge_extension::DocumentId::as_str(cursor))?;
            }
            let uri = builder.build()?;
            let response = read(client, &uri, evidence).await?;
            let page: knowledge_extension::docs::DocumentPage =
                serde_json::from_str(resource_text(&response, &uri)?)
                    .map_err(|_| anyhow::anyhow!("document index failed typed admission"))?;
            let next = admit_document_page(owner, page, &mut after, &mut seen)?;
            if next.is_none() {
                complete = true;
                break;
            }
        }
        ensure!(complete, "document index exceeds 32-page traversal budget");
        let design_id = knowledge_extension::DocumentId::parse("design")?;
        ensure!(
            seen.contains(&design_id),
            "document index omits the owner's design"
        );
        let design_uri = owner.design()?;
        let response = read(client, &design_uri, evidence).await?;
        resource_text(&response, &design_uri)?;
        let uri = ResourceUri::new(owner.contract())?;
        let response = read(client, &uri, evidence).await?;
        let declaration: ContractDeclaration =
            serde_json::from_str(resource_text(&response, &uri)?)
                .map_err(|_| anyhow::anyhow!("served contract failed typed admission"))?;
        require_contract(owner, &declaration)?;
        let response = evidence
            .success(
                Phase::Discovery,
                Method::CompletionComplete,
                Target::ResourceTemplate(veoveo_types::ResourceTemplateUri::new(owner.template())?),
                async {
                    Ok(client
                        .complete(CompleteRequestParams::new(
                            Reference::for_resource(owner.template()),
                            ArgumentInfo::new("doc_id", "de"),
                        ))
                        .await?)
                },
            )
            .await?;
        require_design_completion(&response.completion, &seen)?;
        let nonexistent = ResourceUri::new(owner.nonexistent())?;
        evidence
            .deny(
                Phase::Discovery,
                Method::ResourcesRead,
                Target::Resource(nonexistent.clone()),
                ObservedFailure::mcp(
                    i64::from(ErrorCode::INVALID_PARAMS.0),
                    "unknown resource address",
                ),
                client.read_resource(ReadResourceRequestParams::new(nonexistent.as_str())),
            )
            .await?;
    }
    Ok(())
}
async fn read(
    client: &SmokeMcpClient,
    uri: &ResourceUri,
    evidence: &mut Evidence,
) -> Result<ReadResourceResult> {
    evidence
        .success(
            Phase::Discovery,
            Method::ResourcesRead,
            Target::Resource(uri.clone()),
            async {
                Ok(client
                    .read_resource(ReadResourceRequestParams::new(uri.as_str()))
                    .await?)
            },
        )
        .await
}
fn require_catalogs(
    selected: &[SelectedServer],
    tools: &[Tool],
    resources: &[Resource],
    templates: &[ResourceTemplate],
    prompts: &[Prompt],
) -> Result<()> {
    let actual_tools = tools
        .iter()
        .map(|tool| {
            veoveo_gateway_contract::GatewayToolName::parse(tool.name.as_ref())
                .map_err(anyhow::Error::from)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let resource_ids = resources
        .iter()
        .map(|resource| ResourceUri::new(&resource.uri))
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let template_ids = templates
        .iter()
        .map(|template| veoveo_types::ResourceTemplateUri::new(&template.uri_template))
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let actual_prompts = prompts
        .iter()
        .map(|prompt| {
            veoveo_mcp_contract::PromptName::parse(&prompt.name).map_err(anyhow::Error::from)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        actual_tools.len() == tools.len()
            && resource_ids.len() == resources.len()
            && template_ids.len() == templates.len()
            && actual_prompts.len() == prompts.len(),
        "complete catalogs repeat an identity"
    );
    let owners = actual_tools
        .iter()
        .map(|tool| tool.parts().map(|(owner, _)| owner))
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    let mut seen = BTreeSet::new();
    for server in selected {
        ensure!(
            seen.insert(server.owner),
            "selected discovery owner repeated"
        );
        ensure!(
            server.slug == server.owner.slug() && server.scheme == server.owner.scheme(),
            "selected discovery identity differs from its owner"
        );
        ensure!(
            owners.contains(&server.slug),
            "complete tool catalog omits a selected owner"
        );
        let selected_tools = actual_tools
            .iter()
            .filter(|tool| tool.parts().is_ok_and(|(owner, _)| owner == server.slug))
            .cloned()
            .collect::<BTreeSet<_>>();
        ensure!(
            selected_tools == server.expected_tools,
            "selected owner's complete tools differ from checked configuration"
        );
        ensure!(
            server.expected_prompts.is_subset(&actual_prompts),
            "complete prompt catalog omits a configured selected prompt"
        );
        for uri in [server.owner.docs(), server.owner.contract()] {
            ensure!(
                resources.iter().any(|resource| resource.uri == uri),
                "complete resource catalog omits a selected well-known resource"
            );
        }
        ensure!(
            templates
                .iter()
                .any(|template| template.uri_template == server.owner.template()),
            "complete template catalog omits a selected docs template"
        );
    }
    ensure!(
        seen == SelectedOwner::ALL.iter().copied().collect(),
        "discovery requires all four selected owners"
    );
    Ok(())
}
fn require_design_completion(
    completion: &CompletionInfo,
    documents: &BTreeSet<knowledge_extension::DocumentId>,
) -> Result<()> {
    let expected = documents
        .iter()
        .filter(|id| id.as_str().contains("de"))
        .map(|id| id.as_str())
        .collect::<BTreeSet<_>>();
    let actual = completion
        .values
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    ensure!(
        actual == expected && actual.contains("design") && actual.len() == completion.values.len(),
        "document completion differs from the complete owner index"
    );
    ensure!(
        completion.has_more == Some(false) && completion.total == Some(expected.len().try_into()?),
        "document completion did not report the complete admitted result"
    );
    Ok(())
}

pub(super) struct AdminPromptProfile {
    pub mode: veoveo_mcp_contract::DiscoveryFailureMode,
    pub attempted_servers: BTreeSet<ServerSlug>,
    pub healthy_servers: BTreeSet<ServerSlug>,
    pub healthy_prompts: BTreeSet<veoveo_mcp_contract::PromptName>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum PromptIsolationStatus {
    Qualified,
    UnqualifiedProfileMode,
    UnqualifiedNoUpstreamFailure,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PromptIsolationObservation {
    pub status: PromptIsolationStatus,
    pub failures: Vec<veoveo_gateway_contract::GatewayDiscoveryFailure>,
    pub prompt_count: usize,
}
pub(super) async fn prompt_isolation(
    client: &SmokeMcpClient,
    profile: &AdminPromptProfile,
    evidence: &mut Evidence,
) -> Result<PromptIsolationObservation> {
    if profile.mode != veoveo_mcp_contract::DiscoveryFailureMode::Isolate {
        return Ok(PromptIsolationObservation {
            status: PromptIsolationStatus::UnqualifiedProfileMode,
            failures: Vec::new(),
            prompt_count: 0,
        });
    }
    let mut cursor = None;
    let mut cursors = BTreeSet::new();
    let mut prompts = BTreeSet::new();
    let mut degradation = veoveo_gateway_contract::GatewayDiscoveryDegradation::default();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    for _ in 0..1024 {
        let page = evidence
            .success(
                Phase::Administration,
                Method::PromptsList,
                Target::Gateway,
                async {
                    Ok(tokio::time::timeout_at(
                        deadline,
                        client.list_prompts(Some(
                            PaginatedRequestParams::default().with_cursor(cursor.clone()),
                        )),
                    )
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("admin prompt traversal exceeded thirty seconds")
                    })??)
                },
            )
            .await?;
        let observed = read_prompt_degradation(&page)?;
        evidence.prompt_degradation(&observed)?;
        admit_isolated_prompt_page(profile, &page, observed, &mut prompts, &mut degradation)?;
        let Some(next) = page.next_cursor else {
            return finish_prompt_isolation(profile, prompts, degradation);
        };
        ensure!(
            cursors.insert(next.clone()),
            "admin prompt traversal repeated a cursor"
        );
        cursor = Some(next);
    }
    anyhow::bail!("admin prompt traversal exceeded 1024 pages")
}
fn admit_isolated_prompt_page(
    profile: &AdminPromptProfile,
    page: &ListPromptsResult,
    observed: veoveo_gateway_contract::GatewayDiscoveryDegradation,
    prompts: &mut BTreeSet<veoveo_mcp_contract::PromptName>,
    degradation: &mut veoveo_gateway_contract::GatewayDiscoveryDegradation,
) -> Result<()> {
    use veoveo_gateway_contract::GatewayDiscoverySurface;
    ensure!(
        prompts.len() + page.prompts.len() <= 16384,
        "admin prompt catalog exceeds 16384 items"
    );
    for prompt in &page.prompts {
        ensure!(
            prompts.insert(veoveo_mcp_contract::PromptName::parse(&prompt.name)?),
            "admin prompt catalog repeats an identity"
        );
    }
    ensure!(
        observed.failures.iter().all(|failure| {
            failure.surface == GatewayDiscoverySurface::Prompts
                && profile.attempted_servers.contains(&failure.server)
                && !profile.healthy_servers.contains(&failure.server)
        }),
        "admin prompt degradation names a wrong surface, unattempted or healthy server"
    );
    degradation.merge(observed);
    Ok(())
}
fn read_prompt_degradation(
    page: &ListPromptsResult,
) -> Result<veoveo_gateway_contract::GatewayDiscoveryDegradation> {
    use veoveo_gateway_contract::GATEWAY_DISCOVERY_DEGRADATION_META_KEY;
    page.meta
        .as_ref()
        .and_then(|meta| meta.get(GATEWAY_DISCOVERY_DEGRADATION_META_KEY))
        .map(|value| {
            serde_json::from_value(value.clone())
                .map_err(|_| anyhow::anyhow!("admin prompt degradation failed typed admission"))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}
fn finish_prompt_isolation(
    profile: &AdminPromptProfile,
    prompts: BTreeSet<veoveo_mcp_contract::PromptName>,
    degradation: veoveo_gateway_contract::GatewayDiscoveryDegradation,
) -> Result<PromptIsolationObservation> {
    ensure!(
        profile.healthy_prompts.is_subset(&prompts),
        "isolated prompt discovery omitted a configured healthy prompt"
    );
    let qualified = degradation.failures.iter().any(|failure| {
        failure.code == veoveo_gateway_contract::GatewayDiscoveryFailureCode::UpstreamUnavailable
    });
    Ok(PromptIsolationObservation {
        status: if qualified {
            PromptIsolationStatus::Qualified
        } else {
            PromptIsolationStatus::UnqualifiedNoUpstreamFailure
        },
        failures: degradation.failures,
        prompt_count: prompts.len(),
    })
}

pub(super) async fn admin_documents(
    client: &reqwest::Client,
    admin_token: &str,
    installation: &super::InstalledTarget,
    selected: &[SelectedServer],
    evidence: &mut Evidence,
) -> Result<()> {
    let identity = installation.administrator()?;
    for server in selected {
        let index = installation.public_url(&[
            "admin",
            identity.profile.as_str(),
            "servers",
            server.slug.as_str(),
            "docs",
            "llms.txt",
        ])?;
        let bytes = evidence
            .http_get(client, index, Some(admin_token), None, 200)
            .await?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| anyhow::anyhow!("admin document index is not UTF-8"))?;
        require_admin_index(server.owner, text)?;
        let body = installation.public_url(&[
            "admin",
            identity.profile.as_str(),
            "servers",
            server.slug.as_str(),
            "docs",
            "design",
        ])?;
        let bytes = evidence
            .http_get(client, body, Some(admin_token), None, 200)
            .await?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| anyhow::anyhow!("admin design document is not UTF-8"))?;
        ensure!(
            text.lines()
                .any(|line| line == "## Standards And Protocols"),
            "admin design document omits its declared standards section"
        );
    }
    Ok(())
}
fn require_admin_index(owner: SelectedOwner, text: &str) -> Result<()> {
    let mut lines = text.lines();
    ensure!(
        lines.next() == Some(format!("# {}", owner.slug()).as_str()),
        "admin document index has another owner"
    );
    ensure!(
        lines.any(|line| line == "- [Domain design](design)"),
        "admin document index omits its declared design member"
    );
    Ok(())
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    fn selected() -> Result<Vec<SelectedServer>> {
        SelectedOwner::ALL
            .iter()
            .copied()
            .map(|owner| {
                let tool = veoveo_gateway_contract::GatewayToolName::from_parts(
                    &owner.slug(),
                    &veoveo_types::LocalToolName::parse("probe")?,
                )?;
                Ok(SelectedServer {
                    owner,
                    slug: owner.slug(),
                    scheme: owner.scheme(),
                    mount_path: veoveo_mcp_contract::MountPath::new(format!("/{}", owner.slug()))?,
                    origin: url::Url::parse("http://127.0.0.1:1234")?,
                    allowed_host: "fixture:1234".into(),
                    workload: super::super::WorkloadIdentity {
                        deployment_uid: uuid::Uuid::new_v4(),
                        pod_uid: uuid::Uuid::new_v4(),
                        image_digest: veoveo_types::Sha256Digest::from_bytes([1; 32]),
                    },
                    expected_tools: BTreeSet::from([tool]),
                    expected_prompts: BTreeSet::new(),
                })
            })
            .collect()
    }
    fn catalogs(selected: &[SelectedServer]) -> (Vec<Tool>, Vec<Resource>, Vec<ResourceTemplate>) {
        let tools = selected
            .iter()
            .flat_map(|server| server.expected_tools.iter())
            .map(|name| Tool::new(name.to_string(), "Fixture", JsonObject::new()))
            .collect();
        let resources = selected
            .iter()
            .flat_map(|server| [server.owner.docs(), server.owner.contract()])
            .map(|uri| Resource::new(uri, "Document"))
            .collect();
        let templates = selected
            .iter()
            .map(|server| ResourceTemplate::new(server.owner.template(), "Documents"))
            .collect();
        (tools, resources, templates)
    }
    #[test]
    fn catalogs_require_every_configured_owner_and_refuse_missing_or_foreign_items() -> Result<()> {
        let selected = selected()?;
        let (mut tools, mut resources, templates) = catalogs(&selected);
        require_catalogs(&selected, &tools, &resources, &templates, &[])?;
        tools[0].name = "foreign__probe".into();
        assert!(require_catalogs(&selected, &tools, &resources, &templates, &[]).is_err());
        let (tools, _, _) = catalogs(&selected);
        resources.remove(0);
        assert!(require_catalogs(&selected, &tools, &resources, &templates, &[]).is_err());
        assert!(require_catalogs(&selected, &tools, &resources, &templates[..3], &[]).is_err());
        let mut mismatched = selected.clone();
        mismatched[0].scheme = SelectedOwner::Media.scheme();
        let (_, resources, _) = catalogs(&selected);
        assert!(require_catalogs(&mismatched, &tools, &resources, &templates, &[]).is_err());
        let mut duplicate = resources.clone();
        duplicate.push(resources[0].clone());
        assert!(require_catalogs(&selected, &tools, &duplicate, &templates, &[]).is_err());
        let mut duplicate = tools.clone();
        duplicate.push(tools[0].clone());
        assert!(require_catalogs(&selected, &duplicate, &resources, &templates, &[]).is_err());
        Ok(())
    }
    #[test]
    fn tool_observations_keep_all_owners_missing_extra_and_duplicate_names() -> Result<()> {
        let selected = selected()?;
        let (mut tools, resources, templates) = catalogs(&selected);
        tools.pop(); // The last selected owner is Media.
        tools.push(Tool::new(
            "duckdb__unexpected",
            "Fixture",
            JsonObject::new(),
        ));
        let observations = tool_catalog_observations(&selected, &tools)?;
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("catalog-receipt.json");
        let mut evidence = Evidence::create(&path)?;
        evidence.catalog_observations(&observations)?;
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PersistedCatalogs {
            schema: String,
            tool_catalog_observations: Vec<ToolCatalogObservation>,
        }
        let persisted: PersistedCatalogs = serde_json::from_slice(&std::fs::read(&path)?)?;
        assert_eq!(persisted.schema, "veoveo.ai/installed-protocol/v2");
        assert_eq!(persisted.tool_catalog_observations.len(), 4);
        for observation in &persisted.tool_catalog_observations {
            match observation.owner {
                SelectedOwner::DuckDb => {
                    assert!(!observation.matched);
                    assert!(observation.missing.is_empty());
                    assert_eq!(
                        observation.unexpected,
                        BTreeSet::from([veoveo_gateway_contract::GatewayToolName::parse(
                            "duckdb__unexpected"
                        )?,])
                    );
                }
                SelectedOwner::Media => {
                    assert!(!observation.matched);
                    assert_eq!(observation.missing, observation.expected);
                    assert!(observation.actual.is_empty());
                }
                SelectedOwner::Timeseries | SelectedOwner::Frames => {
                    assert!(observation.matched);
                }
            }
        }
        assert!(require_catalogs(&selected, &tools, &resources, &templates, &[]).is_err());
        let (mut tools, _, _) = catalogs(&selected);
        tools.push(tools[0].clone());
        let observations = tool_catalog_observations(&selected, &tools)?;
        let duplicate = observations
            .iter()
            .find(|item| item.owner == SelectedOwner::DuckDb)
            .expect("every selected owner is observed");
        assert!(!duplicate.matched);
        assert_eq!(duplicate.actual.len(), 2);
        assert!(duplicate.missing.is_empty() && duplicate.unexpected.is_empty());
        Ok(())
    }
    fn document(
        owner: SelectedOwner,
        id: &str,
    ) -> Result<knowledge_extension::docs::DocumentEntry> {
        let id = knowledge_extension::DocumentId::parse(id)?;
        Ok(knowledge_extension::docs::DocumentEntry {
            uri: knowledge_extension::docs::member_uri(&owner.scheme(), &id),
            id,
            title: "Fixture document".into(),
        })
    }
    #[test]
    fn document_owner_cursor_and_completion_checks_refuse_consistent_wrong_responses() -> Result<()>
    {
        use knowledge_extension::docs::DocumentPage;
        for &owner in SelectedOwner::ALL {
            let design = document(owner, "design")?;
            assert_eq!(design.uri, owner.design()?);
            let mut after = None;
            let mut seen = BTreeSet::new();
            admit_document_page(
                owner,
                DocumentPage {
                    items: vec![design.clone()],
                    next_cursor: None,
                },
                &mut after,
                &mut seen,
            )?;
            let completion = CompletionInfo::with_pagination(vec!["design".into()], Some(1), false)
                .map_err(anyhow::Error::msg)?;
            require_design_completion(&completion, &seen)?;
            let wrong = CompletionInfo::with_pagination(vec!["agents".into()], Some(1), false)
                .map_err(anyhow::Error::msg)?;
            assert!(require_design_completion(&wrong, &seen).is_err());
            assert!(
                admit_document_page(
                    owner,
                    DocumentPage {
                        items: vec![design.clone()],
                        next_cursor: None
                    },
                    &mut after,
                    &mut seen
                )
                .is_err()
            );
            assert!(
                admit_document_page(
                    owner,
                    DocumentPage {
                        items: vec![design.clone()],
                        next_cursor: Some(design.id.clone())
                    },
                    &mut None,
                    &mut BTreeSet::new()
                )
                .is_err()
            );
            let mut foreign = design;
            foreign.uri = ResourceUri::new("foreign://docs/design")?;
            assert!(
                admit_document_page(
                    owner,
                    DocumentPage {
                        items: vec![foreign],
                        next_cursor: None
                    },
                    &mut None,
                    &mut BTreeSet::new()
                )
                .is_err()
            );
        }
        Ok(())
    }
    #[test]
    fn wrong_contract_and_administrator_index_owner_cannot_establish_discovery() -> Result<()> {
        let profile = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/duckdb-mcp/contract-compliance.json"
        )))?;
        let declaration = ContractDeclaration::new(profile);
        require_contract(SelectedOwner::DuckDb, &declaration)?;
        assert!(require_contract(SelectedOwner::Frames, &declaration).is_err());
        require_admin_index(
            SelectedOwner::DuckDb,
            "# duckdb\n\n## Docs\n- [Domain design](design)\n",
        )?;
        assert!(
            require_admin_index(
                SelectedOwner::Frames,
                "# duckdb\n- [Domain design](design)\n"
            )
            .is_err()
        );
        assert!(
            require_admin_index(
                SelectedOwner::DuckDb,
                "# duckdb\n- [Agent work manual](agents)\n"
            )
            .is_err()
        );
        Ok(())
    }
    fn admin_profile() -> Result<AdminPromptProfile> {
        Ok(AdminPromptProfile {
            mode: veoveo_mcp_contract::DiscoveryFailureMode::Isolate,
            attempted_servers: BTreeSet::from([
                ServerSlug::parse("media")?,
                ServerSlug::parse("frames")?,
            ]),
            healthy_servers: BTreeSet::from([ServerSlug::parse("frames")?]),
            healthy_prompts: BTreeSet::new(),
        })
    }
    fn partial_page(
        server: &str,
        code: veoveo_gateway_contract::GatewayDiscoveryFailureCode,
    ) -> Result<ListPromptsResult> {
        use veoveo_gateway_contract::*;
        let degradation = GatewayDiscoveryDegradation::new([GatewayDiscoveryFailure {
            server: ServerSlug::parse(server)?,
            surface: GatewayDiscoverySurface::Prompts,
            code,
        }]);
        let mut page = ListPromptsResult::default();
        let mut meta = MetaObject::new();
        meta.insert(
            GATEWAY_DISCOVERY_DEGRADATION_META_KEY.into(),
            serde_json::to_value(degradation)?,
        );
        page.meta = Some(meta);
        Ok(page)
    }
    #[test]
    fn isolation_requires_actual_registered_failure_and_keeps_healthy_prompt_agreement()
    -> Result<()> {
        use veoveo_gateway_contract::{GatewayDiscoveryDegradation, GatewayDiscoveryFailureCode};
        let mut profile = admin_profile()?;
        let mut prompts = BTreeSet::new();
        let mut degradation = GatewayDiscoveryDegradation::default();
        let observed = finish_prompt_isolation(&profile, prompts.clone(), degradation.clone())?;
        assert!(matches!(
            observed.status,
            PromptIsolationStatus::UnqualifiedNoUpstreamFailure
        ));
        let page = partial_page("media", GatewayDiscoveryFailureCode::DiscoveryPending)?;
        admit_isolated_prompt_page(
            &profile,
            &page,
            read_prompt_degradation(&page)?,
            &mut prompts,
            &mut degradation,
        )?;
        let observed = finish_prompt_isolation(&profile, prompts.clone(), degradation.clone())?;
        assert!(matches!(
            observed.status,
            PromptIsolationStatus::UnqualifiedNoUpstreamFailure
        ));
        let page = partial_page("media", GatewayDiscoveryFailureCode::UpstreamUnavailable)?;
        admit_isolated_prompt_page(
            &profile,
            &page,
            read_prompt_degradation(&page)?,
            &mut prompts,
            &mut degradation,
        )?;
        let observed = finish_prompt_isolation(&profile, prompts.clone(), degradation.clone())?;
        assert!(matches!(observed.status, PromptIsolationStatus::Qualified));
        for wrong in ["unregistered", "frames"] {
            let page = partial_page(wrong, GatewayDiscoveryFailureCode::UpstreamUnavailable)?;
            assert!(
                admit_isolated_prompt_page(
                    &profile,
                    &page,
                    read_prompt_degradation(&page)?,
                    &mut prompts,
                    &mut degradation
                )
                .is_err()
            );
        }
        profile
            .healthy_prompts
            .insert(veoveo_mcp_contract::PromptName::parse("configured_prompt")?);
        assert!(finish_prompt_isolation(&profile, prompts, degradation).is_err());
        Ok(())
    }
}
