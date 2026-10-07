use anyhow::Context;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use clap::Parser;
use clap::Subcommand;
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use reqwest::header::HOST;
use reqwest::redirect::Policy;
use rmcp::model::CallToolRequestParams;
use serde_json::Value;

use veoveo_mcp_contract::GatewayTaskStatusDocument;
use veoveo_mcp_contract::GatewayTaskStatusKind;
use veoveo_mcp_contract::RELATED_TASK_META_KEY;

use veoveo_gateway_composition::smoke_support::*;
use veoveo_testing_support::*;
#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand, Debug)]
enum Cmd {
    GatewaySuite {
        /// Local gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.local.json")]
        control_plane: PathBuf,
        /// Gateway control-plane JSON used by smoke scenarios.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        smoke_control_plane: PathBuf,
    },
    GatewayPlatformStore {
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    ContractSchemas {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
    },
    Otel {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    GatewayHttp {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    GatewayKeycloak {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Keycloak realm import fixture.
        #[arg(long, default_value = "configs/keycloak/veoveo-ci-realm.json")]
        realm: PathBuf,
    },
    GatewayAuthenticated {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
    },
    GatewayTwoServers {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    GatewayConsoleStream {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    GatewayChartProjection {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    GatewayTaskRun {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
    },
    GatewayVaultSecrets {
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
}
#[path = "scenarios/suite.rs"]
mod case_0;
use case_0::*;
#[path = "scenarios/basic.rs"]
mod case_1;
use case_1::*;
#[path = "scenarios/gateway/authenticated.rs"]
mod case_2;
use case_2::*;
#[path = "scenarios/gateway/chart_projection.rs"]
mod case_3;
use case_3::*;
#[path = "scenarios/gateway/console_stream.rs"]
mod case_4;
use case_4::*;
#[path = "scenarios/gateway/http.rs"]
mod case_5;
use case_5::*;
#[path = "scenarios/gateway/keycloak.rs"]
mod case_6;
use case_6::*;
#[path = "scenarios/gateway/task_run.rs"]
mod case_7;
use case_7::*;
#[path = "scenarios/gateway/two_servers.rs"]
mod case_8;
use case_8::*;
#[path = "scenarios/secrets.rs"]
mod case_9;
use case_9::*;
fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
}
#[tokio::main]
async fn main() -> Result<()> {
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}
async fn execute() -> Result<()> {
    install_rustls_provider();
    let args = Args::parse();
    match args.cmd {
        Cmd::GatewaySuite {
            control_plane,
            smoke_control_plane,
        } => gateway_suite(&control_plane, &smoke_control_plane).await,
        Cmd::GatewayPlatformStore {
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            gateway_platform_store(&gateway_bin, &control_plane).await
        }
        Cmd::ContractSchemas { conformance_bin } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            contract_schemas(&conformance_bin)
        }
        Cmd::Otel {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            otel(&conformance_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayHttp {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            gateway_http(&conformance_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayKeycloak {
            conformance_bin,
            gateway_bin,
            control_plane,
            realm,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            gateway_keycloak(&conformance_bin, &gateway_bin, &control_plane, &realm).await
        }
        Cmd::GatewayAuthenticated {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;

            gateway_authenticated(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
            )
            .await
        }
        Cmd::GatewayTwoServers {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            gateway_two_servers(&conformance_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayConsoleStream {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            gateway_console_stream(&conformance_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayChartProjection {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            gateway_chart_projection(&conformance_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayTaskRun {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;

            gateway_task_run(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
            )
            .await
        }
        Cmd::GatewayVaultSecrets {
            gateway_bin,
            control_plane,
        } => {
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            gateway_vault_secrets(&gateway_bin, &control_plane).await
        }
    }
}
