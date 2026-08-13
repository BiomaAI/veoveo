use std::borrow::Cow;

use rmcp::{
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, ErrorData as McpError, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo, ServerPeerInfo,
    },
    service::{Peer, RequestContext, RoleClient, RoleServer, ServiceError},
};

/// MCP server that forwards the tool surface of one stdio MCP child.
///
/// All stateless HTTP requests share the single child process. The child may
/// own an explicit application resource (for Rerun, one viewer), but no MCP
/// protocol state is retained by the HTTP endpoint.
#[derive(Clone)]
pub(crate) struct BridgeMcp {
    child: Peer<RoleClient>,
    info: ServerInfo,
}

impl BridgeMcp {
    pub(crate) fn new(child: Peer<RoleClient>, info: ServerInfo) -> Self {
        Self { child, info }
    }
}

/// Advertise a tools-only surface while preserving the child's identity and
/// instructions, so clients see which server they are really talking to.
pub(crate) fn bridge_server_info(child: &ServerPeerInfo) -> ServerInfo {
    let mut info = ServerInfo::default();
    info.capabilities = ServerCapabilities::builder().enable_tools().build();
    if let Some(server_info) = child.server_info.clone() {
        info.server_info = server_info;
    }
    info.instructions = child.instructions.clone();
    info
}

fn child_error(err: ServiceError) -> McpError {
    match err {
        ServiceError::McpError(err) => err,
        other => McpError::internal_error(format!("stdio MCP child request failed: {other}"), None),
    }
}

impl ServerHandler for BridgeMcp {
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
    }

    fn get_info(&self) -> ServerInfo {
        self.info.clone()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.child.list_tools(request).await.map_err(child_error)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.child
            .call_tool(request)
            .await
            .map(Into::into)
            .map_err(child_error)
    }
}
