//! Generic Veoveo MCP conformance CLI.
//!
//! Exercises every surface the server exposes: authorization discovery, resources (+templates),
//! completions, final-extension tasks, subscriptions, and notifications
//! (progress, tasks/status, resources/updated, resources/list_changed).

use anyhow::Result;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

use clap::Parser;

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
use cli::Args;
use cli::Cmd;
use client::TaskCapability;
use client::bearer_token_from_args;
use client::connect;
use veoveo_artifact_service::smoke_output::save_output_uri;
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
    let client = connect(&args, TaskCapability::Enabled).await?;
    let uris = ServerResourceUris::new(veoveo_types::ResourceScheme::parse(args.scheme.clone())?);
    let result = match args.cmd {
        Cmd::Artifact {
            artifact_id,
            output_dir,
        } => {
            std::fs::create_dir_all(&output_dir)?;
            let uri = uris.artifact_uri(artifact_id);
            let http = reqwest::Client::new();
            save_output_uri(&client, &uris, &http, &output_dir, uri.as_str()).await
        }
    };
    client.cancel().await?;
    result
}
