//! Canonical MCP transport configuration for Veoveo HTTP endpoints.

use rmcp::transport::streamable_http_server::StreamableHttpServerConfig;

/// Builds the only supported configuration for a Veoveo Streamable HTTP
/// endpoint.
///
/// Sessions keep the response stream available for notifications,
/// subscriptions, progress, and tasks. Event-stream framing keeps responses
/// and notifications on the same ordered protocol channel.
pub fn canonical_streamable_http_server_config() -> StreamableHttpServerConfig {
    StreamableHttpServerConfig::default()
        .with_stateful_mode(true)
        .with_json_response(false)
}
