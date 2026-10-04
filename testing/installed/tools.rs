//! Typed owner requests over the official SDK; no domain assertions live here.
use anyhow::{Context, Result, ensure};
use rmcp::{Peer, RoleClient, model::CallToolRequestParams};
use serde::{Serialize, de::DeserializeOwned};
use veoveo_gateway_contract::GatewayToolName;

pub async fn call<I: Serialize, O: DeserializeOwned>(
    peer: &Peer<RoleClient>,
    name: GatewayToolName,
    input: &I,
) -> Result<O> {
    let arguments = serde_json::to_value(input)?
        .as_object()
        .cloned()
        .context("tool request must be an object")?;
    let result = peer
        .call_tool(CallToolRequestParams::new(name.to_string()).with_arguments(arguments))
        .await?;
    ensure!(
        result.is_error != Some(true),
        "fixture mutation returned a tool error"
    );
    serde_json::from_value(
        result
            .structured_content
            .context("fixture mutation omitted structured output")?,
    )
    .context("decode fixture mutation receipt")
}
