//! Independently selected Knowledge contributions under an ordinary restricted caller.
use super::*;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::{Exposure, server_contract::McpServerContract};
use veoveo_mcp_knowledge_extension::DocumentId;
use veoveo_types::ResourceTemplateUri;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Expected {
    collections: BTreeSet<CollectionId>,
    tools: BTreeSet<GatewayToolName>,
    resources: BTreeSet<ResourceUri>,
    templates: BTreeSet<ResourceTemplateUri>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    control_plane: PathBuf,
    profile: GatewayProfileId,
    token_file: PathBuf,
    output: PathBuf,
    expected: Expected,
    denied_documents: BTreeSet<DocumentId>,
}
impl Input {
    fn admit(&self, control: &GatewayControlPlane) -> Result<veoveo_types::HttpsUrl> {
        ensure!(
            self.control_plane.is_absolute()
                && self.token_file.is_absolute()
                && self.output.is_absolute(),
            "Knowledge policy fixture paths must be absolute"
        );
        let indexers = control
            .oauth_clients
            .iter()
            .filter_map(|c| c.knowledge_indexing.as_ref())
            .collect::<Vec<_>>();
        let [indexer] = indexers.as_slice() else {
            anyhow::bail!("Knowledge fixture requires one indexing client")
        };
        ensure!(
            self.expected.collections.is_subset(&indexer.collections)
                && self.expected.collections.len() < indexer.collections.len(),
            "Knowledge policy fixture requires approved hidden collection and eligible visible subset"
        );
        ensure!(
            !self.denied_documents.is_empty() && self.denied_documents.len() <= 8,
            "Knowledge policy fixture requires1..8denied documents"
        );
        let profile = control
            .profiles
            .iter()
            .find(|p| p.id == self.profile)
            .context("Knowledge selected profile absent")?;
        let slug: ServerSlug = "knowledge".parse()?;
        let exposure = profile
            .servers
            .iter()
            .find(|s| s.server == slug)
            .context("selected profile does not expose Knowledge")?;
        ensure!(
            exposure.completions == veoveo_mcp_contract::CompletionExposure::Enabled
                && exposed_uri(
                    &exposure.resources,
                    &KnowledgeResource::Sources { after: None }.to_uri()?
                ),
            "selected Knowledge profile must permit catalog and completion requests"
        );
        ensure!(
            control
                .servers
                .iter()
                .filter(|s| !s.knowledge.is_empty())
                .count()
                <= 100
                && self.expected.collections.len() <= 3000,
            "Knowledge fixture traversal budget exceeded"
        );
        let known = veoveo_knowledge_mcp::mcp::KnowledgeContract::resources()?
            .into_iter()
            .map(|r| r.address().to_uri())
            .collect::<Result<BTreeSet<ResourceUri>, _>>()?;
        let templates = veoveo_knowledge_mcp::mcp::KnowledgeContract::resource_templates()?
            .into_iter()
            .map(|r| Ok::<_, anyhow::Error>(r.template().clone()))
            .collect::<Result<BTreeSet<ResourceTemplateUri>, _>>()?;
        for tool in &self.expected.tools {
            let (server, local) = tool.parts()?;
            ensure!(
                server == slug
                    && matches!(local.as_str(), "search" | "embed")
                    && veoveo_policy::exposure_contains(&exposure.tools, &local),
                "expected tool is outside Knowledge exposure"
            );
        }
        for uri in &self.expected.resources {
            KnowledgeResource::parse(uri)?;
            ensure!(
                known.contains(uri) && exposed_uri(&exposure.resources, uri),
                "expected resource is outside Knowledge declarations/exposure"
            );
        }
        for uri in &self.expected.templates {
            ensure!(
                templates.contains(uri) && exposed_template(&exposure.resources, uri),
                "expected template is outside Knowledge declarations/exposure"
            );
        }
        for id in &self.denied_documents {
            let uri = KnowledgeResource::Document(id.clone()).to_uri()?;
            ensure!(
                known.contains(&uri) && !self.expected.resources.contains(&uri),
                "denied document must exist and be excluded from expected resource list"
            );
        }
        ensure!(
            self.expected.resources.len() <= 128
                && self.expected.templates.len() <= 16
                && self.expected.tools.len() <= 2,
            "Knowledge expected discovery budget exceeded"
        );
        endpoint(control, &self.profile)
    }
}
fn exposed_uri(exposure: &Exposure<veoveo_types::ResourceSelector>, uri: &ResourceUri) -> bool {
    match exposure {
        Exposure::All => true,
        Exposure::None => false,
        Exposure::Listed(selectors) => selectors.iter().any(|s| s.matches_uri(uri)),
    }
}
fn exposed_template(
    exposure: &Exposure<veoveo_types::ResourceSelector>,
    uri: &ResourceTemplateUri,
) -> bool {
    match exposure {
        Exposure::All => true,
        Exposure::None => false,
        Exposure::Listed(selectors) => selectors.iter().any(|s| s.matches_template(uri)),
    }
}
#[tokio::test]
#[ignore = "requires unchanged installed control plane and ordinary restricted HTTPS caller; no policy mutation"]
async fn restricted_caller_lists_catalog_and_document_denials_agree_through_installed_gateway()
-> Result<()> {
    let path = std::env::var_os("VEOVEO_KNOWLEDGE_POLICY_INPUT")
        .map(PathBuf::from)
        .context("set VEOVEO_KNOWLEDGE_POLICY_INPUT")?;
    ensure!(
        path.is_absolute() && fs::metadata(&path)?.len() <= 64 * 1024,
        "Knowledge policy input must be absolute and at most64KiB"
    );
    let bytes = fs::read(path)?;
    ensure!(
        bytes.len() <= 64 * 1024,
        "Knowledge policy input exceeds64KiB"
    );
    let input: Input = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("invalid Knowledge policy fixture"))?;
    let control = load_control(&input.control_plane)?;
    let endpoint = input.admit(&control)?;
    let journal = receipt::Journal::open(&input.output, input.profile.clone())?;
    journal.policy(input.expected.clone())?;
    let result = veoveo_testing_support::lifecycle::owner::run(async {
        let owned = cleanup::register(&journal)?;
        let result = tokio::time::timeout(Duration::from_secs(300), async {
            let mut handles = owned.lock().await;
            journal
                .acquire(&mut handles.caller, connect(&endpoint, &input.token_file))
                .await?;
            handles.opened_caller(&journal)?;
            verify(
                handles.caller.as_ref().unwrap().peer(),
                &input,
                &control,
                &journal,
            )
            .await
        })
        .await
        .context("Knowledge policy300second deadline")
        .and_then(|value| value);
        journal.operation(&result)?;
        result
    })
    .await;
    if let Err(error) = &result {
        journal.failure(error)?;
    }
    journal.finish(result.is_ok(), None)?;
    ensure!(
        result.is_ok(),
        "Knowledge policy case failed; inspect private outcome"
    );
    Ok(())
}
fn require_available(meta: Option<&rmcp::model::MetaObject>) -> Result<()> {
    use veoveo_mcp_contract::GatewayDiscoveryMetadata;
    let status = veoveo_gateway_contract::GatewayDiscoveryDegradation::from_meta(meta)
        .map_err(|_| anyhow::anyhow!("invalid gateway discovery status"))?;
    ensure!(
        !status
            .failures
            .iter()
            .any(|f| f.server.as_str() == "knowledge"),
        "Knowledge discovery pending/unavailable cannot establish policy filtering"
    );
    Ok(())
}
fn same<T: Ord>(actual: BTreeSet<T>, expected: &BTreeSet<T>) -> Result<()> {
    ensure!(
        &actual == expected,
        "Knowledge visible set differs from independent expectation"
    );
    Ok(())
}
async fn verify(
    peer: &Peer<RoleClient>,
    input: &Input,
    control: &GatewayControlPlane,
    journal: &receipt::Journal,
) -> Result<()> {
    let slug: ServerSlug = "knowledge".parse()?;
    let mut tools = BTreeSet::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    for page in 0..32 {
        let result = journal
            .request(
                receipt::Request::ToolsList,
                peer.list_tools(
                    cursor
                        .map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
                ),
            )
            .await?;
        require_available(result.meta.as_ref())?;
        ensure!(
            result.tools.len() <= 10_000,
            "discovery page exceeds member budget"
        );
        for tool in result.tools {
            let name: GatewayToolName = tool.name.parse()?;
            let (server, _) = name.parts()?;
            if server == slug {
                ensure!(tools.insert(name), "duplicate Knowledge tool");
            }
        }
        let Some(next) = result.next_cursor else {
            break;
        };
        ensure!(
            page < 31 && seen.insert(next.clone()),
            "tool paging did not terminate"
        );
        cursor = Some(next);
    }
    journal.tools(tools.clone())?;
    same(tools, &input.expected.tools)?;
    let mut resources = BTreeSet::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    for page in 0..32 {
        let result = journal
            .request(
                receipt::Request::ResourcesList,
                peer.list_resources(
                    cursor
                        .map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
                ),
            )
            .await?;
        require_available(result.meta.as_ref())?;
        ensure!(
            result.resources.len() <= 10_000,
            "discovery page exceeds member budget"
        );
        for resource in result.resources {
            let uri: ResourceUri = ResourceUri::new(resource.uri)?;
            if veoveo_types::ResourceUriParts::parse(uri.as_str())?.scheme() == "knowledge" {
                KnowledgeResource::parse(&uri)?;
                ensure!(resources.insert(uri), "duplicate Knowledge resource");
            }
        }
        let Some(next) = result.next_cursor else {
            break;
        };
        ensure!(
            page < 31 && seen.insert(next.clone()),
            "resource paging did not terminate"
        );
        cursor = Some(next);
    }
    journal.resources(resources.clone())?;
    same(resources, &input.expected.resources)?;
    let mut templates = BTreeSet::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    let known = veoveo_knowledge_mcp::mcp::KnowledgeContract::resource_templates()?
        .into_iter()
        .map(|r| Ok::<_, anyhow::Error>(r.template().clone()))
        .collect::<Result<BTreeSet<ResourceTemplateUri>, _>>()?;
    for page in 0..32 {
        let result = journal
            .request(
                receipt::Request::TemplatesList,
                peer.list_resource_templates(
                    cursor
                        .map(|cursor| PaginatedRequestParams::default().with_cursor(Some(cursor))),
                ),
            )
            .await?;
        require_available(result.meta.as_ref())?;
        ensure!(
            result.resource_templates.len() <= 10_000,
            "discovery page exceeds member budget"
        );
        for template in result.resource_templates {
            let uri: ResourceTemplateUri = ResourceTemplateUri::new(template.uri_template)?;
            if known.contains(&uri) {
                ensure!(templates.insert(uri), "duplicate Knowledge template");
            } else {
                ensure!(
                    !veoveo_types::ResourceSelector::Scheme {
                        scheme: "knowledge".parse()?
                    }
                    .matches_template(&uri),
                    "unknown Knowledge template returned"
                );
            }
        }
        let Some(next) = result.next_cursor else {
            break;
        };
        ensure!(
            page < 31 && seen.insert(next.clone()),
            "template paging did not terminate"
        );
        cursor = Some(next);
    }
    journal.templates(templates.clone())?;
    same(templates, &input.expected.templates)?;
    let mut collections = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut after = None;
    for page in 0..100 {
        let uri = KnowledgeResource::Sources {
            after: after.clone(),
        }
        .to_uri()?;
        let result: SourceCatalogPage = read_json(peer, journal, &uri).await?;
        for source in result.items {
            ensure!(
                source.entity == CatalogEntity::DataService
                    && source.contract_revision > 0
                    && source.uri == KnowledgeResource::Source(source.server.clone()).to_uri()?
                    && sources
                        .last()
                        .is_none_or(|previous| source.server > *previous)
                    && sources.insert(source.server.clone()),
                "Knowledge source catalog identity/order invalid"
            );
            for collection in source.collections {
                ensure!(
                    collection.server() == &source.server && collections.insert(collection),
                    "Knowledge collection source/uniqueness invalid"
                );
            }
        }
        let Some(next) = result.next_cursor else {
            break;
        };
        ensure!(
            page < 99 && sources.last() == Some(&next),
            "Knowledge source cursor invalid"
        );
        after = Some(next);
    }
    let expected_sources = input
        .expected
        .collections
        .iter()
        .map(|c| c.server().clone())
        .collect::<BTreeSet<_>>();
    same(sources.clone(), &expected_sources)?;
    journal.collections(collections.clone())?;
    same(collections, &input.expected.collections)?;
    let completed = journal
        .request(
            receipt::Request::Completion {
                template: ResourceTemplateUri::new("knowledge://source/{server}")?,
                argument: "server".into(),
                value: String::new(),
            },
            peer.complete(CompleteRequestParams::new(
                Reference::for_resource("knowledge://source/{server}"),
                ArgumentInfo::new("server", ""),
            )),
        )
        .await?
        .completion;
    ensure!(
        completed.has_more != Some(true),
        "source completion is incomplete"
    );
    let count = completed.values.len();
    let completed_sources = completed
        .values
        .into_iter()
        .map(|s| s.parse())
        .collect::<Result<BTreeSet<ServerSlug>, _>>()?;
    ensure!(
        completed_sources.len() == count,
        "duplicate source completion"
    );
    same(completed_sources, &sources)?;
    let mut completed = BTreeSet::new();
    // Include hidden approved sources too: their prefixes must produce no hidden
    // collection names even when absent from the caller's source catalog.
    let prefixes = control
        .servers
        .iter()
        .filter(|s| !s.knowledge.is_empty())
        .map(|s| s.slug.clone())
        .collect::<BTreeSet<_>>();
    ensure!(
        prefixes.len() <= 100,
        "Knowledge completion prefix budget exceeded"
    );
    for source in prefixes {
        let value = format!("{source}.");
        let result = journal
            .request(
                receipt::Request::Completion {
                    template: ResourceTemplateUri::new("knowledge://collection/{collection}")?,
                    argument: "collection".into(),
                    value: value.clone(),
                },
                peer.complete(CompleteRequestParams::new(
                    Reference::for_resource("knowledge://collection/{collection}"),
                    ArgumentInfo::new("collection", value),
                )),
            )
            .await?
            .completion;
        ensure!(
            result.has_more != Some(true),
            "collection completion is incomplete"
        );
        for name in result.values {
            ensure!(
                completed.insert(name.parse::<CollectionId>()?),
                "duplicate completion collection"
            );
        }
    }
    same(completed, &input.expected.collections)?;
    for id in &input.denied_documents {
        journal
            .denied(peer, KnowledgeResource::Document(id.clone()).to_uri()?)
            .await?;
    }
    // A revoked or expired caller also returns -32600. A successful admitted
    // read afterward prevents that loss of authentication from passing as a
    // selected document's policy restriction.
    let _: SourceCatalogPage = read_json(
        peer,
        journal,
        &KnowledgeResource::Sources { after: None }.to_uri()?,
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(Input, GatewayControlPlane)> {
        let mut control = control_fixture::plane();
        let knowledge = crate::hosted::plane(&[])
            .servers
            .into_iter()
            .find(|s| s.slug.as_str() == "knowledge")
            .unwrap();
        control.servers.push(knowledge);
        control.profiles[0]
            .servers
            .push(veoveo_mcp_contract::ProfileServerExposure {
                server: "knowledge".parse()?,
                tools: Exposure::All,
                resources: Exposure::All,
                prompts: Exposure::None,
                completions: veoveo_mcp_contract::CompletionExposure::Enabled,
                tasks: veoveo_mcp_contract::TaskExposure::Disabled,
            });
        let input: Input = serde_json::from_value(serde_json::json!({
            "controlPlane":"/private/control.json","profile":control.profiles[0].id,"tokenFile":"/private/caller-token","output":"/private/outcome.json",
            "expected":{"collections":[],"tools":[],"resources":[],"templates":[]},"deniedDocuments":["design"]
        }))?;
        Ok((input, control))
    }
    #[test]
    fn closed_policy_fixture_admits_empty_visible_sets_but_rejects_ineligible_or_unrestricted_input()
    -> Result<()> {
        let (mut input, control) = fixture()?;
        input.admit(&control)?;
        let bytes = serde_json::json!({"controlPlane":"/private/control","profile":"operator","tokenFile":"/private/token","output":"/private/outcome","expected":{"collections":[],"tools":[],"resources":[],"templates":[]},"deniedDocuments":["design"],"extra":true});
        ensure!(serde_json::from_value::<Input>(bytes).is_err());
        input.expected.collections.insert("media.records".parse()?);
        ensure!(input.admit(&control).is_err());
        input.expected.collections.clear();
        input.expected.collections.insert("time.calendars".parse()?);
        ensure!(input.admit(&control).is_err());
        input.expected.collections.clear();
        input.expected.tools.insert("media__inspect".parse()?);
        ensure!(input.admit(&control).is_err());
        input.expected.tools.clear();
        input
            .expected
            .templates
            .insert(ResourceTemplateUri::new("knowledge://unknown/{id}")?);
        ensure!(input.admit(&control).is_err());
        input.expected.templates.clear();
        input.denied_documents.clear();
        ensure!(input.admit(&control).is_err());
        input.denied_documents.insert("missing-document".parse()?);
        ensure!(input.admit(&control).is_err());
        Ok(())
    }
    #[test]
    fn independent_sets_and_exact_policy_denial_reject_leaks_and_other_failures() -> Result<()> {
        use veoveo_gateway_contract::{
            GatewayDiscoveryDegradation, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
            GatewayDiscoverySurface,
        };
        use veoveo_mcp_contract::GatewayDiscoveryMetadata;
        let failure = GatewayDiscoveryFailure {
            server: "knowledge".parse()?,
            surface: GatewayDiscoverySurface::Tools,
            code: GatewayDiscoveryFailureCode::UpstreamUnavailable,
        };
        ensure!(
            require_available(
                GatewayDiscoveryDegradation::new([failure.clone()])
                    .into_meta()
                    .as_ref()
            )
            .is_err()
        );
        let other = GatewayDiscoveryFailure {
            server: "media".parse()?,
            ..failure
        };
        require_available(
            GatewayDiscoveryDegradation::new([other])
                .into_meta()
                .as_ref(),
        )?;

        ensure!(same(BTreeSet::from(["hidden"]), &BTreeSet::new()).is_err());
        ensure!(same(BTreeSet::<u8>::new(), &BTreeSet::from([1])).is_err());
        receipt::require_denied::<()>(Err(rmcp::ServiceError::McpError(
            rmcp::ErrorData::invalid_request("policy", None),
        )))?;
        ensure!(receipt::require_denied::<()>(Ok(())).is_err());
        ensure!(
            receipt::require_denied::<()>(Err(rmcp::ServiceError::McpError(
                rmcp::ErrorData::resource_not_found("missing", None)
            )))
            .is_err()
        );
        Ok(())
    }
}
