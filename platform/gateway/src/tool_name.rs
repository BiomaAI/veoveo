use std::fmt;

use anyhow::Result;
use veoveo_mcp_contract::{GatewayToolName, LocalToolName, ServerSlug};

use crate::GatewayCatalog;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayToolProjection {
    pub server: ServerSlug,
    pub tool: LocalToolName,
}

impl GatewayToolProjection {
    pub fn new(server: ServerSlug, tool: LocalToolName) -> Self {
        Self { server, tool }
    }

    pub fn gateway_name(&self) -> Result<GatewayToolName, GatewayNameError> {
        GatewayToolName::new(format!("{}__{}", self.server, self.tool))
            .map_err(GatewayNameError::InvalidProjectedToolName)
    }

    pub fn parse(name: &GatewayToolName) -> Result<Self, GatewayNameError> {
        let Some((server, tool)) = name.as_str().split_once("__") else {
            return Err(GatewayNameError::MissingNamespace(name.clone()));
        };
        if tool.contains("__") {
            return Err(GatewayNameError::InvalidNamespaceShape(name.clone()));
        }
        Ok(Self {
            server: ServerSlug::new(server).map_err(GatewayNameError::InvalidServerSlug)?,
            tool: LocalToolName::new(tool).map_err(GatewayNameError::InvalidLocalToolName)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayNameError {
    UnknownServer(ServerSlug),
    MissingNamespace(GatewayToolName),
    InvalidNamespaceShape(GatewayToolName),
    InvalidServerSlug(veoveo_mcp_contract::IdentifierError),
    InvalidLocalToolName(veoveo_mcp_contract::IdentifierError),
    InvalidProjectedToolName(veoveo_mcp_contract::IdentifierError),
}

impl fmt::Display for GatewayNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownServer(server) => write!(f, "unknown server `{server}`"),
            Self::MissingNamespace(name) => {
                write!(f, "gateway tool `{name}` is missing server namespace")
            }
            Self::InvalidNamespaceShape(name) => {
                write!(f, "gateway tool `{name}` has an invalid namespace shape")
            }
            Self::InvalidServerSlug(err)
            | Self::InvalidLocalToolName(err)
            | Self::InvalidProjectedToolName(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for GatewayNameError {}

impl GatewayCatalog {
    pub fn project_tool_name(
        &self,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<GatewayToolName, GatewayNameError> {
        if self.server(server).is_none() {
            return Err(GatewayNameError::UnknownServer(server.clone()));
        }
        GatewayToolProjection::new(server.clone(), tool.clone()).gateway_name()
    }

    pub fn parse_tool_name(
        &self,
        name: &GatewayToolName,
    ) -> Result<GatewayToolProjection, GatewayNameError> {
        let projection = GatewayToolProjection::parse(name)?;
        if self.server(&projection.server).is_none() {
            return Err(GatewayNameError::UnknownServer(projection.server));
        }
        Ok(projection)
    }
}
