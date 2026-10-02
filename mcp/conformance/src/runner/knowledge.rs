//! Live knowledge-source checks. Declared change and search capabilities require
//! owner-supplied qualification probes; document collections need neither operation.
mod probes;
use super::{CheckResult, Client, failed, passed, skipped};
use crate::KnowledgeSourceTarget;
use anyhow::{Context, Result, ensure};
use rmcp::{
    model::{
        ClientCapabilities, ReadResourceRequestParams, ReadResourceResult, ResourceContents,
        ResourceTemplate, ServerResult, Tool,
    },
    service::PeerRequestOptions,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_mcp_knowledge_extension::{
    self as knowledge, CollectionDescriptor, Observation, Revision,
};
use veoveo_types::{
    ResourceScheme, ResourceTemplateUri, ResourceUri, ResourceUriBuilder, ResourceUriParts,
};

const PAGE_LIMIT: usize = 100;
const ITEM_BYTES: usize = 64 * 1024;

// Read-only view of an owner-defined page. Its member DTO may expose additional
// domain fields; the common enumeration contract requires these URI links.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberPage {
    items: Vec<MemberLink>,
    next_cursor: Option<String>,
}
#[derive(Deserialize)]
struct MemberLink {
    uri: ResourceUri,
}

pub(super) async fn check(
    client: &Client,
    profile: &KnowledgeSourceTarget,
    templates: &[ResourceTemplate],
    tools: &[Tool],
    probes: &crate::knowledge_probes::KnowledgeProbes<'_>,
    checks: &mut Vec<CheckResult>,
) {
    let info = client.peer_info();
    let settings = info
        .as_ref()
        .and_then(|info| info.capabilities.extensions.as_ref())
        .and_then(|extensions| extensions.get(knowledge::EXTENSION_ID));
    let Some(settings) = settings else {
        for id in ["K01", "K02", "K03", "K04", "K05", "K06", "K07", "K08"] {
            checks.push(skipped(
                id,
                "server does not declare the knowledge-source extension",
            ));
        }
        return;
    };
    if !settings.is_empty() {
        checks.push(failed(
            "K01",
            "knowledge-source declares unsupported settings",
        ));
        return;
    }
    let descriptors = descriptors(templates, profile);
    let descriptors = match descriptors {
        Ok(values) => {
            checks.push(passed(
                "K01",
                "collection declarations are typed, unique and server-owned",
                Some(json!({"collections": values.len()})),
            ));
            values
        }
        Err(error) => {
            checks.push(failed("K01", error.to_string()));
            return;
        }
    };
    let docs: Vec<_> = descriptors
        .iter()
        .filter(|d| d.collection().name().as_str() == "docs")
        .collect();
    checks.push(
        if docs.len() == 1 && valid_docs_descriptor(docs[0], profile, templates) {
            passed(
                "K02",
                "server declares its immutable profile-accessible docs collection",
                None,
            )
        } else {
            failed("K02", "missing or invalid docs collection")
        },
    );

    for descriptor in &descriptors {
        let enumeration =
            tokio::time::timeout(Duration::from_secs(30), enumerate(client, descriptor)).await;
        let members = match enumeration {
            Ok(Ok(members)) => {
                checks.push(passed(
                    "K03",
                    format!(
                        "{} has bounded unique member pages",
                        descriptor.collection()
                    ),
                    Some(json!({"members": members.len()})),
                ));
                members
            }
            Ok(Err(error)) => {
                checks.push(failed("K03", error.to_string()));
                continue;
            }
            Err(_) => {
                checks.push(failed("K03", "enumeration exceeded 30 seconds"));
                continue;
            }
        };
        // Required docs checks in runner.rs separately require agents and design.
        if members.is_empty() {
            for id in ["K04", "K05", "K06"] {
                checks.push(failed(
                    id,
                    format!(
                        "{} needs a populated qualification fixture",
                        descriptor.collection()
                    ),
                ));
            }
            continue;
        }
        let result = tokio::time::timeout(
            Duration::from_secs(60),
            check_members(client, profile, descriptor, &members),
        )
        .await;
        match result {
            Ok(Ok(())) => {
                for (id, message) in [
                    ("K04", "members are bounded text or JSON"),
                    ("K05", "observations bind content and collection access"),
                    (
                        "K06",
                        "matching validators return empty conditional responses",
                    ),
                ] {
                    checks.push(passed(
                        id,
                        format!("{}: {message}", descriptor.collection()),
                        Some(json!({"members": members.len()})),
                    ));
                }
            }
            outcome => {
                let error = match outcome {
                    Ok(Err(error)) => error.to_string(),
                    _ => "member qualification exceeded 60 seconds".into(),
                };
                for id in ["K04", "K05", "K06"] {
                    checks.push(failed(id, error.clone()));
                }
            }
        }
    }
    probes::check(client, profile, &descriptors, tools, probes, checks).await;
}

fn descriptors(
    templates: &[ResourceTemplate],
    profile: &KnowledgeSourceTarget,
) -> Result<Vec<CollectionDescriptor>> {
    let mut declarations = Vec::new();
    let mut names = BTreeSet::new();
    for template in templates {
        if let Some(declaration) = knowledge::client::collection(template)? {
            let member_template = ResourceTemplateUri::new(template.uri_template.clone())?;
            ensure!(
                profile.owns_scheme(url::Url::parse(member_template.as_str())?.scheme()),
                "collection template uses an unowned scheme"
            );
            ensure!(
                profile.owns_scheme(url::Url::parse(declaration.enumerate().as_str())?.scheme()),
                "enumeration uses an unowned scheme"
            );
            ensure!(
                declaration.collection().server() == profile.server(),
                "collection belongs to another server"
            );
            ensure!(
                names.insert(declaration.collection().clone()),
                "duplicate collection declaration"
            );
            declarations.push(declaration);
        }
    }
    ensure!(
        !declarations.is_empty(),
        "extension declares no collections"
    );
    Ok(declarations)
}

fn valid_docs_descriptor(
    descriptor: &CollectionDescriptor,
    profile: &KnowledgeSourceTarget,
    templates: &[ResourceTemplate],
) -> bool {
    let server = profile.server();
    let Ok(enumeration) = url::Url::parse(descriptor.enumerate().as_str()) else {
        return false;
    };
    let Ok(scheme) = ResourceScheme::new(enumeration.scheme()) else {
        return false;
    };
    descriptor == &knowledge::docs::collection(server, &scheme)
        && templates.iter().any(|template| {
            template.uri_template == knowledge::docs::member_template(&scheme).as_str()
                && knowledge::client::collection(template)
                    .ok()
                    .flatten()
                    .as_ref()
                    == Some(descriptor)
        })
}

async fn read(
    client: &Client,
    uri: &ResourceUri,
    revision: Option<&Revision>,
) -> Result<ReadResourceResult> {
    let (request, options) = knowledge::client::read_request(
        ReadResourceRequestParams::new(uri.as_str()),
        ClientCapabilities::default(),
        revision,
        PeerRequestOptions::default(),
    );
    match client
        .peer()
        .send_request_with_option(request, options)
        .await?
        .await_response()
        .await?
    {
        ServerResult::ReadResourceResult(result) => Ok(result),
        _ => anyhow::bail!("knowledge read requires a complete resource response"),
    }
}

fn text(result: &ReadResourceResult) -> Result<&str> {
    let [
        ResourceContents::TextResourceContents {
            text, mime_type, ..
        },
    ] = result.contents.as_slice()
    else {
        anyhow::bail!("knowledge member requires one text item")
    };
    let mime = mime_type
        .as_deref()
        .unwrap_or("text/plain")
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    ensure!(
        (mime.starts_with("text/") && mime != "text/html" && mime != "text/event-stream")
            || mime == "application/json"
            || mime.ends_with("+json"),
        "member MIME type is not admitted by the kernel"
    );
    ensure!(
        text.len() <= ITEM_BYTES,
        "knowledge item exceeds the kernel's 64 KiB limit"
    );
    Ok(text)
}

async fn enumerate(client: &Client, descriptor: &CollectionDescriptor) -> Result<Vec<ResourceUri>> {
    let root = descriptor.enumerate().expand_scalars(&BTreeMap::new())?;
    let mut uri = root.clone();
    let mut members = Vec::new();
    let mut seen_members = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    for _ in 0..8 {
        let result = read(client, &uri, None).await?;
        let page: MemberPage = serde_json::from_str(text(&result)?)?;
        ensure!(
            page.items.len() <= PAGE_LIMIT,
            "knowledge enumeration exceeds 100 members per page"
        );
        for item in page.items {
            ensure!(
                seen_members.insert(item.uri.clone()),
                "enumeration repeated a member"
            );
            members.push(item.uri);
        }
        let Some(cursor) = page.next_cursor else {
            return Ok(members);
        };
        ensure!(
            !cursor.is_empty() && cursor.len() <= 4096 && seen_cursors.insert(cursor.clone()),
            "invalid or repeated enumeration cursor"
        );
        uri = if descriptor
            .enumerate()
            .variables()
            .any(|name| name == "cursor")
        {
            descriptor
                .enumerate()
                .expand_scalars(&BTreeMap::from([("cursor".into(), cursor)]))?
        } else {
            ResourceUriBuilder::new(root.as_str())?
                .query_pair("cursor", &cursor)?
                .build()?
        };
    }
    anyhow::bail!("knowledge qualification fixture exceeds eight enumeration pages")
}

async fn check_members(
    client: &Client,
    profile: &KnowledgeSourceTarget,
    descriptor: &CollectionDescriptor,
    members: &[ResourceUri],
) -> Result<()> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    for uri in members {
        ensure!(
            profile.owns_scheme(ResourceUriParts::parse(uri.as_str())?.scheme()),
            "enumerated member uses an unowned scheme"
        );
        let result = read(client, uri, None).await?;
        text(&result)?;
        let observation: Observation = knowledge::client::validate_read(&result, uri, None)?
            .context("declared member omitted its observation")?;
        observation.validate_collection(descriptor)?;
        let conditional = read(client, uri, Some(observation.revision())).await?;
        let unchanged =
            knowledge::client::validate_read(&conditional, uri, Some(observation.revision()))?
                .context("conditional read omitted its observation")?;
        ensure!(
            unchanged.not_modified(),
            "matching revision returned content"
        );
        unchanged.validate_collection(descriptor)?;
        ensure!(
            unchanged.content_sha256() == observation.content_sha256(),
            "unchanged revision changed its digest"
        );
        let mut meta = rmcp::model::RequestMetaObject::default();
        knowledge::client::declare_read(&mut meta, Some(observation.revision()));
        meta.insert(
            "io.modelcontextprotocol/protocolVersion".into(),
            json!(rmcp::model::ProtocolVersion::V_2026_07_28),
        );
        let denied = http.post(profile.endpoint().clone())
            .header("Accept", "application/json, text/event-stream")
            .header("Mcp-Method", "resources/read")
            .header("Mcp-Name", uri.as_str())
            .header("MCP-Protocol-Version", rmcp::model::ProtocolVersion::V_2026_07_28.as_str())
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "resources/read", "params": {"uri": uri, "_meta": meta}}))
            .send().await?;
        ensure!(
            denied.status() == reqwest::StatusCode::UNAUTHORIZED,
            "unauthenticated conditional read must return HTTP 401"
        );
    }
    Ok(())
}
