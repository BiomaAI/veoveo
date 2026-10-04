//! Gateway startup loads the installation-owned model contract.
pub use crate::contract::authoring::ModelConnection;
use crate::contract::authoring::validate_model_connections;
use anyhow::{Context, Result, ensure};
use veoveo_mcp_gateway::GatewayCatalog;

pub fn from_env(catalog: &GatewayCatalog) -> Result<Vec<ModelConnection>> {
    let source = std::env::var("VEOVEO_AGENT_MODELS").unwrap_or_else(|_| "[]".into());
    ensure!(
        source.len() <= 128 * 1024,
        "agent model configuration exceeds 128 KiB"
    );
    let models: Vec<ModelConnection> =
        serde_json::from_str(&source).context("invalid VEOVEO_AGENT_MODELS")?;
    validate(&models, catalog)?;
    Ok(models)
}

pub fn validate(models: &[ModelConnection], catalog: &GatewayCatalog) -> Result<()> {
    validate_model_connections(
        models,
        &crate::gateway::installation::installation_facts(catalog)?,
    )
}
