//! Gateway startup loads the installation-owned model contract.
use anyhow::{Context, Result, ensure};
pub(crate) use veoveo_mcp_contract::agent_management::ModelConnection;
use veoveo_mcp_contract::agent_management::validate_model_connections;
use veoveo_mcp_gateway::GatewayCatalog;

pub(crate) fn from_env(catalog: &GatewayCatalog) -> Result<Vec<ModelConnection>> {
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

pub(crate) fn validate(models: &[ModelConnection], catalog: &GatewayCatalog) -> Result<()> {
    validate_model_connections(models, catalog.control_plane())
}
