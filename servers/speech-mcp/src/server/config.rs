use clap::Parser;
use secrecy::SecretString;
use std::path::PathBuf;
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Parser)]
#[command(name = "speech-mcp")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8811)]
    pub port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub public_base_url: String,
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub artifact_service_url: String,
    #[arg(long, default_value = "/opt/speech/bin/python")]
    pub python: PathBuf,
    #[arg(long, default_value_t = 4)]
    pub inference_capacity: u8,
    #[arg(long, default_value_t = 2)]
    pub concurrent_recordings: usize,
    #[arg(long, default_value_t = false)]
    pub allow_loopback_hosts: bool,
    #[arg(long = "allowed-host")]
    pub allowed_hosts: Vec<String>,
    #[arg(long, env = "VEOVEO_SURREAL_ENDPOINT")]
    pub surreal_endpoint: String,
    #[arg(long, env = "VEOVEO_SURREAL_NAMESPACE")]
    pub surreal_namespace: String,
    #[arg(long, env = "VEOVEO_SURREAL_DATABASE")]
    pub surreal_database: String,
    #[arg(long, env = "VEOVEO_SURREAL_AUTH_LEVEL", value_parser = database_auth)]
    pub surreal_auth_level: StoreAuthLevel,
    #[arg(long, env = "VEOVEO_SURREAL_USERNAME")]
    pub surreal_username: String,
    #[arg(long, env = "VEOVEO_SURREAL_PASSWORD", hide_env_values = true, value_parser = secret)]
    pub surreal_password: SecretString,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub internal_trust_jwks: String,
}

fn secret(value: &str) -> Result<SecretString, String> {
    if value.is_empty() {
        Err("secret must not be empty".into())
    } else {
        Ok(value.into())
    }
}
fn database_auth(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        _ => Err("Speech requires database-scoped credentials".into()),
    }
}
