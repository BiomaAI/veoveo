use rmcp::{
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, ErrorData as McpError, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo,
    },
    service::{Peer, RequestContext, RoleClient, RoleServer, ServiceError},
};

/// MCP server that forwards the tool surface of one stdio MCP child.
///
/// All HTTP sessions share the single child process: the child owns one
/// stateful surface (for rerun, one viewer), so sessions are views onto it.
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
pub(crate) fn bridge_server_info(child: &ServerInfo) -> ServerInfo {
    let mut info = ServerInfo::default();
    info.capabilities = ServerCapabilities::builder().enable_tools().build();
    info.server_info = child.server_info.clone();
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
    ) -> Result<CallToolResult, McpError> {
        self.child.call_tool(request).await.map_err(child_error)
    }
}
