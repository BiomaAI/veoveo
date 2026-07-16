use anyhow::bail;

use super::client::{Client, FinalTaskClient};
use super::*;
use veoveo_mcp_task_extension::{
    DetailedTask, RequestMeta as FinalRequestMeta, TaskStatus as FinalTaskStatus, ToolCallParams,
};

pub(super) async fn read_resource_json(client: &Client, uri: &str) -> Result<Value> {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let text = result
        .contents
        .iter()
        .find_map(|c| match c {
            rmcp::model::ResourceContents::TextResourceContents { text, .. } => Some(text.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("resource {uri} returned no text contents"))?;
    Ok(serde_json::from_str(&text)?)
}

async fn read_resource_blob(client: &Client, uri: &str) -> Result<(Vec<u8>, Option<String>)> {
    let result = client
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let (blob, mime_type) = result
        .contents
        .iter()
        .find_map(|c| match c {
            rmcp::model::ResourceContents::BlobResourceContents {
                blob, mime_type, ..
            } => Some((blob.clone(), mime_type.clone())),
            _ => None,
        })
        .ok_or_else(|| anyhow!("resource {uri} returned no blob contents"))?;
    Ok((BASE64_STANDARD.decode(blob)?, mime_type))
}

pub(super) async fn cmd_info(client: &Client) -> Result<()> {
    let info = client
        .peer_info()
        .ok_or_else(|| anyhow!("no server info"))?;
    println!(
        "server: {} v{}",
        info.server_info.name, info.server_info.version
    );
    println!("protocol: {}", info.protocol_version);
    println!(
        "capabilities: {}",
        serde_json::to_string_pretty(&info.capabilities)?
    );
    if let Some(instructions) = &info.instructions {
        println!("instructions:\n{instructions}");
    }
    let tools = client.list_tools(Default::default()).await?;
    for tool in tools.tools {
        println!(
            "\ntool `{}` (task support: {:?})",
            tool.name,
            tool.execution.as_ref().map(|e| &e.task_support)
        );
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
    let prompts = client.list_prompts(Default::default()).await?;
    for prompt in prompts.prompts {
        println!(
            "prompt `{}` — {}",
            prompt.name,
            prompt.description.unwrap_or_default()
        );
    }
    let templates = client.list_resource_templates(Default::default()).await?;
    for t in templates.resource_templates {
        println!(
            "template: {} — {}",
            t.uri_template,
            t.description.unwrap_or_default()
        );
    }
    Ok(())
}

pub(super) fn cmd_models_from_catalog(
    catalog: Value,
    query: Option<String>,
    ty: Option<String>,
) -> Result<()> {
    let models = catalog.as_array().ok_or_else(|| anyhow!("bad catalog"))?;
    let needle = query.map(|q| q.to_lowercase());
    let mut shown = 0usize;
    for m in models {
        let id = m["model_id"].as_str().unwrap_or_default();
        let mtype = m["type"].as_str().unwrap_or_default();
        let desc = m["description"].as_str().unwrap_or_default();
        if let Some(t) = &ty
            && mtype != t
        {
            continue;
        }
        if let Some(n) = &needle
            && !id.to_lowercase().contains(n)
            && !desc.to_lowercase().contains(n)
        {
            continue;
        }
        let price = m["base_price"]
            .as_f64()
            .map(|p| format!("${p}"))
            .unwrap_or_default();
        println!("{id}  [{mtype}] {price}");
        let short: String = desc.chars().take(110).collect();
        println!("    {short}");
        shown += 1;
    }
    println!("\n{shown} / {} models", models.len());
    Ok(())
}

pub(super) async fn cmd_complete(
    client: &Client,
    uris: &ServerResourceUris,
    prefix: String,
) -> Result<()> {
    let result = client
        .complete(CompleteRequestParams::new(
            Reference::for_resource(uris.model_template()),
            ArgumentInfo::new("model_id", prefix),
        ))
        .await?;
    for v in &result.completion.values {
        println!("{v}");
    }
    println!(
        "\n{} shown, total {:?}, has_more {:?}",
        result.completion.values.len(),
        result.completion.total,
        result.completion.has_more
    );
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
    let resources = client.list_resources(Default::default()).await?;
    let app_uris: Vec<String> = resources
        .resources
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
    let tools = client.list_tools(Default::default()).await?;
    let mut linked_tools = 0usize;
    for tool in &tools.tools {
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
    let value = read_resource_json(client, &uri).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
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

    let params = CallToolRequestParams::new(tool_name)
        .with_arguments(arguments)
        .with_task(TaskMetadata::new().with_ttl(3_600_000));
    let created = client
        .send_request(ClientRequest::CallToolRequest(Request::new(params)))
        .await?;
    let ServerResult::CreateTaskResult(created) = created else {
        return Err(anyhow!("expected CreateTaskResult, got {created:?}"));
    };
    let task_id = created.task.task_id.clone();
    println!("task {task_id} created (status {:?})", created.task.status);

    let poll_ms = created.task.poll_interval.unwrap_or(3000);
    let final_task = loop {
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        let info = client
            .send_request(ClientRequest::GetTaskRequest(Request::new(
                GetTaskParams::new(task_id.clone()),
            )))
            .await?;
        let ServerResult::GetTaskResult(info) = info else {
            return Err(anyhow!("expected GetTaskResult, got {info:?}"));
        };
        println!(
            "poll: {:?} — {}",
            info.task.status,
            info.task.status_message.as_deref().unwrap_or_default()
        );
        match info.task.status {
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                break info.task;
            }
            _ => {}
        }
    };
    if final_task.status != TaskStatus::Completed {
        let err = client
            .send_request(ClientRequest::GetTaskPayloadRequest(Request::new(
                GetTaskPayloadParams::new(task_id.clone()),
            )))
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown error".into());
        return Err(anyhow!("task ended {:?}: {err}", final_task.status));
    }

    let payload = client
        .send_request(ClientRequest::GetTaskPayloadRequest(Request::new(
            GetTaskPayloadParams::new(task_id.clone()),
        )))
        .await?;
    let ServerResult::CallToolResult(result) = payload else {
        return Err(anyhow!("expected CallToolResult payload, got {payload:?}"));
    };
    print_call_tool_result(&result);
    Ok(())
}

pub(super) async fn cmd_task_call(
    final_tasks: &FinalTaskClient,
    tool_name: String,
    arguments: String,
) -> Result<()> {
    let arguments = serde_json::from_str::<Value>(&arguments)?
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow!("tool arguments must be a JSON object"))?;
    let created = final_tasks
        .start_tool(ToolCallParams {
            meta: FinalRequestMeta::new().with_task_capability(),
            name: tool_name,
            arguments: arguments.into_iter().collect(),
        })
        .await?;
    let task_id = created.task.task_id;
    let poll_ms = created
        .task
        .poll_interval_ms
        .unwrap_or(100)
        .clamp(10, 5_000);
    println!(
        "task {task_id} created (status {:?}, poll {poll_ms}ms)",
        created.task.status
    );

    let final_task = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let task = final_tasks.get(task_id).await?;
            let message = task.metadata().status_message.as_deref().unwrap_or("");
            println!("poll: {:?} - {message}", task.status());
            match task {
                DetailedTask::Working { .. } => {
                    tokio::time::sleep(Duration::from_millis(poll_ms)).await;
                }
                DetailedTask::InputRequired { .. } => {
                    return Err(anyhow!("task {task_id} requires additional input"));
                }
                terminal => return Ok(terminal),
            }
        }
    })
    .await
    .map_err(|_| anyhow!("timed out waiting for task {task_id}"))??;

    match final_task {
        DetailedTask::Completed { result, .. } => {
            let result: CallToolResult =
                serde_json::from_value(Value::Object(result.into_iter().collect()))?;
            print_call_tool_result(&result);
            Ok(())
        }
        DetailedTask::Failed { error, .. } => {
            Err(anyhow!("task failed ({}): {}", error.code, error.message))
        }
        DetailedTask::Cancelled { .. } => Err(anyhow!("task was cancelled")),
        DetailedTask::Working { .. } | DetailedTask::InputRequired { .. } => {
            unreachable!("task wait returns only terminal states")
        }
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

pub(super) async fn cmd_tasks(client: &Client) -> Result<()> {
    let result = client
        .send_request(ClientRequest::ListTasksRequest(ListTasksRequest::default()))
        .await?;
    let ServerResult::ListTasksResult(result) = result else {
        return Err(anyhow!("expected ListTasksResult, got {result:?}"));
    };
    for task in &result.tasks {
        println!(
            "{} {:?} {}",
            task.task_id,
            task.status,
            task.status_message.as_deref().unwrap_or_default()
        );
    }
    println!("{} task(s)", result.tasks.len());
    if let Some(cursor) = result.next_cursor {
        println!("next cursor: {cursor}");
    }
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

fn extension_for_mime(mime_type: Option<&str>) -> &'static str {
    match mime_type.and_then(|m| m.split(';').next()) {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        Some("image/gif") => "gif",
        Some("video/mp4") => "mp4",
        Some("video/webm") => "webm",
        Some("audio/mpeg") => "mp3",
        Some("audio/wav") => "wav",
        _ => "bin",
    }
}

pub(super) async fn save_output_uri(
    client: &Client,
    uris: &ServerResourceUris,
    http: &reqwest::Client,
    output_dir: &std::path::Path,
    uri: &str,
) -> Result<()> {
    let (name, bytes) = if let Some(artifact_id) = uris.parse_artifact_uri(uri) {
        let (bytes, mime_type) = read_resource_blob(client, uri).await?;
        let ext = extension_for_mime(mime_type.as_deref());
        (format!("{artifact_id}.{ext}"), bytes)
    } else if uri.starts_with("http://") || uri.starts_with("https://") {
        let name = uri
            .split('?')
            .next()
            .and_then(|p| p.rsplit('/').next())
            .filter(|n| !n.is_empty())
            .unwrap_or("output.bin")
            .to_string();
        let bytes = http
            .get(uri)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec();
        (name, bytes)
    } else {
        return Err(anyhow!("unsupported output resource uri: {uri}"));
    };

    let path = output_dir.join(name);
    std::fs::write(&path, &bytes)?;
    println!("saved {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

pub(super) struct RunCommand {
    pub(super) tool_name: String,
    pub(super) model_id: String,
    pub(super) input: String,
    pub(super) output_dir: PathBuf,
    pub(super) cancel: bool,
}

pub(super) async fn cmd_run(
    client: &Client,
    final_tasks: &FinalTaskClient,
    uris: &ServerResourceUris,
    command: RunCommand,
) -> Result<()> {
    let RunCommand {
        tool_name,
        model_id,
        input,
        output_dir,
        cancel,
    } = command;
    let input: Value = serde_json::from_str(&input)?;
    let created = final_tasks
        .start_tool(ToolCallParams {
            meta: FinalRequestMeta::new().with_task_capability(),
            name: tool_name,
            arguments: serde_json::json!({ "model": model_id, "input": input })
                .as_object()
                .cloned()
                .unwrap()
                .into_iter()
                .collect(),
        })
        .await?;
    let task_id = created.task.task_id;
    println!(
        "task {task_id} created (status {:?}, poll {}ms)",
        created.task.status,
        created.task.poll_interval_ms.unwrap_or(3000)
    );

    if cancel {
        let ready = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let task = final_tasks.get(task_id).await?;
                let message = task.metadata().status_message.clone().unwrap_or_default();
                if message.contains("prediction ") {
                    return Ok(message);
                }
                match task.status() {
                    FinalTaskStatus::Completed
                    | FinalTaskStatus::Failed
                    | FinalTaskStatus::Cancelled => {
                        return Err(anyhow!(
                            "task reached {:?} before provider cancellation could be requested",
                            task.status()
                        ));
                    }
                    _ => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            }
        })
        .await
        .map_err(|_| anyhow!("timed out waiting for task {task_id} provider binding"))??;
        println!("cancel target ready: {ready}");
        final_tasks.cancel(task_id).await?;
        let cancelled = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let task = final_tasks.get(task_id).await?;
                match task.status() {
                    FinalTaskStatus::Cancelled => return Ok(task),
                    FinalTaskStatus::Completed | FinalTaskStatus::Failed => {
                        return Err(anyhow!(
                            "task reached {:?} while cancellation was pending",
                            task.status()
                        ));
                    }
                    _ => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            }
        })
        .await
        .map_err(|_| anyhow!("timed out waiting for task {task_id} cancellation"))??;
        if cancelled.status() != FinalTaskStatus::Cancelled
            || cancelled.metadata().task_id != task_id
        {
            return Err(anyhow!(
                "tasks/get after cancellation returned {cancelled:?}"
            ));
        }
        println!("cancelled task {task_id} (status Cancelled)");
        return Ok(());
    }

    // Poll final tasks/get, honoring the server's suggested interval. Subscribe
    // to the prediction resource as soon as the status message names it.
    let poll_ms = created.task.poll_interval_ms.unwrap_or(3000);
    let mut subscribed_uri = None::<String>;
    let final_task = loop {
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        let task = final_tasks.get(task_id).await?;
        let message = task.metadata().status_message.clone().unwrap_or_default();
        println!("poll: {:?} — {message}", task.status());

        let prediction_prefix = format!("{}://prediction/", uris.scheme());
        if subscribed_uri.is_none()
            && let Some(idx) = message.find(&prediction_prefix)
        {
            let uri: String = message[idx..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches([';', ','])
                .to_string();
            client
                .subscribe(SubscribeRequestParams::new(uri.clone()))
                .await?;
            println!("subscribed to {uri}");
            subscribed_uri = Some(uri);
        }

        match task.status() {
            FinalTaskStatus::Completed | FinalTaskStatus::Failed | FinalTaskStatus::Cancelled => {
                break task;
            }
            _ => {}
        }
    };

    let result: CallToolResult = match final_task {
        DetailedTask::Completed { result, .. } => {
            serde_json::from_value(Value::Object(result.into_iter().collect()))?
        }
        DetailedTask::Failed { error, .. } => {
            if let Some(uri) = subscribed_uri {
                client
                    .unsubscribe(UnsubscribeRequestParams::new(uri.clone()))
                    .await?;
                println!("unsubscribed from {uri}");
            }
            return Err(anyhow!("task failed ({}): {}", error.code, error.message));
        }
        DetailedTask::Cancelled { .. } => return Err(anyhow!("task was cancelled")),
        other => return Err(anyhow!("unexpected non-terminal task state: {other:?}")),
    };
    let outputs = print_call_tool_result(&result);

    if !outputs.is_empty() {
        std::fs::create_dir_all(&output_dir)?;
        let http = reqwest::Client::new();
        for uri in outputs {
            save_output_uri(client, uris, &http, &output_dir, &uri).await?;
        }
    }
    if let Some(uri) = subscribed_uri {
        client
            .unsubscribe(UnsubscribeRequestParams::new(uri.clone()))
            .await?;
        println!("unsubscribed from {uri}");
    }
    Ok(())
}
