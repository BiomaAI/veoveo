//! Agent authoring discovers tools using the admitted person's native MCP session.
use axum::http::{HeaderMap, StatusCode};
use futures::future::BoxFuture;
use secrecy::SecretString;
use std::{num::NonZeroU16, sync::Arc};
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::{GatewayProfileId, PublicDeployment};
use veoveo_mcp_gateway::{AuthenticatedSubject, http::native_mcp};

#[derive(Clone)]
pub struct CapabilityCaller {
    pub profile: GatewayProfileId,
    pub subject: AuthenticatedSubject,
    pub bearer: SecretString,
}
impl CapabilityCaller {
    pub fn new(
        profile: GatewayProfileId,
        subject: AuthenticatedSubject,
        headers: &HeaderMap,
    ) -> Result<Self, StatusCode> {
        Ok(Self {
            profile,
            subject,
            bearer: native_mcp::bearer(headers)?,
        })
    }
}

/// The authoring handler admits action and current context before discovery.
pub trait AgentsCapabilityReader: Send + Sync {
    fn required_tools<'a>(
        &'a self,
        caller: &'a CapabilityCaller,
        required: &'a [GatewayToolName],
    ) -> BoxFuture<'a, Result<Vec<rmcp::model::Tool>, StatusCode>>;
}

#[derive(Clone)]
pub struct NativeAgentCapabilityReader {
    native: native_mcp::NativeTransport,
}
impl NativeAgentCapabilityReader {
    pub fn new(
        port: NonZeroU16,
        deployment: &PublicDeployment,
        config: rmcp::model::ClientConfig,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            native: native_mcp::NativeTransport::new(port, deployment, config)?,
        })
    }
    pub fn shared(self) -> Arc<dyn AgentsCapabilityReader> {
        Arc::new(self)
    }
}
impl AgentsCapabilityReader for NativeAgentCapabilityReader {
    fn required_tools<'a>(
        &'a self,
        caller: &'a CapabilityCaller,
        required: &'a [GatewayToolName],
    ) -> BoxFuture<'a, Result<Vec<rmcp::model::Tool>, StatusCode>> {
        Box::pin(async move {
            let client = self.native.connect(&caller.profile, &caller.bearer).await?;
            let result = native_mcp::required_tools(&client, required).await;
            client.close().await;
            result
        })
    }
}

/// Authoring only consumes native discovery; it never starts Workspace workers.
pub fn client_config() -> rmcp::model::ClientConfig {
    use rmcp::model::{ClientCapabilities, ClientConfig, ElicitationCapability, Implementation};
    let mut capabilities = ClientCapabilities::default();
    capabilities
        .extensions
        .get_or_insert_default()
        .entry(rmcp::model::TASKS_EXTENSION_ID.into())
        .or_default();
    capabilities.elicitation = Some(
        ElicitationCapability::new()
            .with_form(Default::default())
            .with_url(Default::default()),
    );
    ClientConfig::new(
        capabilities,
        Implementation::new("veoveo-agents", env!("CARGO_PKG_VERSION")),
    )
}
