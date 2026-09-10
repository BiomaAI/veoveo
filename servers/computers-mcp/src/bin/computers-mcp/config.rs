use clap::Parser;
use std::path::PathBuf;
use veoveo_task_runtime::{StoreAuthLevel, TaskRuntimeConfig};

// Credentials are never included in Debug output or environment diagnostics.
#[derive(Parser)]
#[command(name = "computers-mcp", about = "Veoveo Computers service")]
pub struct Args {
    #[arg(long, env = "VEOVEO_COMPUTERS_CONFIG")]
    pub config: PathBuf,
    #[arg(long, env = "VEOVEO_SURREAL_ENDPOINT")]
    surreal_endpoint: String,
    #[arg(long, env = "VEOVEO_SURREAL_NAMESPACE")]
    surreal_namespace: String,
    #[arg(long, env = "VEOVEO_SURREAL_DATABASE")]
    surreal_database: String,
    #[arg(long, env = "VEOVEO_SURREAL_AUTH_LEVEL", value_parser = database_auth)]
    surreal_auth_level: StoreAuthLevel,
    #[arg(long, env = "VEOVEO_SURREAL_USERNAME")]
    surreal_username: String,
    #[arg(long, env = "VEOVEO_SURREAL_PASSWORD", hide_env_values = true)]
    surreal_password: String,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub internal_trust_jwks: String,
}
fn database_auth(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        _ => Err("Computers requires database-scoped SurrealDB credentials".into()),
    }
}
impl Args {
    pub fn store(self) -> TaskRuntimeConfig {
        TaskRuntimeConfig::new(
            self.surreal_endpoint,
            self.surreal_namespace,
            self.surreal_database,
            self.surreal_auth_level,
            self.surreal_username,
            self.surreal_password,
        )
    }
}
