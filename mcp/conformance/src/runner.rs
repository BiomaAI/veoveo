use std::collections::BTreeSet;

use anyhow::{Context, Result, anyhow};
use reqwest::header::HOST;
use rmcp::{
    ClientHandler, ServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, Tool},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};

use crate::{
    CheckResult, CheckStatus, ConformanceCredentials, ConformanceReport, ConformanceReportSchema,
    HostedServerConformanceProfile, ObservedImplementation, SurfaceExpectation,
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
    profile.validate()?;
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
    if let Some(token) = credentials.bearer_token() {
        transport = transport.auth_header(token.to_owned());
    }
    let client = CertificationClient
        .serve(StreamableHttpClientTransport::from_config(transport))
        .await
        .context("initializing the MCP conformance session")?;
    let info = client
        .peer_info()
        .ok_or_else(|| anyhow!("MCP initialization returned no server information"))?
        .clone();
    checks.push(passed(
        "VV-MCP-TRANSPORT-001",
        "sessionful Streamable HTTP initialization succeeded",
        Some(json!({"endpoint": profile.endpoint})),
    ));

    let implementation = ObservedImplementation {
        name: info.server_info.name.clone(),
        version: info.server_info.version.clone(),
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
    let tasks_advertised =
        contains_enabled_key(&capabilities, veoveo_mcp_task_extension::EXTENSION_ID)
            || contains_enabled_key(&capabilities, "tasks");
    let subscriptions_advertised = contains_enabled_key(&capabilities, "subscribe");

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

    check_well_known_surface(&client, profile, &mut checks).await;
    check_admin_docs(&http, profile.http.docs_llms_url.as_deref(), &mut checks).await;

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
    checks: &mut Vec<CheckResult>,
) {
    if profile.surfaces.resources == SurfaceExpectation::Forbidden {
        for id in ["VV-MCP-DOCS-001", "VV-MCP-DOCS-002", "VV-MCP-CONTRACT-001"] {
            checks.push(skipped(id, "resources are forbidden for this profile"));
        }
        return;
    }

    let mut index_errors = Vec::new();
    let mut serving_scheme = None;
    let mut listed_ids = Vec::new();
    for scheme in &profile.owned_resource_schemes {
        let uri = format!("{scheme}://docs");
        match read_text_resource(client, &uri).await {
            Ok(text) => match serde_json::from_str::<Vec<DocIndexEntry>>(&text) {
                Ok(entries) => {
                    let ids: Vec<String> = entries.into_iter().map(|entry| entry.id).collect();
                    if ["agents", "design"].iter().all(|id| ids.iter().any(|listed| listed == id)) {
                        serving_scheme = Some(scheme.clone());
                        listed_ids = ids;
                        break;
                    }
                    index_errors.push(format!("{uri}: index lists {ids:?} without agents+design"));
                }
                Err(error) => index_errors.push(format!("{uri}: index is not a JSON list: {error}")),
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
        for id in ["VV-MCP-DOCS-002", "VV-MCP-CONTRACT-001"] {
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
    checks.push(match read_text_resource(client, &contract_uri).await {
        Ok(text) => {
            match serde_json::from_str::<veoveo_mcp_contract::docs::ContractDeclaration>(&text) {
                Ok(declaration) => {
                    let revision_matches = declaration.contract_revision
                        == veoveo_mcp_contract::docs::CONTRACT_REVISION;
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
                    if revision_matches && unmet.is_empty() {
                        passed(
                            "VV-MCP-CONTRACT-001",
                            "contract declaration matches the current revision with C18-C21 met",
                            Some(json!({
                                "server": declaration.server,
                                "contractRevision": declaration.contract_revision
                            })),
                        )
                    } else {
                        failed(
                            "VV-MCP-CONTRACT-001",
                            format!(
                                "declaration revision {} (expected {}), unmet well-known items {unmet:?}",
                                declaration.contract_revision,
                                veoveo_mcp_contract::docs::CONTRACT_REVISION
                            ),
                        )
                    }
                }
                Err(error) => failed(
                    "VV-MCP-CONTRACT-001",
                    format!("{contract_uri} is not a contract declaration: {error}"),
                ),
            }
        }
        Err(error) => failed("VV-MCP-CONTRACT-001", format!("{contract_uri}: {error}")),
    });
}

/// Fetches the admin llms.txt projection and every listed document body
/// (contract C20).
async fn check_admin_docs(
    http: &reqwest::Client,
    url: Option<&str>,
    checks: &mut Vec<CheckResult>,
) {
    let Some(url) = url else {
        for id in ["VV-MCP-DOCS-HTTP-001", "VV-MCP-DOCS-HTTP-002"] {
            checks.push(skipped(id, "admin docs URL is not selected by this profile"));
        }
        return;
    };
    let body = match http.get(url).send().await {
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
            let (_, rest) = line.split_once("](docs/")?;
            rest.split_once(')').map(|(id, _)| id)
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

    let base = url.trim_end_matches("llms.txt");
    let mut body_errors = Vec::new();
    for id in &listed {
        let doc_url = format!("{base}{id}");
        match http.get(&doc_url).send().await {
            Ok(response) if response.status().is_success() => {
                match response.text().await {
                    Ok(text) if !text.trim().is_empty() => {}
                    Ok(_) => body_errors.push(format!("{doc_url}: body is empty")),
                    Err(error) => body_errors.push(format!("{doc_url}: {error}")),
                }
            }
            Ok(response) => body_errors.push(format!("{doc_url}: {}", response.status())),
            Err(error) => body_errors.push(format!("{doc_url}: {error}")),
        }
    }
    checks.push(if body_errors.is_empty() {
        passed(
            "VV-MCP-DOCS-HTTP-002",
            "every listed document body serves over the admin mount",
            Some(json!({"documents": listed.len()})),
        )
    } else {
        failed("VV-MCP-DOCS-HTTP-002", body_errors.join("; "))
    });
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
            validate_tool_schema(tool)
                .err()
                .map(|error| error.to_string())
        })
        .collect::<Vec<_>>();
    checks.push(if invalid.is_empty() {
        passed(
            "VV-MCP-TOOLS-003",
            "tool input schemas are self-contained JSON Schema object roots",
            Some(json!({"validated": tools.len()})),
        )
    } else {
        failed(
            "VV-MCP-TOOLS-003",
            format!("invalid tool schemas: {invalid:?}"),
        )
    });
}

fn validate_tool_schema(tool: &Tool) -> Result<()> {
    let schema = Value::Object(tool.input_schema.as_ref().clone());
    jsonschema::meta::validate(&schema)
        .map_err(|error| anyhow!("tool `{}`: {error}", tool.name))?;
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(anyhow!("tool `{}` schema root is not an object", tool.name));
    }
    if contains_key(&schema, "$ref") {
        return Err(anyhow!("tool `{}` schema contains $ref", tool.name));
    }
    Ok(())
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

fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => {
            object.contains_key(key) || object.values().any(|value| contains_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| contains_key(value, key)),
        _ => false,
    }
}

fn contains_enabled_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => {
            object
                .get(key)
                .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
                || object
                    .values()
                    .any(|value| contains_enabled_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| contains_enabled_key(value, key)),
        _ => false,
    }
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
