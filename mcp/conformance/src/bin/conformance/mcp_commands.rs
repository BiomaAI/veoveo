use anyhow::bail;
use rmcp::model::Tool;
use veoveo_mcp_conformance::catalog;

use super::client::Client;
use super::*;

async fn read_resource_text(client: &Client, uri: &str) -> Result<(String, Option<String>)> {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    result
        .contents
        .iter()
        .find_map(|c| match c {
            rmcp::model::ResourceContents::TextResourceContents {
                text, mime_type, ..
            } => Some((text.clone(), mime_type.clone())),
            _ => None,
        })
        .ok_or_else(|| anyhow!("resource {uri} returned no text contents"))
}

pub(super) async fn cmd_info(client: &Client) -> Result<()> {
    let info = client
        .peer_info()
        .ok_or_else(|| anyhow!("no server info"))?;
    let server = info
        .server_info
        .as_ref()
        .ok_or_else(|| anyhow!("Discover omitted serverInfo"))?;
    println!("server: {} v{}", server.name, server.version);
    println!("protocol: {}", info.protocol_version);
    println!(
        "capabilities: {}",
        serde_json::to_string_pretty(&info.capabilities)?
    );
    if let Some(instructions) = &info.instructions {
        println!("instructions:\n{instructions}");
    }
    cmd_tools(client).await?;
    let prompts = catalog::prompts(client).await?;
    for prompt in prompts {
        println!(
            "prompt `{}` — {}",
            prompt.name,
            prompt.description.unwrap_or_default()
        );
    }
    let templates = catalog::templates(client).await?;
    for t in templates {
        println!(
            "template: {} — {}",
            t.uri_template,
            t.description.unwrap_or_default()
        );
    }
    Ok(())
}

pub(super) async fn cmd_tools(client: &Client) -> Result<()> {
    let tools = catalog::tools(client).await?;
    validate_tool_schemas(&tools)?;
    println!("schema compatibility: {} tool(s) valid", tools.len());
    for tool in tools {
        println!("\ntool `{}`", tool.name);
        if let Some(annotations) = &tool.annotations {
            println!("  annotations: {}", serde_json::to_string(annotations)?);
        }
        println!("  {}", tool.description.as_deref().unwrap_or(""));
        println!(
            "  input schema: {}",
            serde_json::to_string(&tool.input_schema)?
        );
        if let Some(schema) = &tool.output_schema {
            println!("  output schema: {}", serde_json::to_string(schema)?);
        }
    }
    Ok(())
}

fn validate_tool_schemas(tools: &[Tool]) -> Result<()> {
    for tool in tools {
        veoveo_mcp_conformance::validate_tool_input_schema(tool)?;
    }
    Ok(())
}

pub(super) async fn cmd_prompts(client: &Client) -> Result<()> {
    let prompts = client.list_prompts(Default::default()).await?;
    for prompt in prompts.prompts {
        println!(
            "{} — {}",
            prompt.name,
            prompt.description.unwrap_or_default()
        );
        for argument in prompt.arguments.unwrap_or_default() {
            println!(
                "    {}{} — {}",
                argument.name,
                if argument.required == Some(true) {
                    " *"
                } else {
                    ""
                },
                argument.description.unwrap_or_default()
            );
        }
    }
    if let Some(cursor) = prompts.next_cursor {
        println!("\nnext cursor: {cursor}");
    }
    Ok(())
}

pub(super) async fn cmd_resources(client: &Client) -> Result<()> {
    let resources = client.list_resources(Default::default()).await?;
    for resource in resources.resources {
        println!(
            "{} — {}",
            resource.uri,
            resource.description.unwrap_or_default()
        );
    }
    if let Some(cursor) = resources.next_cursor {
        println!("\nnext cursor: {cursor}");
    }
    Ok(())
}

const MAX_APP_HTML_BYTES: usize = 2 * 1024 * 1024;

pub(super) async fn cmd_apps_check(client: &Client) -> Result<()> {
    let info = client
        .peer_info()
        .ok_or_else(|| anyhow!("no server info"))?;
    if !veoveo_mcp_apps_extension::server_declares_ui(&info.capabilities) {
        bail!(
            "server does not declare the `{}` extension",
            veoveo_mcp_apps_extension::EXTENSION_ID
        );
    }
    let resources = catalog::resources(client).await?;
    let app_uris: Vec<String> = resources
        .iter()
        .filter(|resource| veoveo_mcp_apps_extension::is_app_resource(resource))
        .map(|resource| resource.uri.clone())
        .collect();
    if app_uris.is_empty() {
        bail!(
            "server declares the apps extension but lists no `{}` resources",
            veoveo_mcp_apps_extension::APP_MIME_TYPE
        );
    }
    let tools = catalog::tools(client).await?;
    let mut linked_tools = 0usize;
    for tool in &tools {
        if let Some(link) = veoveo_mcp_apps_extension::tool_app_link(tool) {
            if !app_uris.contains(&link.resource_uri) {
                bail!(
                    "tool `{}` links app `{}` which is not a listed app resource",
                    tool.name,
                    link.resource_uri
                );
            }
            linked_tools += 1;
        }
    }
    for uri in &app_uris {
        let result = client
            .read_resource(ReadResourceRequestParams::new(uri.as_str()))
            .await?;
        let contents = result
            .contents
            .iter()
            .find_map(|content| match content {
                rmcp::model::ResourceContents::TextResourceContents {
                    text, mime_type, ..
                } => Some((text.clone(), mime_type.clone())),
                _ => None,
            })
            .ok_or_else(|| anyhow!("app resource {uri} returned no text contents"))?;
        let (html, mime_type) = contents;
        if mime_type.as_deref() != Some(veoveo_mcp_apps_extension::APP_MIME_TYPE) {
            bail!(
                "app resource {uri} has mime `{}` (expected `{}`)",
                mime_type.unwrap_or_default(),
                veoveo_mcp_apps_extension::APP_MIME_TYPE
            );
        }
        if html.len() > MAX_APP_HTML_BYTES {
            bail!(
                "app resource {uri} is {} bytes (cap {MAX_APP_HTML_BYTES})",
                html.len()
            );
        }
        assert_self_contained_html(uri, &html)?;
    }
    println!(
        "apps-check ok: {} app resource(s), {} app-linked tool(s)",
        app_uris.len(),
        linked_tools
    );
    Ok(())
}

/// Rejects fetch-capable references to external origins. Namespace
/// identifiers such as `xmlns="http://…"` are not fetches and stay allowed.
fn assert_self_contained_html(uri: &str, html: &str) -> Result<()> {
    let lowered = html.to_ascii_lowercase();
    for needle in [
        "src=\"http://",
        "src=\"https://",
        "src='http://",
        "src='https://",
        "href=\"http://",
        "href=\"https://",
        "href='http://",
        "href='https://",
        "url(http://",
        "url(https://",
        "url(\"http",
        "url('http",
        "@import",
    ] {
        if lowered.contains(needle) {
            bail!("app resource {uri} references an external origin via `{needle}`");
        }
    }
    Ok(())
}

pub(super) async fn cmd_resource(client: &Client, uri: String) -> Result<()> {
    let (text, mime_type) = read_resource_text(client, &uri).await?;
    println!("{}", format_text_resource(&text, mime_type.as_deref())?);
    Ok(())
}

fn format_text_resource(text: &str, mime_type: Option<&str>) -> Result<String> {
    let json = mime_type.is_none_or(|mime_type| {
        let essence = mime_type
            .split_once(';')
            .map_or(mime_type, |(essence, _)| essence)
            .trim()
            .to_ascii_lowercase();
        essence == "application/json" || essence.ends_with("+json")
    });
    if json {
        return Ok(serde_json::to_string_pretty(
            &serde_json::from_str::<Value>(text)?,
        )?);
    }
    Ok(text.to_owned())
}

pub(super) async fn cmd_prompt(
    client: &Client,
    name: String,
    arguments: Option<String>,
) -> Result<()> {
    let arguments = arguments
        .map(|raw| serde_json::from_str::<Value>(&raw))
        .transpose()?
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| anyhow!("prompt arguments must be a JSON object"))
        })
        .transpose()?;
    let mut params = GetPromptRequestParams::new(name);
    if let Some(arguments) = arguments {
        params = params.with_arguments(arguments);
    }
    let result = client.get_prompt(params).await?;
    if let Some(description) = result.description {
        println!("{description}");
    }
    for message in result.messages {
        match message.content {
            ContentBlock::Text(text) => println!("\n{:?}:\n{}", message.role, text.text),
            other => println!("\n{:?}:\n{other:?}", message.role),
        }
    }
    Ok(())
}

pub(super) async fn cmd_call(
    client: &Client,
    tool_name: String,
    arguments: String,
    task: bool,
) -> Result<()> {
    let arguments = serde_json::from_str::<Value>(&arguments)?
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    if !task {
        let result = client
            .call_tool(CallToolRequestParams::new(tool_name).with_arguments(arguments))
            .await?;
        print_call_tool_result(&result);
        return Ok(());
    }

    let created = start_task(client, tool_name, arguments).await?;
    let result = await_task(client, created, Duration::from_secs(3_600)).await?;
    print_call_tool_result(&result);
    Ok(())
}

pub(super) async fn cmd_task_call(
    client: &Client,
    tool_name: String,
    arguments: String,
    timeout: Duration,
) -> Result<()> {
    let arguments = serde_json::from_str::<Value>(&arguments)?
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let created = start_task(client, tool_name, arguments).await?;
    let result = await_task(client, created, timeout).await?;
    ensure_call_tool_succeeded(&result)?;
    print_call_tool_result(&result);
    Ok(())
}

async fn start_task(
    client: &Client,
    tool_name: String,
    arguments: JsonObject,
) -> Result<rmcp::model::CreateTaskResult> {
    match client
        .call_tool_once(CallToolRequestParams::new(tool_name).with_arguments(arguments))
        .await?
    {
        CallToolResponse::Task(created) => Ok(created),
        CallToolResponse::Complete(result) => {
            ensure_call_tool_succeeded(&result)?;
            Err(anyhow!("tool completed without creating a task"))
        }
        CallToolResponse::InputRequired(_) => Err(anyhow!(
            "tool requires direct multi-round input before task creation"
        )),
        _ => Err(anyhow!("unsupported tools/call response")),
    }
}

async fn await_task(
    client: &Client,
    created: rmcp::model::CreateTaskResult,
    timeout: Duration,
) -> Result<CallToolResult> {
    let task_id = created.task.task_id.clone();
    let poll_ms = created
        .task
        .poll_interval_ms
        .unwrap_or(100)
        .clamp(10, 5_000);
    println!(
        "task {task_id} created (status {:?}, poll {poll_ms}ms)",
        created.task.status
    );
    let terminal = tokio::time::timeout(timeout, async {
        loop {
            let current = client.get_task(GetTaskParams::new(task_id.clone())).await?;
            println!(
                "poll: {:?} — {}",
                current.task.status(),
                current
                    .task
                    .task
                    .status_message
                    .as_deref()
                    .unwrap_or_default()
            );
            match current.task.payload {
                TaskPayload::Working => tokio::time::sleep(Duration::from_millis(poll_ms)).await,
                TaskPayload::InputRequired { .. } => {
                    return Err(anyhow!("task {task_id} requires additional input"));
                }
                terminal => return Ok(terminal),
            }
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting {timeout:?} for task {task_id}"))??;
    match terminal {
        TaskPayload::Completed { result } => Ok(serde_json::from_value(Value::Object(
            result.into_iter().collect(),
        ))?),
        TaskPayload::Failed { error } => Err(anyhow!(
            "task failed: {}",
            Value::Object(error.into_iter().collect())
        )),
        TaskPayload::Cancelled => Err(anyhow!("task was cancelled")),
        TaskPayload::Working | TaskPayload::InputRequired { .. } => unreachable!(),
        _ => Err(anyhow!("unsupported terminal task payload")),
    }
}

pub(super) async fn cmd_complete_resource(
    client: &Client,
    uri: String,
    argument: String,
    prefix: String,
) -> Result<()> {
    let result = client
        .complete(CompleteRequestParams::new(
            Reference::for_resource(uri),
            ArgumentInfo::new(argument, prefix),
        ))
        .await?;
    for value in &result.completion.values {
        println!("{value}");
    }
    println!(
        "\n{} shown, total {:?}, has_more {:?}",
        result.completion.values.len(),
        result.completion.total,
        result.completion.has_more
    );
    Ok(())
}

fn print_call_tool_result(result: &CallToolResult) -> Vec<String> {
    let mut outputs = Vec::new();
    for block in result.content.iter() {
        match block {
            ContentBlock::Text(t) => println!("{}", t.text),
            ContentBlock::ResourceLink(link) => {
                println!(
                    "output: {} ({})",
                    link.uri,
                    link.mime_type.as_deref().unwrap_or("unknown")
                );
                outputs.push(link.uri.clone());
            }
            other => println!("{other:?}"),
        }
    }
    if let Some(structured) = &result.structured_content {
        println!("structured: {structured}");
    }
    outputs
}

fn ensure_call_tool_succeeded(result: &CallToolResult) -> Result<()> {
    if result.is_error != Some(true) {
        return Ok(());
    }
    let detail = result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    Err(anyhow!(if detail.is_empty() {
        "tool task completed with an error result".to_owned()
    } else {
        detail
    }))
}

/// Request-scoped subscriptions deliver through their SDK handle, independently
/// of the ordinary ClientHandler notification callbacks.

#[cfg(test)]
mod schema_validation_tests {
    use super::*;

    #[test]
    fn generic_resource_output_preserves_typed_html() {
        let html = "<!doctype html><p>live stream</p>";
        assert_eq!(
            format_text_resource(html, Some("text/html;profile=mcp-app")).unwrap(),
            html
        );
    }

    #[test]
    fn generic_resource_output_pretty_prints_json_media_types() {
        assert_eq!(
            format_text_resource(
                "{\"frames\":1}",
                Some("application/vnd.veoveo.stream-results+json"),
            )
            .unwrap(),
            "{\n  \"frames\": 1\n}"
        );
    }
}
