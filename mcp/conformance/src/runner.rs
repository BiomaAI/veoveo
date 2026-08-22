use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use reqwest::header::HOST;
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, Tool},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};

use crate::{
    CheckResult, CheckStatus, ConformanceCredentials, ConformanceReport, ConformanceReportSchema,
    HostedServerConformanceProfile, ObservedImplementation, SurfaceExpectation,
    validate_tool_input_schema,
};

#[derive(Clone, Default)]
struct CertificationClient;

impl ClientHandler for CertificationClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-mcp-conformance", env!("CARGO_PKG_VERSION")),
        )
    }
}

/// Execute every check applicable to one typed hosted-server profile.
pub async fn run_hosted_server_conformance(
    profile: &HostedServerConformanceProfile,
    credentials: &ConformanceCredentials,
) -> Result<ConformanceReport> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    profile.validate()?;
    let bearer_token = credentials
        .bearer_token()
        .ok_or_else(|| anyhow!("hosted certification requires an out-of-band bearer token"))?;
    let started_at = chrono::Utc::now();
    let mut checks = Vec::new();
    let http = reqwest::Client::new();

    if profile.http.require_authentication_rejection {
        let outcome = http.get(&profile.endpoint).send().await;
        checks.push(match outcome {
            Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => passed(
                "VV-MCP-HTTP-001",
                "unauthenticated MCP requests are rejected",
                Some(json!({"status": response.status().as_u16()})),
            ),
            Ok(response) => failed(
                "VV-MCP-HTTP-001",
                format!(
                    "unauthenticated MCP request returned {}, expected 401",
                    response.status()
                ),
            ),
            Err(error) => failed(
                "VV-MCP-HTTP-001",
                format!("unauthenticated MCP request failed: {error}"),
            ),
        });
    } else {
        checks.push(skipped(
            "VV-MCP-HTTP-001",
            "authentication rejection is not selected by this profile",
        ));
    }

    if let Some(host) = &profile.http.rejected_host {
        let outcome = http.get(&profile.endpoint).header(HOST, host).send().await;
        checks.push(match outcome {
            Ok(response) if response.status() == reqwest::StatusCode::MISDIRECTED_REQUEST => {
                passed(
                    "VV-MCP-HTTP-002",
                    "untrusted Host authority is rejected",
                    Some(json!({"status": response.status().as_u16(), "host": host})),
                )
            }
            Ok(response) => failed(
                "VV-MCP-HTTP-002",
                format!(
                    "untrusted Host authority returned {}, expected 421",
                    response.status()
                ),
            ),
            Err(error) => failed(
                "VV-MCP-HTTP-002",
                format!("Host rejection request failed: {error}"),
            ),
        });
    } else {
        checks.push(skipped(
            "VV-MCP-HTTP-002",
            "Host rejection is not selected by this profile",
        ));
    }

    check_success_url(
        &http,
        "VV-MCP-HTTP-003",
        "health endpoint",
        profile.http.health_url.as_deref(),
        &mut checks,
    )
    .await;
    check_success_url(
        &http,
        "VV-MCP-HTTP-004",
        "readiness endpoint",
        profile.http.readiness_url.as_deref(),
        &mut checks,
    )
    .await;

    let mut transport = StreamableHttpClientTransportConfig::with_uri(profile.endpoint.clone());
    transport = transport.auth_header(bearer_token.to_owned());
    let client = CertificationClient
        .serve_with_lifecycle(
            StreamableHttpClientTransport::from_config(transport),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .context("discovering the MCP server")?;
    let info = client
        .peer_info()
        .ok_or_else(|| anyhow!("MCP Discover returned no server information"))?
        .clone();
    checks.push(passed(
        "VV-MCP-TRANSPORT-001",
        "stateless Streamable HTTP Discover succeeded",
        Some(json!({"endpoint": profile.endpoint})),
    ));

    let server_info = info
        .server_info
        .as_ref()
        .ok_or_else(|| anyhow!("MCP Discover omitted serverInfo"))?;
    let implementation = ObservedImplementation {
        name: server_info.name.clone(),
        version: server_info.version.clone(),
        protocol_version: info.protocol_version.to_string(),
    };
    checks.push(if implementation.name == profile.server_slug {
        passed(
            "VV-MCP-IDENTITY-001",
            "server implementation identity matches the declared slug",
            Some(json!({"name": implementation.name})),
        )
    } else {
        failed(
            "VV-MCP-IDENTITY-001",
            format!(
                "server identity is {:?}, expected {:?}",
                implementation.name, profile.server_slug
            ),
        )
    });

    let capabilities = serde_json::to_value(&info.capabilities)?;
    let tools_advertised = capability_present(&capabilities, "tools");
    let resources_advertised = capability_present(&capabilities, "resources");
    let prompts_advertised = capability_present(&capabilities, "prompts");
    let completions_advertised = capability_present(&capabilities, "completions");
    let tasks_advertised = info.capabilities.supports_tasks();
    let subscriptions_advertised = info
        .capabilities
        .tools
        .as_ref()
        .is_some_and(|capability| capability.list_changed == Some(true))
        || info
            .capabilities
            .resources
            .as_ref()
            .is_some_and(|capability| {
                capability.list_changed == Some(true) || capability.subscribe == Some(true)
            })
        || info
            .capabilities
            .prompts
            .as_ref()
            .is_some_and(|capability| capability.list_changed == Some(true))
        || tasks_advertised;

    let tools = if should_query(profile.surfaces.tools, tools_advertised) {
        match client.list_tools(Default::default()).await {
            Ok(result) => Some(result.tools),
            Err(error) => {
                checks.push(failed(
                    "VV-MCP-TOOLS-001",
                    format!("tools/list failed: {error}"),
                ));
                None
            }
        }
    } else {
        None
    };
    check_tools(profile, tools_advertised, tools.as_deref(), &mut checks);
    let tool_names = tools
        .as_ref()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| tool.name.to_string())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let resources = if should_query(profile.surfaces.resources, resources_advertised) {
        match client.list_resources(Default::default()).await {
            Ok(result) => Some(result.resources),
            Err(error) => {
                checks.push(failed(
                    "VV-MCP-RESOURCES-001",
                    format!("resources/list failed: {error}"),
                ));
                None
            }
        }
    } else {
        None
    };
    let resource_uris = resources
        .as_ref()
        .map(|resources| {
            resources
                .iter()
                .map(|resource| resource.uri.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    check_named_surface(
        "VV-MCP-RESOURCES-001",
        "resources",
        profile.surfaces.resources,
        resources_advertised,
        &profile.surfaces.required_resources,
        &resource_uris,
        &mut checks,
    );

    let templates = if resources_advertised {
        match client.list_resource_templates(Default::default()).await {
            Ok(result) => Some(result.resource_templates),
            Err(error) => {
                checks.push(failed(
                    "VV-MCP-TEMPLATES-001",
                    format!("resources/templates/list failed: {error}"),
                ));
                None
            }
        }
    } else {
        None
    };
    let template_uris = templates
        .as_ref()
        .map(|templates| {
            templates
                .iter()
                .map(|template| template.uri_template.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let templates_present = !template_uris.is_empty();
    check_named_surface(
        "VV-MCP-TEMPLATES-001",
        "resource templates",
        profile.surfaces.resource_templates,
        templates_present,
        &profile.surfaces.required_resource_templates,
        &template_uris,
        &mut checks,
    );

    let prompts = if should_query(profile.surfaces.prompts, prompts_advertised) {
        match client.list_prompts(Default::default()).await {
            Ok(result) => Some(result.prompts),
            Err(error) => {
                checks.push(failed(
                    "VV-MCP-PROMPTS-001",
                    format!("prompts/list failed: {error}"),
                ));
                None
            }
        }
    } else {
        None
    };
    let prompt_names = prompts
        .as_ref()
        .map(|prompts| {
            prompts
                .iter()
                .map(|prompt| prompt.name.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    check_named_surface(
        "VV-MCP-PROMPTS-001",
        "prompts",
        profile.surfaces.prompts,
        prompts_advertised,
        &profile.surfaces.required_prompts,
        &prompt_names,
        &mut checks,
    );

    check_capability(
        "VV-MCP-COMPLETIONS-001",
        "completions",
        profile.surfaces.completions,
        completions_advertised,
        &mut checks,
    );
    check_capability(
        "VV-MCP-TASKS-001",
        "tasks",
        profile.surfaces.tasks,
        tasks_advertised,
        &mut checks,
    );
    check_capability(
        "VV-MCP-SUBSCRIPTIONS-001",
        "subscriptions",
        profile.surfaces.subscriptions,
        subscriptions_advertised,
        &mut checks,
    );

    let owned_uris = resource_uris
        .iter()
        .chain(template_uris.iter())
        .filter(|uri| {
            uri.split_once("://")
                .is_some_and(|(scheme, _)| profile.owned_resource_schemes.contains(scheme))
        })
        .count();
    let foreign_uris = resource_uris
        .iter()
        .chain(template_uris.iter())
        .filter(|uri| {
            uri.split_once("://")
                .is_none_or(|(scheme, _)| !profile.owned_resource_schemes.contains(scheme))
        })
        .cloned()
        .collect::<Vec<_>>();
    checks.push(if foreign_uris.is_empty() {
        passed(
            "VV-MCP-URI-001",
            "listed resources remain inside declared URI ownership",
            Some(json!({
                "ownedSchemes": profile.owned_resource_schemes,
                "uriCount": owned_uris
            })),
        )
    } else {
        failed(
            "VV-MCP-URI-001",
            format!("listed resources use undeclared URI schemes: {foreign_uris:?}"),
        )
    });

    let observed_surface = ObservedSurface {
        tool_names,
        resource_uris,
        template_uris,
        prompt_names,
        tasks_advertised,
    };
    check_well_known_surface(
        &client,
        profile,
        &implementation,
        &observed_surface,
        &mut checks,
    )
    .await;
    check_admin_docs(
        &http,
        &profile.http.docs_llms_url,
        bearer_token,
        &mut checks,
    )
    .await;

    client.cancel().await?;
    Ok(ConformanceReport {
        schema_version: ConformanceReportSchema::V1,
        profile_id: profile.profile_id.clone(),
        contract_revision: profile.contract_revision.clone(),
        started_at,
        completed_at: chrono::Utc::now(),
        implementation: Some(implementation),
        observed_capabilities: Some(capabilities),
        checks,
    })
}

type Client = rmcp::service::RunningService<rmcp::RoleClient, CertificationClient>;

#[derive(Default)]
struct ObservedSurface {
    tool_names: BTreeSet<String>,
    resource_uris: BTreeSet<String>,
    template_uris: BTreeSet<String>,
    prompt_names: BTreeSet<String>,
    tasks_advertised: bool,
}

/// One entry of the document index served at `{scheme}://docs` (C18).
#[derive(serde::Deserialize)]
struct DocIndexEntry {
    id: String,
}

async fn read_text_resource(client: &Client, uri: &str) -> Result<String> {
    let result = client
        .read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
        .await?;
    result
        .contents
        .iter()
        .find_map(|contents| match contents {
            rmcp::model::ResourceContents::TextResourceContents { text, .. } => Some(text.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("resource {uri} returned no text contents"))
}

/// Reads the Well-Known Surface over the live session (contract C18, C19):
/// at least one owned scheme must serve a docs index listing `agents` and
/// `design`, every listed body must read, and `{scheme}://contract` must
/// deserialize to the current contract revision with C18-C21 met.
async fn check_well_known_surface(
    client: &Client,
    profile: &HostedServerConformanceProfile,
    implementation: &ObservedImplementation,
    observed: &ObservedSurface,
    checks: &mut Vec<CheckResult>,
) {
    let mut index_errors = Vec::new();
    let mut serving_scheme = None;
    let mut listed_ids = Vec::new();
    for scheme in &profile.owned_resource_schemes {
        let uri = format!("{scheme}://docs");
        match read_text_resource(client, &uri).await {
            Ok(text) => match serde_json::from_str::<Vec<DocIndexEntry>>(&text) {
                Ok(entries) => {
                    let ids: Vec<String> = entries.into_iter().map(|entry| entry.id).collect();
                    if ["agents", "design"]
                        .iter()
                        .all(|id| ids.iter().any(|listed| listed == id))
                    {
                        serving_scheme = Some(scheme.clone());
                        listed_ids = ids;
                        break;
                    }
                    index_errors.push(format!("{uri}: index lists {ids:?} without agents+design"));
                }
                Err(error) => {
                    index_errors.push(format!("{uri}: index is not a JSON list: {error}"))
                }
            },
            Err(error) => index_errors.push(format!("{uri}: {error}")),
        }
    }

    let Some(scheme) = serving_scheme else {
        checks.push(failed(
            "VV-MCP-DOCS-001",
            format!(
                "no owned scheme serves a docs index with agents+design: {}",
                index_errors.join("; ")
            ),
        ));
        for id in [
            "VV-MCP-DOCS-002",
            "VV-MCP-CONTRACT-001",
            "VV-MCP-CONTRACT-002",
        ] {
            checks.push(failed(id, "docs index unavailable".to_owned()));
        }
        return;
    };
    checks.push(passed(
        "VV-MCP-DOCS-001",
        "docs index lists the required documents",
        Some(json!({"scheme": scheme, "documents": listed_ids})),
    ));

    let mut body_errors = Vec::new();
    for id in &listed_ids {
        let uri = format!("{scheme}://docs/{id}");
        match read_text_resource(client, &uri).await {
            Ok(body) if !body.trim().is_empty() => {}
            Ok(_) => body_errors.push(format!("{uri}: body is empty")),
            Err(error) => body_errors.push(format!("{uri}: {error}")),
        }
    }
    checks.push(if body_errors.is_empty() {
        passed(
            "VV-MCP-DOCS-002",
            "every listed document body reads",
            Some(json!({"documents": listed_ids.len()})),
        )
    } else {
        failed("VV-MCP-DOCS-002", body_errors.join("; "))
    });

    let contract_uri = format!("{scheme}://contract");
    match read_text_resource(client, &contract_uri).await {
        Ok(text) => {
            match serde_json::from_str::<veoveo_mcp_contract::docs::ContractDeclaration>(&text) {
                Ok(declaration) => {
                    let revision_matches = declaration.contract_revision
                        == veoveo_mcp_contract::docs::CONTRACT_REVISION;
                    let identity_matches = declaration.server == profile.server_slug
                        && declaration.server == implementation.name;
                    let unmet: Vec<&str> = ["C18", "C19", "C20", "C21"]
                        .into_iter()
                        .filter(|id| {
                            !declaration.compliance.iter().any(|item| {
                                item.id == *id
                                    && item.status
                                        == veoveo_mcp_contract::docs::ComplianceStatus::Met
                            })
                        })
                        .collect();
                    checks.push(if revision_matches && identity_matches && unmet.is_empty() {
                        passed(
                            "VV-MCP-CONTRACT-001",
                            "contract declaration matches the selected revision and server identity with C18-C21 met",
                            Some(json!({
                                "server": declaration.server,
                                "contractRevision": declaration.contract_revision,
                                "selectedRevision": profile.contract_revision
                            })),
                        )
                    } else {
                        failed(
                            "VV-MCP-CONTRACT-001",
                            format!(
                                "declaration server {:?}, observed server {:?}, expected profile server {:?}; revision {} (expected {} for {}), unmet well-known items {unmet:?}",
                                declaration.server,
                                implementation.name,
                                profile.server_slug,
                                declaration.contract_revision,
                                veoveo_mcp_contract::docs::CONTRACT_REVISION,
                                profile.contract_revision,
                            ),
                        )
                    });
                    checks.push(passed(
                        "VV-MCP-CONTRACT-002",
                        "Discover and list methods provide the observed running surface",
                        Some(json!({
                            "tools": observed.tool_names,
                            "resources": observed.resource_uris,
                            "resourceTemplates": observed.template_uris,
                            "prompts": observed.prompt_names,
                            "tasksAdvertised": observed.tasks_advertised,
                        })),
                    ));
                }
                Err(error) => {
                    let summary = format!("{contract_uri} is not a contract declaration: {error}");
                    checks.push(failed("VV-MCP-CONTRACT-001", summary.clone()));
                    checks.push(failed("VV-MCP-CONTRACT-002", summary));
                }
            }
        }
        Err(error) => {
            let summary = format!("{contract_uri}: {error}");
            checks.push(failed("VV-MCP-CONTRACT-001", summary.clone()));
            checks.push(failed("VV-MCP-CONTRACT-002", summary));
        }
    }
}

/// Fetches the admin llms.txt projection and every listed document body
/// (contract C20).
async fn check_admin_docs(
    http: &reqwest::Client,
    url: &str,
    bearer_token: &str,
    checks: &mut Vec<CheckResult>,
) {
    let index_url = match url::Url::parse(url) {
        Ok(url) => url,
        Err(error) => {
            let summary = format!("validated llms.txt URL did not parse: {error}");
            checks.push(failed("VV-MCP-DOCS-HTTP-001", summary.clone()));
            checks.push(failed("VV-MCP-DOCS-HTTP-002", summary));
            return;
        }
    };
    checks.push(match http.get(url).send().await {
        Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => passed(
            "VV-MCP-DOCS-HTTP-000",
            "admin docs reject requests without the gateway internal identity",
            Some(json!({"status": response.status().as_u16()})),
        ),
        Ok(response) => failed(
            "VV-MCP-DOCS-HTTP-000",
            format!(
                "unauthenticated llms.txt returned {}, expected 401",
                response.status()
            ),
        ),
        Err(error) => failed(
            "VV-MCP-DOCS-HTTP-000",
            format!("unauthenticated llms.txt request failed: {error}"),
        ),
    });
    let body = match http.get(url).bearer_auth(bearer_token).send().await {
        Ok(response) if response.status().is_success() => match response.text().await {
            Ok(body) => body,
            Err(error) => {
                checks.push(failed(
                    "VV-MCP-DOCS-HTTP-001",
                    format!("llms.txt body failed to read: {error}"),
                ));
                checks.push(failed(
                    "VV-MCP-DOCS-HTTP-002",
                    "llms.txt unavailable".to_owned(),
                ));
                return;
            }
        },
        Ok(response) => {
            checks.push(failed(
                "VV-MCP-DOCS-HTTP-001",
                format!("llms.txt returned {}", response.status()),
            ));
            checks.push(failed(
                "VV-MCP-DOCS-HTTP-002",
                "llms.txt unavailable".to_owned(),
            ));
            return;
        }
        Err(error) => {
            checks.push(failed(
                "VV-MCP-DOCS-HTTP-001",
                format!("llms.txt request failed: {error}"),
            ));
            checks.push(failed(
                "VV-MCP-DOCS-HTTP-002",
                "llms.txt unavailable".to_owned(),
            ));
            return;
        }
    };

    let listed: Vec<&str> = body
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once("](")?;
            rest.split_once(')').map(|(target, _)| target)
        })
        .collect();
    checks.push(
        if ["agents", "design"].iter().all(|id| listed.contains(id)) {
            passed(
                "VV-MCP-DOCS-HTTP-001",
                "llms.txt lists the required documents",
                Some(json!({"url": url, "documents": listed})),
            )
        } else {
            failed(
                "VV-MCP-DOCS-HTTP-001",
                format!("llms.txt lists {listed:?} without agents+design"),
            )
        },
    );

    let mut body_errors = Vec::new();
    for target in &listed {
        if target.is_empty()
            || target.contains('/')
            || target.contains(['?', '#'])
            || *target == "."
            || *target == ".."
        {
            body_errors.push(format!(
                "{target:?}: document link must be one relative path segment"
            ));
            continue;
        }
        let doc_url = match index_url.join(target) {
            Ok(url) if same_origin(&index_url, &url) => url,
            Ok(url) => {
                body_errors.push(format!(
                    "{url}: document link leaves the authenticated origin"
                ));
                continue;
            }
            Err(error) => {
                body_errors.push(format!("{target:?}: invalid document link: {error}"));
                continue;
            }
        };
        match http.get(doc_url.clone()).send().await {
            Ok(response) if response.status() == reqwest::StatusCode::UNAUTHORIZED => {}
            Ok(response) => body_errors.push(format!(
                "{doc_url}: unauthenticated document returned {}, expected 401",
                response.status()
            )),
            Err(error) => body_errors.push(format!(
                "{doc_url}: unauthenticated document request failed: {error}"
            )),
        }
        match http
            .get(doc_url.clone())
            .bearer_auth(bearer_token)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(text) if !text.trim().is_empty() => {}
                Ok(_) => body_errors.push(format!("{doc_url}: body is empty")),
                Err(error) => body_errors.push(format!("{doc_url}: {error}")),
            },
            Ok(response) => body_errors.push(format!("{doc_url}: {}", response.status())),
            Err(error) => body_errors.push(format!("{doc_url}: {error}")),
        }
    }
    checks.push(if body_errors.is_empty() {
        passed(
            "VV-MCP-DOCS-HTTP-002",
            "every listed document rejects unauthenticated access and serves over the authenticated admin mount",
            Some(json!({"documents": listed.len()})),
        )
    } else {
        failed("VV-MCP-DOCS-HTTP-002", body_errors.join("; "))
    });
}

fn same_origin(left: &url::Url, right: &url::Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

async fn check_success_url(
    http: &reqwest::Client,
    requirement_id: &'static str,
    name: &'static str,
    url: Option<&str>,
    checks: &mut Vec<CheckResult>,
) {
    let Some(url) = url else {
        checks.push(skipped(
            requirement_id,
            format!("{name} is not selected by this profile"),
        ));
        return;
    };
    let outcome = http.get(url).send().await;
    checks.push(match outcome {
        Ok(response) if response.status().is_success() => passed(
            requirement_id,
            format!("{name} returned success"),
            Some(json!({"url": url, "status": response.status().as_u16()})),
        ),
        Ok(response) => failed(
            requirement_id,
            format!("{name} returned {}", response.status()),
        ),
        Err(error) => failed(requirement_id, format!("{name} request failed: {error}")),
    });
}

fn check_tools(
    profile: &HostedServerConformanceProfile,
    advertised: bool,
    tools: Option<&[Tool]>,
    checks: &mut Vec<CheckResult>,
) {
    check_capability(
        "VV-MCP-TOOLS-001",
        "tools",
        profile.surfaces.tools,
        advertised,
        checks,
    );
    let Some(tools) = tools else {
        return;
    };
    let names = tools
        .iter()
        .map(|tool| tool.name.to_string())
        .collect::<BTreeSet<_>>();
    let missing = profile
        .surfaces
        .required_tools
        .difference(&names)
        .cloned()
        .collect::<Vec<_>>();
    checks.push(if missing.is_empty() {
        passed(
            "VV-MCP-TOOLS-002",
            "required tools are listed",
            Some(json!({"tools": names})),
        )
    } else {
        failed(
            "VV-MCP-TOOLS-002",
            format!("required tools are missing: {missing:?}"),
        )
    });
    let invalid = tools
        .iter()
        .filter_map(|tool| {
            validate_tool_input_schema(tool)
                .err()
                .map(|error| error.to_string())
        })
        .collect::<Vec<_>>();
    checks.push(if invalid.is_empty() {
        passed(
            "VV-MCP-TOOLS-003",
            "tool input schemas are bounded JSON Schema 2020-12 object roots",
            Some(json!({"validated": tools.len()})),
        )
    } else {
        failed(
            "VV-MCP-TOOLS-003",
            format!("invalid tool schemas: {invalid:?}"),
        )
    });
}

#[allow(clippy::too_many_arguments)]
fn check_named_surface(
    requirement_id: &'static str,
    name: &'static str,
    expectation: SurfaceExpectation,
    advertised: bool,
    required: &BTreeSet<String>,
    observed: &BTreeSet<String>,
    checks: &mut Vec<CheckResult>,
) {
    check_capability(requirement_id, name, expectation, advertised, checks);
    if should_query(expectation, advertised) {
        let missing = required.difference(observed).cloned().collect::<Vec<_>>();
        checks.push(if missing.is_empty() {
            passed(
                format!("{requirement_id}-NAMES"),
                format!("required {name} are listed"),
                Some(json!({"observed": observed})),
            )
        } else {
            failed(
                format!("{requirement_id}-NAMES"),
                format!("required {name} are missing: {missing:?}"),
            )
        });
    }
}

fn check_capability(
    requirement_id: impl Into<String>,
    name: &str,
    expectation: SurfaceExpectation,
    advertised: bool,
    checks: &mut Vec<CheckResult>,
) {
    let requirement_id = requirement_id.into();
    let result = match (expectation, advertised) {
        (SurfaceExpectation::Required, true) => {
            passed(&requirement_id, format!("{name} are advertised"), None)
        }
        (SurfaceExpectation::Required, false) => {
            failed(&requirement_id, format!("{name} are required but absent"))
        }
        (SurfaceExpectation::Forbidden, true) => failed(
            &requirement_id,
            format!("{name} are forbidden but advertised"),
        ),
        (SurfaceExpectation::Forbidden, false) => {
            passed(&requirement_id, format!("{name} are not advertised"), None)
        }
        (SurfaceExpectation::Optional, true) => passed(
            &requirement_id,
            format!("optional {name} are advertised"),
            None,
        ),
        (SurfaceExpectation::Optional, false) => skipped(
            &requirement_id,
            format!("optional {name} are not advertised"),
        ),
    };
    checks.push(result);
}

fn should_query(expectation: SurfaceExpectation, advertised: bool) -> bool {
    advertised && expectation != SurfaceExpectation::Forbidden
}

fn capability_present(capabilities: &Value, name: &str) -> bool {
    capabilities
        .as_object()
        .and_then(|object| object.get(name))
        .is_some_and(|value| !value.is_null())
}

fn passed(
    requirement_id: impl Into<String>,
    summary: impl Into<String>,
    evidence: Option<Value>,
) -> CheckResult {
    CheckResult {
        requirement_id: requirement_id.into(),
        status: CheckStatus::Passed,
        summary: summary.into(),
        evidence,
    }
}

fn failed(requirement_id: impl Into<String>, summary: impl Into<String>) -> CheckResult {
    CheckResult {
        requirement_id: requirement_id.into(),
        status: CheckStatus::Failed,
        summary: summary.into(),
        evidence: None,
    }
}

fn skipped(requirement_id: impl Into<String>, summary: impl Into<String>) -> CheckResult {
    CheckResult {
        requirement_id: requirement_id.into(),
        status: CheckStatus::Skipped,
        summary: summary.into(),
        evidence: None,
    }
}
