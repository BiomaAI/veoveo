use super::*;
use anyhow::bail;
use veoveo_media_mcp::contract::MediaModelUri;
pub(super) async fn read_resource_json(client: &Client, uri: &str) -> Result<Value> {
    let (text, _) = read_resource_text(client, uri).await?;
    Ok(serde_json::from_str(&text)?)
}

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

pub(super) async fn cmd_models(
    client: &Client,
    query: Option<String>,
    ty: Option<String>,
) -> Result<()> {
    use veoveo_media_mcp::contract::{MediaModelIndexUri, ModelCatalogOutput};
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    let needle = query.map(|q| q.to_lowercase());
    let mut shown = 0;
    let mut total = None;
    let mut received = 0;
    for _ in 0..100 {
        veoveo_testing_support::lifecycle::owner::check_effect()?;
        let uri = MediaModelIndexUri::new(None, None, Some(&100), cursor.as_ref());
        let page: ModelCatalogOutput =
            serde_json::from_value(read_resource_json(client, uri.as_str()).await?)?;
        if let Some(total) = total {
            anyhow::ensure!(
                total == page.total_available,
                "Media catalog changed during traversal"
            );
        }
        total = Some(page.total_available);
        for model in &page.models {
            anyhow::ensure!(
                seen.insert(model.model_id.clone()),
                "Media catalog repeated a model during traversal"
            );
            received += 1;
            if ty.as_ref().is_some_and(|t| *t != model.model_type)
                || needle.as_ref().is_some_and(|n| {
                    !model.model_id.as_str().to_lowercase().contains(n)
                        && !model.description.to_lowercase().contains(n)
                })
            {
                continue;
            }
            let price = model
                .base_price
                .map(|p| format!("${p}"))
                .unwrap_or_default();
            println!("{}  [{}] {price}", model.model_id, model.model_type);
            println!(
                "    {}",
                model.description.chars().take(110).collect::<String>()
            );
            shown += 1;
        }
        cursor = page.next_cursor.clone();
        if cursor.is_none() {
            anyhow::ensure!(
                received == page.total_available,
                "Media catalog traversal ended before all models were read"
            );
            println!("\n{shown} / {received} models");
            return Ok(());
        }
    }
    Err(anyhow!("Media model catalog exceeded 100 pages"))
}

pub(super) async fn cmd_complete(client: &Client, prefix: String) -> Result<()> {
    let result = client
        .complete(CompleteRequestParams::new(
            Reference::for_resource(MediaModelUri::TEMPLATE),
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

use veoveo_artifact_service::smoke_output::save_output_uri;

pub(super) struct RunCommand {
    pub(super) tool_name: String,
    pub(super) model_id: String,
    pub(super) input: String,
    pub(super) output_dir: PathBuf,
    pub(super) cancel: bool,
    pub(super) cleanup_connection: veoveo_mcp_conformance::client::ConnectionOptions,
}

pub(super) async fn cmd_run(
    client: &Client,
    uris: &ServerResourceUris,
    command: RunCommand,
) -> Result<()> {
    let RunCommand {
        tool_name,
        model_id,
        input,
        output_dir,
        cancel,
        cleanup_connection,
    } = command;
    let input: Value = serde_json::from_str(&input)?;
    let arguments = serde_json::json!({ "model": model_id, "input": input })
        .as_object()
        .cloned()
        .expect("run arguments are an object");
    use veoveo_testing_support::lifecycle::owner::{self, CleanupKind};
    let task_identity = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    let cleanup_identity = std::sync::Arc::clone(&task_identity);
    let intent = uuid::Uuid::now_v7();
    let cleanup = owner::register_cleanup(
        CleanupKind::Remote,
        "Media Task",
        &intent.to_string(),
        move || async move {
            let id = cleanup_identity
                .lock()
                .expect("owned Task identity")
                .clone()
                .context(
                    "Task dispatch outcome unresolved; reconcile the retained operation intent",
                )?;
            let connection = veoveo_mcp_conformance::client::connect(
                &cleanup_connection,
                TaskCapability::Enabled,
            )
            .await?;
            let current = connection
                .get_task(GetTaskParams::new(id.clone()))
                .await?
                .task;
            anyhow::ensure!(
                current.task.task_id == id,
                "cleanup Task observation has another identity"
            );
            if matches!(
                current.status(),
                TaskStatus::Working | TaskStatus::InputRequired
            ) {
                connection
                    .cancel_task(CancelTaskParams::new(id.clone()))
                    .await?;
            }
            loop {
                let task = connection
                    .get_task(GetTaskParams::new(id.clone()))
                    .await?
                    .task;
                anyhow::ensure!(
                    task.task.task_id == id,
                    "cleanup Task observation has another identity"
                );
                if matches!(
                    task.status(),
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
                ) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            connection.cancel().await?;
            Ok(())
        },
    )?;
    owner::check_effect()?;
    let created = start_task(client, tool_name, arguments).await?;
    let task_id = created.task.task_id.clone();
    *task_identity.lock().expect("owned Task identity") = Some(task_id.clone());
    println!(
        "task {task_id} created (status {:?}, poll {}ms)",
        created.task.status,
        created.task.poll_interval_ms.unwrap_or(3000)
    );

    if cancel {
        let ready = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let task = client
                    .get_task(GetTaskParams::new(task_id.clone()))
                    .await?
                    .task;
                let message = task.task.status_message.clone().unwrap_or_default();
                if message.contains("prediction ") {
                    return Ok(message);
                }
                match task.status() {
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
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
        client
            .cancel_task(CancelTaskParams::new(task_id.clone()))
            .await?;
        let cancelled = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let task = client
                    .get_task(GetTaskParams::new(task_id.clone()))
                    .await?
                    .task;
                match task.status() {
                    TaskStatus::Cancelled => return Ok(task),
                    TaskStatus::Completed | TaskStatus::Failed => {
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
        if cancelled.status() != TaskStatus::Cancelled || cancelled.task.task_id != task_id {
            return Err(anyhow!(
                "tasks/get after cancellation returned {cancelled:?}"
            ));
        }
        cleanup.settled()?;
        println!("cancelled task {task_id} (status Cancelled)");
        return Ok(());
    }

    // Poll final tasks/get, honoring the server's suggested interval. Subscribe
    // to the prediction resource as soon as the status message names it.
    let poll_ms = created.task.poll_interval_ms.unwrap_or(3000);
    let mut subscription = None;
    let final_task = loop {
        owner::check_effect()?;
        wait_with_resource_updates(&mut subscription, Duration::from_millis(poll_ms)).await?;
        let task = client
            .get_task(GetTaskParams::new(task_id.clone()))
            .await?
            .task;
        let message = task.task.status_message.clone().unwrap_or_default();
        println!("poll: {:?} — {message}", task.status());

        let prediction_prefix = format!("{}://prediction/", uris.scheme());
        if subscription.is_none()
            && let Some(idx) = message.find(&prediction_prefix)
        {
            let uri: String = message[idx..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_end_matches([';', ','])
                .to_string();
            let filter = SubscriptionFilter::builder()
                .resource_subscription(uri.clone())
                .build();
            subscription = Some(client.listen(filter).await?);
            println!("subscribed to {uri}");
        }

        match task.status() {
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => {
                break task;
            }
            _ => {}
        }
    };

    anyhow::ensure!(
        final_task.task.task_id == task_id,
        "terminal Task observation has another identity"
    );
    cleanup.settled()?;
    let result: CallToolResult = match final_task.payload {
        TaskPayload::Completed { result } => {
            serde_json::from_value(Value::Object(result.into_iter().collect()))?
        }
        TaskPayload::Failed { error } => {
            if let Some(mut subscription) = subscription {
                subscription.cancel().await?;
            }
            return Err(anyhow!(
                "task failed: {}",
                Value::Object(error.into_iter().collect())
            ));
        }
        TaskPayload::Cancelled => return Err(anyhow!("task was cancelled")),
        other => return Err(anyhow!("unexpected non-terminal task state: {other:?}")),
    };
    ensure_call_tool_succeeded(&result)?;
    let links = print_call_tool_result(&result);
    let outputs = if uris.scheme() == &veoveo_types::ResourceScheme::parse("media")? {
        let value = result
            .structured_content
            .clone()
            .ok_or_else(|| anyhow!("Media completion omitted its structured generation result"))?;
        let generation: MediaGenerationResult = serde_json::from_value(value)
            .context("Media completion does not satisfy the current result contract")?;
        // Gateway Task handles stay opaque. The domain result carries its native identity.
        let canonical: MediaGenerationResult = serde_json::from_value(
            read_resource_json(client, generation.result_uri().as_str()).await?,
        )?;
        anyhow::ensure!(
            generation == canonical,
            "Media completion disagrees with its canonical generation resource"
        );
        anyhow::ensure!(
            links == [generation.result_uri().to_string()],
            "Media completion must link exactly its canonical generation result"
        );
        generation
            .artifacts()
            .iter()
            .map(|artifact| artifact.artifact_uri.to_string())
            .collect::<Vec<_>>()
    } else {
        links
    };

    if !outputs.is_empty() {
        std::fs::create_dir_all(&output_dir)?;
        let http = reqwest::Client::new();
        for uri in outputs {
            save_output_uri(client, uris, &http, &output_dir, &uri).await?;
        }
    }
    if let Some(mut subscription) = subscription {
        subscription.cancel().await?;
        println!("subscription cancelled");
    }
    Ok(())
}

async fn wait_with_resource_updates(
    subscription: &mut Option<rmcp::service::Subscription>,
    delay: Duration,
) -> Result<()> {
    let timer = tokio::time::sleep(delay);
    tokio::pin!(timer);
    let Some(subscription) = subscription.as_mut() else {
        timer.await;
        return Ok(());
    };
    loop {
        tokio::select! {
            () = &mut timer => return Ok(()),
            notification = subscription.next() => {
                match notification? {
                    Some(rmcp::model::ServerNotification::ResourceUpdatedNotification(update)) => {
                        eprintln!("  [resource updated] {}", update.params.uri);
                    }
                    Some(_) => bail!("prediction subscription received an unexpected notification"),
                    None => bail!("prediction subscription ended before Task completion: {:?}", subscription.end()),
                }
            }
        }
    }
}
