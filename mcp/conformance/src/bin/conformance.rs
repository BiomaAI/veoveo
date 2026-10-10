//! Domain-neutral MCP conformance and separate public-client OAuth login.
use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use reqwest::header::WWW_AUTHENTICATE;
use rmcp::model::ArgumentInfo;
use rmcp::model::CallToolRequestParams;
use rmcp::model::CallToolResponse;
use rmcp::model::CallToolResult;
use rmcp::model::CompleteRequestParams;
use rmcp::model::ContentBlock;
use rmcp::model::GetPromptRequestParams;
use rmcp::model::GetTaskParams;
use rmcp::model::JsonObject;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::Reference;
use rmcp::model::TaskPayload;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;
#[path = "conformance/auth_discovery.rs"]
mod auth_discovery;
#[path = "conformance/cli.rs"]
mod cli;
#[path = "conformance/client.rs"]
mod client;
#[path = "conformance/mcp_commands.rs"]
mod mcp_commands;
#[path = "conformance/oauth_login.rs"]
mod oauth_login;
#[path = "conformance/source_checks.rs"]
mod source_checks;
use cli::Args;
use cli::Cmd;
use client::TaskCapability;
use client::bearer_token_from_args;
use mcp_commands::*;
#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let args = Args::parse();
    if let Cmd::OAuthLogin(login) = &args.cmd {
        return oauth_login::run(login)
            .await
            .map_err(|_| anyhow!("OAuth login failed"));
    }
    let endpoint = args.url.clone();
    let bearer = bearer_token_from_args(&args)?;
    let result = execute(args).await;
    if let Err(error) = &result {
        veoveo_mcp_conformance::client::failure::record(&endpoint, bearer.as_deref(), error)?;
    }
    result
}
async fn execute(args: Args) -> Result<()> {
    match &args.cmd {
        Cmd::KnowledgeSource(command) => {
            return command.run(&args.url, bearer_token_from_args(&args)?).await;
        }
        Cmd::Certify { profile, report } => {
            let profile = serde_json::from_slice(&std::fs::read(profile)?)?;
            let credentials = bearer_token_from_args(&args)?
                .map(veoveo_mcp_conformance::ConformanceCredentials::bearer)
                .unwrap_or_default();
            let result =
                veoveo_mcp_conformance::run_hosted_server_conformance(&profile, &credentials)
                    .await?;
            if let Some(parent) = report.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut bytes = serde_json::to_vec_pretty(&result)?;
            bytes.push(b'\n');
            std::fs::write(report, bytes)?;
            anyhow::ensure!(result.passed(), "hosted-server conformance failed");
            return Ok(());
        }
        Cmd::Schemas { output_dir } => {
            std::fs::create_dir_all(output_dir)?;
            for (name, schema) in [
                (
                    "mcp-conformance-profile.schema.json",
                    veoveo_mcp_conformance::hosted_server_conformance_profile_schema(),
                ),
                (
                    "mcp-conformance-report.schema.json",
                    veoveo_mcp_conformance::conformance_report_schema(),
                ),
            ] {
                std::fs::write(output_dir.join(name), serde_json::to_vec_pretty(&schema)?)?;
            }
            return Ok(());
        }
        Cmd::AuthDiscovery {
            metadata_url,
            required_scopes,
            required_extensions,
            authorization_server_metadata_url,
            authorization_server_jwks_url,
            required_jwks_key_ids,
            required_grant_types,
            required_grant_profiles,
            required_token_auth_methods,
        } => {
            return auth_discovery::cmd_auth_discovery(auth_discovery::AuthDiscoveryCheck {
                endpoint_url: &args.url,
                metadata_url: metadata_url.as_deref(),
                required_scopes,
                required_extensions,
                authorization_server_metadata_url: authorization_server_metadata_url.as_deref(),
                authorization_server_jwks_url: authorization_server_jwks_url.as_deref(),
                required_jwks_key_ids,
                required_grant_types,
                required_grant_profiles,
                required_token_auth_methods,
            })
            .await;
        }
        _ => {}
    }
    let capability = match &args.cmd {
        Cmd::Call { task: false, .. } => TaskCapability::Disabled,
        _ => TaskCapability::Enabled,
    };
    let client = client::connect(&args, capability).await?;
    let result = match args.cmd {
        Cmd::Info => cmd_info(&client).await,
        Cmd::Tools => cmd_tools(&client).await,
        Cmd::Resources => cmd_resources(&client).await,
        Cmd::AppsCheck => cmd_apps_check(&client).await,
        Cmd::Prompts => cmd_prompts(&client).await,
        Cmd::Resource { uri } => cmd_resource(&client, uri).await,
        Cmd::Prompt { name, arguments } => cmd_prompt(&client, name, arguments).await,
        Cmd::Call {
            tool_name,
            arguments,
            task,
        } => cmd_call(&client, tool_name, arguments, task).await,
        Cmd::TaskCall {
            tool_name,
            arguments,
            timeout_seconds,
        } => {
            cmd_task_call(
                &client,
                tool_name,
                arguments,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        Cmd::CompleteResource {
            uri,
            argument,
            prefix,
        } => cmd_complete_resource(&client, uri, argument, prefix).await,
        Cmd::OAuthLogin(_)
        | Cmd::KnowledgeSource(_)
        | Cmd::Certify { .. }
        | Cmd::Schemas { .. }
        | Cmd::AuthDiscovery { .. } => unreachable!("handled before MCP connection"),
    };
    client.cancel().await?;
    result
}
