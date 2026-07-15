use std::{path::PathBuf, time::Duration};

use clap::Parser;
use secrecy::SecretString;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

use crate::{
    contract::CaptureLimits, renderer::RendererConfig, source::SourceConfig,
    state::ViewServiceConfig,
};

#[derive(Parser)]
#[command(name = "server", about = "Headless GPU 3D Tiles View MCP server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8801)]
    pub port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub public_base_url: String,
    #[arg(long, default_value = "/etc/veoveo/view/layers.json")]
    pub layer_catalog: PathBuf,
    #[arg(long, env = "VIEW_REQUIRE_NVIDIA", default_value_t = true, action = clap::ArgAction::Set)]
    pub require_nvidia: bool,
    #[arg(long, default_value_t = 2_147_483_648)]
    pub raw_cache_bytes: u64,
    #[arg(long, default_value_t = 4_294_967_296)]
    pub decoded_cache_bytes: u64,
    #[arg(long, default_value_t = 6_442_450_944)]
    pub gpu_cache_bytes: u64,
    #[arg(long, default_value_t = 268_435_456)]
    pub max_source_response_bytes: u64,
    #[arg(long, default_value_t = 30)]
    pub source_timeout_seconds: u64,
    #[arg(long, default_value_t = 4096)]
    pub max_width_px: u32,
    #[arg(long, default_value_t = 4096)]
    pub max_height_px: u32,
    #[arg(long, default_value_t = 16_777_216)]
    pub max_pixels: u64,
    #[arg(long, default_value_t = 120_000)]
    pub max_deadline_ms: u32,
    #[arg(long, default_value_t = 512)]
    pub max_views: usize,
    #[arg(long, default_value_t = 32)]
    pub max_views_per_owner: usize,
    #[arg(long, default_value_t = 128)]
    pub max_frames: usize,
    #[arg(long, default_value_t = 268_435_456)]
    pub max_frame_bytes: u64,
    #[arg(long, default_value_t = 8_388_608)]
    pub max_single_frame_bytes: u64,
    #[arg(long, default_value_t = 16)]
    pub max_concurrent_loads: usize,
    #[arg(long, default_value_t = 2_000.0)]
    pub detail_falloff_meters: f64,
    #[arg(long, default_value_t = 1_000_000)]
    pub max_tree_nodes: usize,
    #[arg(long, default_value_t = 4)]
    pub max_captures_in_flight: usize,
    #[arg(long, default_value_t = 85)]
    pub jpeg_quality: u8,
    #[arg(long, default_value_t = false)]
    pub allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub allowed_hosts: Vec<String>,
    #[arg(long = "surreal-endpoint", env = "VEOVEO_SURREAL_ENDPOINT")]
    pub surreal_endpoint: String,
    #[arg(long = "surreal-namespace", env = "VEOVEO_SURREAL_NAMESPACE")]
    pub surreal_namespace: String,
    #[arg(long = "surreal-database", env = "VEOVEO_SURREAL_DATABASE")]
    pub surreal_database: String,
    #[arg(
        long = "surreal-auth-level",
        env = "VEOVEO_SURREAL_AUTH_LEVEL",
        value_parser = parse_database_auth_level
    )]
    pub surreal_auth_level: StoreAuthLevel,
    #[arg(long = "surreal-username", env = "VEOVEO_SURREAL_USERNAME")]
    pub surreal_username: String,
    #[arg(
        long = "surreal-password",
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    pub surreal_password: SecretString,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub internal_trust_jwks: String,
}

impl Args {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.raw_cache_bytes > 0,
            "raw cache budget must be positive"
        );
        anyhow::ensure!(
            self.decoded_cache_bytes > 0,
            "decoded cache budget must be positive"
        );
        anyhow::ensure!(
            self.gpu_cache_bytes > 0,
            "GPU cache budget must be positive"
        );
        anyhow::ensure!(
            self.max_source_response_bytes > 0,
            "source response limit must be positive"
        );
        anyhow::ensure!(
            self.source_timeout_seconds > 0,
            "source timeout must be positive"
        );
        anyhow::ensure!(
            self.max_width_px > 0 && self.max_height_px > 0 && self.max_pixels > 0,
            "capture dimensions must be positive"
        );
        anyhow::ensure!(
            self.max_deadline_ms > 0,
            "capture deadline must be positive"
        );
        anyhow::ensure!(
            self.max_views > 0
                && self.max_views_per_owner > 0
                && self.max_views_per_owner <= self.max_views,
            "view limits are invalid"
        );
        anyhow::ensure!(
            self.max_frames > 0
                && self.max_frame_bytes > 0
                && self.max_single_frame_bytes > 0
                && self.max_single_frame_bytes <= self.max_frame_bytes,
            "frame retention limits are invalid"
        );
        anyhow::ensure!(
            self.max_concurrent_loads > 0 && self.max_captures_in_flight > 0,
            "concurrency limits must be positive"
        );
        anyhow::ensure!(
            self.detail_falloff_meters.is_finite() && self.detail_falloff_meters >= 0.0,
            "detail falloff must be finite and non-negative"
        );
        anyhow::ensure!(self.max_tree_nodes > 0, "tree node limit must be positive");
        anyhow::ensure!(
            (1..=100).contains(&self.jpeg_quality),
            "JPEG quality must be between 1 and 100"
        );
        Ok(())
    }

    pub fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }

    pub fn source_config(&self) -> SourceConfig {
        SourceConfig {
            raw_cache_bytes: self.raw_cache_bytes,
            max_response_bytes: self.max_source_response_bytes,
            request_timeout: Duration::from_secs(self.source_timeout_seconds),
        }
    }

    pub fn renderer_config(&self) -> RendererConfig {
        RendererConfig {
            require_nvidia: self.require_nvidia,
            gpu_cache_bytes: self.gpu_cache_bytes,
            jpeg_quality: self.jpeg_quality,
        }
    }

    pub fn view_config(&self) -> ViewServiceConfig {
        ViewServiceConfig {
            capture_limits: CaptureLimits {
                max_width_px: self.max_width_px,
                max_height_px: self.max_height_px,
                max_pixels: self.max_pixels,
                max_deadline_ms: self.max_deadline_ms,
            },
            max_views: self.max_views,
            max_views_per_owner: self.max_views_per_owner,
            max_frames: self.max_frames,
            max_frame_bytes: self.max_frame_bytes,
            max_single_frame_bytes: self.max_single_frame_bytes,
            decoded_cache_bytes: self.decoded_cache_bytes,
            max_concurrent_loads: self.max_concurrent_loads,
            max_tree_nodes: self.max_tree_nodes,
            detail_falloff_meters: self.detail_falloff_meters,
        }
    }
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("view requires database-scoped SurrealDB credentials".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_owned())
        .ok_or_else(|| "expected a host authority such as view-mcp:8801".to_owned())
}
