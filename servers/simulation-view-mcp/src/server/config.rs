use std::{net::IpAddr, path::PathBuf, sync::Arc, time::Duration};

use clap::Parser;
use secrecy::SecretString;
use url::Url;
use veoveo_mcp_contract::{
    LiveMediaEndpoint, LiveMediaTransport, PublicDeployment, is_valid_live_signaling_url,
    parse_allowed_host_authority,
};
use veoveo_platform_store::{StoreAuthLevel, StoreConfig, StoreCredentials};
use veoveo_simulation_scene::GeospatialLayerCatalog;

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
    #[arg(
        long,
        env = "SIMULATION_VIEW_RECONCILE_INTERVAL_SECONDS",
        default_value_t = 10
    )]
    pub reconcile_interval_seconds: u64,
    #[arg(
        long,
        env = "SIMULATION_VIEW_AUTHORIZATION_RENEWAL_LEAD_SECONDS",
        default_value_t = 300
    )]
    pub authorization_renewal_lead_seconds: u64,
    #[arg(
        long,
        env = "SIMULATION_VIEW_RECONCILE_RETRY_MAX_SECONDS",
        default_value_t = 60
    )]
    pub reconcile_retry_max_seconds: u64,
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
        env = "ARTIFACT_SERVICE_URL",
        default_value = "http://artifact-service:8790"
    )]
    pub artifact_service_url: String,
    #[arg(
        long,
        env = "SIMULATION_VIEW_LAYER_CATALOG",
        default_value = "/etc/veoveo/simulation-view/layers.json"
    )]
    pub layer_catalog: PathBuf,
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
    #[arg(long, env = "SIMULATION_VIEW_PUBLIC_MEDIA_IP")]
    pub public_media_ip: IpAddr,
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
        anyhow::ensure!(
            is_valid_live_signaling_url(&self.public_signaling_url),
            "public signaling URL must be credential-free HTTPS/WSS or exact-loopback WS"
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
        anyhow::ensure!(
            self.reconcile_interval_seconds > 0
                && self.authorization_renewal_lead_seconds > 0
                && self.authorization_renewal_lead_seconds < 24 * 60 * 60
                && self.reconcile_retry_max_seconds >= self.reconcile_interval_seconds,
            "Simulation View reconciliation timing is invalid"
        );
        let _ = self.store_config()?;
        let _ = crate::artifacts::SceneArtifactMaterializer::new(
            &self.artifact_service_url,
            &self.renderer_endpoint,
            &self.renderer_control_token,
        )?;
        let _ = SimulationViewService::new(self.service_config()?)?;
        Ok(())
    }

    pub fn store_config(&self) -> anyhow::Result<StoreConfig> {
        Ok(StoreConfig::builder(
            &self.surreal_endpoint,
            self.surreal_namespace.clone(),
            self.surreal_database.clone(),
            StoreCredentials::new(
                self.surreal_auth_level,
                self.surreal_username.clone(),
                self.surreal_password.clone(),
            ),
        )
        .build()?)
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
                media_host: self.public_media_ip,
                media_port: self.public_media_port,
            },
            maximum_frame_age_ms: self.maximum_frame_age_ms,
            layer_catalog: Arc::new(GeospatialLayerCatalog::from_path(&self.layer_catalog)?),
        })
    }
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("Simulation View requires database-scoped SurrealDB credentials".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
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
