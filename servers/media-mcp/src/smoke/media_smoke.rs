//! Generic Veoveo MCP conformance CLI.
//!
//! Exercises every surface the server exposes: authorization discovery, resources (+templates),
//! completions, final-extension tasks, subscriptions, and notifications
//! (progress, tasks/status, resources/updated, resources/list_changed).
use anyhow::Context;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use veoveo_media_mcp::contract::MediaGenerationResult;

use anyhow::Result;
use anyhow::anyhow;
use axum::Json as AxumJson;
use axum::Router as AxumRouter;
use axum::extract::Path as AxumPath;
use axum::extract::Query as AxumQuery;
use axum::extract::State as AxumState;
use axum::response::IntoResponse as AxumIntoResponse;
use axum::routing::get as axum_get;
use axum::routing::post as axum_post;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

use chrono::Utc;
use clap::Parser;

use rmcp::model::ArgumentInfo;
use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResponse;
use rmcp::model::CallToolResult;
use rmcp::model::CancelTaskParams;
use rmcp::model::CompleteRequestParams;
use rmcp::model::ContentBlock;
use rmcp::model::GetTaskParams;
use rmcp::model::JsonObject;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::Reference;
use rmcp::model::SubscriptionFilter;
use rmcp::model::TaskPayload;
use rmcp::model::TaskStatus;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

use veoveo_mcp_contract::GatewayProfileId;

use veoveo_mcp_contract::ServerResourceUris;
use veoveo_mcp_contract::ServerSlug;

use veoveo_mcp_contract::TokenSubject;

use veoveo_types::ScopeName;
use veoveo_types::TenantId;
use veoveo_types::WorkContextId;

#[path = "utility/cli.rs"]
mod cli;
#[path = "utility/client.rs"]
mod client;
#[path = "utility/fake_services.rs"]
mod fake_services;
#[path = "utility/mcp_commands.rs"]
mod mcp_commands;
use cli::Args;
use cli::Cmd;
use client::Client;
use client::TaskCapability;
use client::bearer_token_from_args;
use client::connect;
use fake_services::*;
use mcp_commands::*;
#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let endpoint = args.url.clone();
    let bearer = bearer_token_from_args(&args)?;
    let result = veoveo_testing_support::lifecycle::owner::run(execute(args)).await;
    if let Err(error) = &result {
        veoveo_mcp_conformance::client::failure::record(&endpoint, bearer.as_deref(), error)?;
    }
    result
}
async fn execute(args: Args) -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    match &args.cmd {
        Cmd::FakeMediaProvider {
            port,
            ready_file,
            completion_delay_ms,
            webhook_secret,
        } => {
            return cmd_fake_media_provider(
                *port,
                ready_file.clone(),
                *completion_delay_ms,
                webhook_secret.clone(),
            )
            .await;
        }
        _ => {}
    }
    let cleanup_connection = veoveo_mcp_conformance::client::ConnectionOptions {
        url: args.url.clone(),
        bearer_token: bearer_token_from_args(&args)?,
    };
    let client = connect(&args, TaskCapability::Enabled).await?;
    let uris = ServerResourceUris::new(veoveo_types::ResourceScheme::parse(args.scheme.clone())?);
    let result = match args.cmd {
        Cmd::Models { query, r#type } => cmd_models(&client, query, r#type).await,
        Cmd::Complete { prefix } => cmd_complete(&client, prefix).await,
        Cmd::Schema { model_id } => {
            let value: veoveo_media_mcp::contract::ModelResourceOutput = serde_json::from_value(
                read_resource_json(&client, &uris.model_uri(&model_id)).await?,
            )?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Prediction { id } => {
            let value: veoveo_media_mcp::contract::GenerationPredictionSummary =
                serde_json::from_value(
                    read_resource_json(&client, &uris.prediction_uri(&id)).await?,
                )?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Usage { task_id } => {
            let value = read_resource_json(&client, &uris.usage_task_uri(&task_id)).await?;
            println!("{}", serde_json::to_string_pretty(&value)?);
            Ok(())
        }
        Cmd::Run {
            model_id,
            tool_name,
            input,
            output_dir,
            cancel,
        } => {
            cmd_run(
                &client,
                &uris,
                RunCommand {
                    tool_name,
                    model_id,
                    input,
                    output_dir,
                    cancel,
                    cleanup_connection,
                },
            )
            .await
        }
        _ => unreachable!("handled before client connection"),
    };
    client.cancel().await?;
    result
}
