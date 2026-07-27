use std::time::Duration;

use clap::Parser;
use url::Url;
use veoveo_mcp_contract::{
    LiveMediaEndpoint, LiveMediaTransport, PublicDeployment, parse_allowed_host_authority,
};

use crate::{
    contract::CapacityProfile,
    state::{SimulationViewConfig, SimulationViewService},
};

#[derive(Parser)]
#[command(
    name = "simulation-view-mcp",
    about = "Provider-neutral renderer-only Simulation View MCP server"
)]
pub(super) struct Args {
    #[arg(long, default_value_t = 8808)]
    pub port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub public_base_url: String,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub internal_trust_jwks: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_RENDERER_ENDPOINT",
        default_value = "http://simulation-view-isaac:8810"
    )]
    pub renderer_endpoint: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_POSE_ENDPOINT",
        default_value = "http://simulation-view-pose:8811"
    )]
    pub pose_endpoint: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_RENDERER_CONTROL_TOKEN",
        hide_env_values = true
    )]
    pub renderer_control_token: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_POSE_CONTROL_TOKEN",
        hide_env_values = true
    )]
    pub pose_control_token: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_RENDERER_SIGNALING_URL",
        default_value = "ws://simulation-view-isaac:49100"
    )]
    pub renderer_signaling_url: String,
    #[arg(long, env = "SIMULATION_VIEW_PUBLIC_SIGNALING_URL")]
    pub public_signaling_url: String,
    #[arg(long, env = "SIMULATION_VIEW_PUBLIC_MEDIA_HOST")]
    pub public_media_host: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_PUBLIC_MEDIA_PORT",
        default_value_t = 47998
    )]
    pub public_media_port: u16,
    #[arg(long, default_value = "rtx4090-development-v1")]
    pub capacity_profile: String,
    #[arg(long, default_value_t = 16)]
    pub maximum_logical_cameras: u32,
    #[arg(long, default_value_t = 4)]
    pub maximum_rendered_cameras: u32,
    #[arg(long, default_value_t = 2)]
    pub maximum_streamed_cameras: u32,
    #[arg(long, default_value_t = 497_664_000)]
    pub maximum_render_pixels_per_second: u64,
    #[arg(long, default_value_t = 2)]
    pub maximum_nvenc_sessions: u32,
    #[arg(long, default_value_t = 21_474_836_480)]
    pub gpu_memory_budget_bytes: u64,
    #[arg(long, default_value_t = 10_000)]
    pub maximum_entity_instances: u32,
    #[arg(long, default_value_t = 8)]
    pub maximum_cameras_per_owner: u32,
    #[arg(long, default_value_t = 12)]
    pub maximum_cameras_per_work_context: u32,
    #[arg(long, default_value_t = 4_294_967_296)]
    pub maximum_asset_bytes: u64,
    #[arg(long, default_value_t = 120)]
    pub lease_seconds: u64,
    #[arg(long, default_value_t = 500)]
    pub maximum_frame_age_ms: u32,
    #[arg(long, default_value_t = false)]
    pub allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub allowed_hosts: Vec<String>,
}

impl Args {
    pub fn validate(&self) -> anyhow::Result<()> {
        self.public_deployment()?;
        let signaling = Url::parse(&self.public_signaling_url)?;
        anyhow::ensure!(
            matches!(signaling.scheme(), "https" | "wss")
                && signaling.host_str().is_some()
                && signaling.username().is_empty()
                && signaling.password().is_none()
                && signaling.query().is_none()
                && signaling.fragment().is_none(),
            "public signaling URL must be a credential-free HTTPS or WSS URL"
        );
        let upstream = Url::parse(&self.renderer_signaling_url)?;
        anyhow::ensure!(
            upstream.scheme() == "ws"
                && upstream.host_str().is_some()
                && upstream.username().is_empty()
                && upstream.password().is_none()
                && upstream.port().is_some()
                && upstream.query().is_none()
                && upstream.fragment().is_none(),
            "renderer signaling URL must be a credential-free internal ws URL with an explicit base port"
        );
        let rendered_span = self.maximum_rendered_cameras.saturating_sub(1);
        anyhow::ensure!(
            u32::from(self.public_media_port).saturating_add(rendered_span) <= u32::from(u16::MAX)
                && u32::from(upstream.port().expect("validated explicit port"))
                    .saturating_add(rendered_span)
                    <= u32::from(u16::MAX),
            "rendered camera count exceeds the configured signaling or media port range"
        );
        validate_control_token(&self.renderer_control_token)?;
        validate_control_token(&self.pose_control_token)?;
        let _ = SimulationViewService::new(self.service_config()?)?;
        Ok(())
    }

    pub fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }

    pub fn service_config(&self) -> anyhow::Result<SimulationViewConfig> {
        Ok(SimulationViewConfig {
            capacity: CapacityProfile {
                profile: self.capacity_profile.clone(),
                maximum_logical_cameras: self.maximum_logical_cameras,
                maximum_rendered_cameras: self.maximum_rendered_cameras,
                maximum_streamed_cameras: self.maximum_streamed_cameras,
                maximum_render_pixels_per_second: self.maximum_render_pixels_per_second,
                maximum_nvenc_sessions: self.maximum_nvenc_sessions,
                gpu_memory_budget_bytes: self.gpu_memory_budget_bytes,
                maximum_entity_instances: self.maximum_entity_instances,
                maximum_cameras_per_owner: self.maximum_cameras_per_owner,
                maximum_cameras_per_work_context: self.maximum_cameras_per_work_context,
            },
            maximum_asset_bytes: self.maximum_asset_bytes,
            lease_duration: Duration::from_secs(self.lease_seconds),
            endpoint: LiveMediaEndpoint {
                transport: LiveMediaTransport::WebRtc,
                signaling_url: self.public_signaling_url.clone(),
                media_host: self.public_media_host.clone(),
                media_port: self.public_media_port,
            },
            maximum_frame_age_ms: self.maximum_frame_age_ms,
        })
    }
}

fn validate_control_token(value: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        (32..=512).contains(&value.len()) && !value.chars().any(char::is_whitespace),
        "runtime control tokens must contain 32 to 512 non-whitespace characters"
    );
    Ok(())
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_owned())
        .ok_or_else(|| "expected a host authority such as simulation-view-mcp:8808".to_owned())
}
