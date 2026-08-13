//! Canonical stateless MCP transport configuration for Veoveo HTTP endpoints.

use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, session::never::NeverSessionManager,
};

/// Builds the only supported configuration for a Veoveo Streamable HTTP
/// endpoint.
///
/// Ordinary request responses use JSON. The transport opens an SSE response
/// only when a final protocol flow, such as `subscriptions/listen`, requires a
/// stream. Every non-discovery request must carry the final per-request
/// protocol metadata.
pub fn canonical_streamable_http_server_config() -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_stateless_protocol_metadata_required(true)
}

/// Returns rmcp's no-session transport adapter.
///
/// `StreamableHttpService` requires a `SessionManager` type parameter even in
/// stateless mode. This adapter rejects every session operation and has no
/// replay store.
pub fn stateless_session_manager() -> Arc<NeverSessionManager> {
    Arc::new(NeverSessionManager::default())
}
