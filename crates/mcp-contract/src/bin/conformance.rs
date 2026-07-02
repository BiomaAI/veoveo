//! Generic Veoveo MCP conformance CLI.
//!
//! Exercises every surface the server exposes: resources (+templates),
//! completions, SEP-1319 tasks, subscriptions, and notifications
//! (progress, tasks/status, resources/updated, resources/list_changed).

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use clap::{Parser, Subcommand};
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        ArgumentInfo, CallToolRequestParams, CallToolResult, CancelTaskParams, ClientCapabilities,
        ClientInfo, ClientRequest, CompleteRequestParams, ContentBlock, GetTaskParams,
        GetTaskPayloadParams, Implementation, NumberOrString, ProgressNotificationParam,
        ProgressToken, ReadResourceRequestParams, Reference, Request, RequestParamsMeta,
        ResourceUpdatedNotificationParam, ServerResult, SubscribeRequestParams, TaskMetadata,
        TaskStatus, TaskStatusNotificationParam,
    },
    service::NotificationContext,
    transport::StreamableHttpClientTransport,
};
use serde_json::Value;
use veoveo_mcp_contract::ProviderUris;

#[derive(Parser, Debug)]
#[command(name = "conformance", about = "Veoveo MCP conformance client")]
struct Args {
    /// MCP endpoint of the server under test.
    #[arg(long, default_value = "http://localhost:8787/media/mcp", global = true)]
    url: String,
    /// URI scheme used by the server's Veoveo resources.
    #[arg(long, default_value = "media", global = true)]
    scheme: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Show server info, capabilities, instructions, and the tool list.
    Info,
    /// Read the model catalog resource, optionally filtering locally.
    Models {
        query: Option<String>,
        /// Filter by model type (e.g. image-to-image, text-to-video).
        #[arg(long)]
        r#type: Option<String>,
    },
    /// Autocomplete model ids via completion/complete on the model template.
    Complete { prefix: String },
    /// Read the full schema resource for one model.
    Schema { model_id: String },
    /// Read the live state of a prediction resource.
    Prediction { id: String },
    /// Read a task usage report.
    Usage { task_id: String },
    /// Read and save an artifact resource.
    Artifact {
        sha256: String,
        /// Where to save the artifact file.
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
    },
    /// Run a model as an MCP task and download its outputs.
    Run {
        model_id: String,
        /// Model input as a JSON object (see `schema <model_id>`).
        #[arg(long)]
        input: String,
        /// Where to save output files.
        #[arg(long, default_value = "output")]
        output_dir: PathBuf,
        /// Cancel the task right after submission (tests tasks/cancel).
        #[arg(long)]
        cancel: bool,
    },
}

/// Client handler that surfaces every server-initiated notification.
#[derive(Clone, Default)]
struct CliHandler;

impl ClientHandler for CliHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-conformance", env!("CARGO_PKG_VERSION")),
        )
    }

    async fn on_progress(
        &self,
        params: ProgressNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!(
            "  [progress] {:.0}%{}",
            params.progress * 100.0 / params.total.unwrap_or(1.0),
            params
                .message
                .map(|m| format!(" — {m}"))
                .unwrap_or_default()
        );
    }

    async fn on_task_status(
        &self,
        params: TaskStatusNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!(
            "  [task {}] {:?}: {}",
            params.task.task_id,
            params.task.status,
            params.task.status_message.as_deref().unwrap_or("")
        );
    }

    async fn on_resource_updated(
        &self,
        params: ResourceUpdatedNotificationParam,
        _context: NotificationContext<rmcp::RoleClient>,
    ) {
        println!("  [resource updated] {}", params.uri);
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<rmcp::RoleClient>) {
        println!("  [resource list changed]");
    }
}

type Client = rmcp::service::RunningService<rmcp::RoleClient, CliHandler>;

async fn connect(url: &str) -> Result<Client> {
    let transport = StreamableHttpClientTransport::from_uri(url);
    Ok(CliHandler.serve(transport).await?)
}

async fn read_resource_json(client: &Client, uri: &str) -> Result<Value> {
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

async fn cmd_info(client: &Client) -> Result<()> {
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
        println!("  {}", tool.description.as_deref().unwrap_or(""));
        println!(
            "  input schema: {}",
            serde_json::to_string(&tool.input_schema)?
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

fn cmd_models_from_catalog(
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

async fn cmd_complete(client: &Client, uris: &ProviderUris, prefix: String) -> Result<()> {
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

async fn save_output_uri(
    client: &Client,
    uris: &ProviderUris,
    http: &reqwest::Client,
    output_dir: &std::path::Path,
    uri: &str,
) -> Result<()> {
    let (name, bytes) = if let Some(sha256) = uris.parse_artifact_uri(uri) {
        let (bytes, mime_type) = read_resource_blob(client, uri).await?;
        let ext = extension_for_mime(mime_type.as_deref());
        (format!("{sha256}.{ext}"), bytes)
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

async fn cmd_run(
    client: &Client,
    uris: &ProviderUris,
    model_id: String,
    input: String,
    output_dir: PathBuf,
    cancel: bool,
) -> Result<()> {
    let input: Value = serde_json::from_str(&input)?;

    // tools/call augmented with SEP-1319 task metadata + a progress token.
    let mut params = CallToolRequestParams::new("run")
        .with_arguments(
            serde_json::json!({ "model": model_id, "input": input })
                .as_object()
                .cloned()
                .unwrap(),
        )
        .with_task(TaskMetadata::new().with_ttl(3_600_000));
    params.set_progress_token(ProgressToken(NumberOrString::String(Arc::from("run"))));

    let created = client
        .send_request(ClientRequest::CallToolRequest(Request::new(params)))
        .await?;
    let ServerResult::CreateTaskResult(created) = created else {
        return Err(anyhow!("expected CreateTaskResult, got {created:?}"));
    };
    let task_id = created.task.task_id.clone();
    println!(
        "task {task_id} created (status {:?}, poll {}ms)",
        created.task.status,
        created.task.poll_interval.unwrap_or(3000)
    );

    if cancel {
        let result = client
            .send_request(ClientRequest::CancelTaskRequest(Request::new(
                CancelTaskParams::new(task_id.clone()),
            )))
            .await?;
        println!("cancel result: {result:?}");
        return Ok(());
    }

    // Poll tasks/get, honoring the server's suggested interval. Subscribe to
    // the prediction resource as soon as the statusMessage names it.
    let poll_ms = created.task.poll_interval.unwrap_or(3000);
    let mut subscribed = false;
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
        let message = info.task.status_message.clone().unwrap_or_default();
        println!("poll: {:?} — {message}", info.task.status);

        let prediction_prefix = format!("{}://prediction/", uris.scheme());
        if !subscribed && let Some(idx) = message.find(&prediction_prefix) {
            let uri: String = message[idx..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches(|c: char| c == ';' || c == ',')
                .to_string();
            client
                .subscribe(SubscribeRequestParams::new(uri.clone()))
                .await?;
            println!("subscribed to {uri}");
            subscribed = true;
        }

        match info.task.status {
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                break info.task;
            }
            _ => {}
        }
    };

    if final_task.status != TaskStatus::Completed {
        // tasks/result surfaces the failure detail as a JSON-RPC error.
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
    let result: CallToolResult = match payload {
        ServerResult::CallToolResult(r) => r,
        ServerResult::CustomResult(c) => serde_json::from_value(c.0)?,
        other => return Err(anyhow!("unexpected tasks/result payload: {other:?}")),
    };
    let outputs = print_call_tool_result(&result);

    if !outputs.is_empty() {
        std::fs::create_dir_all(&output_dir)?;
        let http = reqwest::Client::new();
        for uri in outputs {
            save_output_uri(client, uris, &http, &output_dir, &uri).await?;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args = Args::parse();
    let client = connect(&args.url).await?;
    let uris = ProviderUris::new(args.scheme);

    let result = match args.cmd {
        Cmd::Info => cmd_info(&client).await,
        Cmd::Models { query, r#type } => {
            let catalog = read_resource_json(&client, &uris.models_uri()).await?;
            cmd_models_from_catalog(catalog, query, r#type)
        }
        Cmd::Complete { prefix } => cmd_complete(&client, &uris, prefix).await,
        Cmd::Schema { model_id } => {
            let value = read_resource_json(&client, &uris.model_uri(&model_id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Prediction { id } => {
            let value = read_resource_json(&client, &uris.prediction_uri(&id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Usage { task_id } => {
            let value = read_resource_json(&client, &uris.usage_task_uri(&task_id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Artifact { sha256, output_dir } => {
            std::fs::create_dir_all(&output_dir)?;
            let uri = uris.artifact_uri(&sha256);
            let http = reqwest::Client::new();
            save_output_uri(&client, &uris, &http, &output_dir, &uri).await
        }
        Cmd::Run {
            model_id,
            input,
            output_dir,
            cancel,
        } => cmd_run(&client, &uris, model_id, input, output_dir, cancel).await,
    };

    client.cancel().await?;
    result
}
