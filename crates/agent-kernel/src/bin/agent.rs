//! The agent process: one manifest, one data dir, one long life.

use std::path::Path;

use anyhow::{Context, Result, bail};
use clap::Parser;
use veoveo_agent_kernel::operator::{OPERATOR_PORT_FILE, OPERATOR_TOKEN_ENV};
use veoveo_mcp_contract::{TelemetryGuard, init_server_telemetry};

#[path = "agent/cli.rs"]
mod cli;
#[path = "agent/run.rs"]
mod run;

use cli::{Args, Cmd};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-agent-kernel", "info,veoveo_agent_kernel=debug")?;
    let args = Args::parse();
    match args.cmd {
        Cmd::Run(run_args) => run::cmd_run(run_args).await,
        Cmd::Replay(replay_args) => {
            let manifest =
                veoveo_agent_kernel::manifest::AgentManifest::load(&replay_args.manifest)?;
            let report =
                veoveo_agent_kernel::replay::replay_domain(&manifest, &replay_args.data_dir)?;
            println!(
                "{}",
                serde_json::json!({
                    "applied": report.applied,
                    "skipped": report.skipped,
                    "output": report.output_path,
                })
            );
            Ok(())
        }
        Cmd::Ask(ask_args) => {
            let response = operator_client(&ask_args.data_dir)?
                .post("/v1/prompt")
                .json(&serde_json::json!({ "text": ask_args.text }))
                .send()
                .await?;
            print_operator_response(response).await
        }
        Cmd::Status(status_args) => {
            let response = operator_client(&status_args.data_dir)?
                .get("/v1/status")
                .send()
                .await?;
            print_operator_response(response).await
        }
        Cmd::Timeline(timeline_args) => {
            let query = veoveo_agent_kernel::timeline::TimelineQuery {
                entities: timeline_args.entities,
                timeline: timeline_args.timeline,
                max_rows: timeline_args.max_rows,
            };
            let rows = veoveo_agent_kernel::timeline::query_segments(
                &timeline_args.data_dir.join(&timeline_args.rrd_dir),
                &query,
            )?;
            println!("{}", serde_json::to_string_pretty(&rows)?);
            Ok(())
        }
    }
}

/// A client for the running agent's loopback operator endpoint.
struct OperatorClient {
    base: String,
    http: reqwest::Client,
    token: Option<String>,
}

impl OperatorClient {
    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut builder = self.http.request(method, format!("{}{path}", self.base));
        if let Some(token) = &self.token {
            builder = builder.bearer_auth(token);
        }
        builder
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::POST, path)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.request(reqwest::Method::GET, path)
    }
}

fn operator_client(data_dir: &Path) -> Result<OperatorClient> {
    let port_file = data_dir.join(OPERATOR_PORT_FILE);
    let port = std::fs::read_to_string(&port_file)
        .with_context(|| format!("reading {} — is the agent running?", port_file.display()))?
        .trim()
        .parse::<u16>()
        .context("operator port file is malformed")?;
    Ok(OperatorClient {
        base: format!("http://127.0.0.1:{port}"),
        http: reqwest::Client::new(),
        token: std::env::var(OPERATOR_TOKEN_ENV).ok(),
    })
}

async fn print_operator_response(response: reqwest::Response) -> Result<()> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("operator endpoint returned {status}: {body}");
    }
    println!("{body}");
    Ok(())
}
