//! Canonical stateless MCP transport configuration for Veoveo HTTP endpoints.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, Response, header},
    middleware::Next,
};
use futures::StreamExt as _;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, session::never::NeverSessionManager,
};

/// Maximum bytes in one fully serialized terminal MCP JSON response.
pub const MAX_MCP_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

const RESPONSE_BUDGET_ERROR_CODE: i64 = -32_010;

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

/// Replaces an oversized serialized JSON-RPC response with one bounded error.
///
/// The caller supplies bytes after final JSON serialization. The replacement
/// never contains a prefix or fragment of the original result.
pub fn bound_serialized_jsonrpc_response(
    request_id: Option<&serde_json::Value>,
    serialized: Vec<u8>,
) -> Vec<u8> {
    if serialized.len() <= MAX_MCP_RESPONSE_BYTES {
        return serialized;
    }
    let actual_bytes = serialized.len();
    serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.cloned().unwrap_or(serde_json::Value::Null),
        "error": {
            "code": RESPONSE_BUDGET_ERROR_CODE,
            "message": "serialized MCP response exceeds the byte budget",
            "data": {
                "code": "response_budget_exceeded",
                "maximum_bytes": MAX_MCP_RESPONSE_BYTES,
                "actual_bytes": actual_bytes,
            }
        }
    }))
    .expect("the static response-budget diagnostic serializes")
}

/// Applies the canonical terminal MCP response budget after rmcp has produced
/// the final JSON bytes and before Axum starts transport delivery.
pub async fn enforce_serialized_mcp_response(request: Request<Body>, next: Next) -> Response<Body> {
    let response = next.run(request).await;
    let is_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !is_json {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let mut stream = body.into_data_stream();
    let mut serialized = Vec::new();
    let mut collection_failure = None;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) if serialized.len().saturating_add(chunk.len()) <= MAX_MCP_RESPONSE_BYTES => {
                serialized.extend_from_slice(&chunk);
            }
            Ok(_) => {
                collection_failure = Some("response_budget_exceeded");
                break;
            }
            Err(_) => {
                collection_failure = Some("response_serialization_failed");
                break;
            }
        }
    }
    let serialized = match collection_failure {
        Some(code) => response_collection_diagnostic(code),
        None => serialized,
    };
    let request_id = serde_json::from_slice::<serde_json::Value>(&serialized)
        .ok()
        .and_then(|value| value.get("id").cloned());
    let bounded = bound_serialized_jsonrpc_response(request_id.as_ref(), serialized);
    parts.headers.remove(header::CONTENT_LENGTH);
    if let Ok(length) = bounded.len().to_string().parse() {
        parts.headers.insert(header::CONTENT_LENGTH, length);
    }
    Response::from_parts(parts, Body::from(bounded))
}

fn response_collection_diagnostic(code: &'static str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": null,
        "error": {
            "code": RESPONSE_BUDGET_ERROR_CODE,
            "message": if code == "response_budget_exceeded" {
                "serialized MCP response exceeds the byte budget"
            } else {
                "serialized MCP response could not be collected"
            },
            "data": {
                "code": code,
                "maximum_bytes": MAX_MCP_RESPONSE_BYTES,
            }
        }
    }))
    .expect("the static response-collection diagnostic serializes")
}

#[cfg(test)]
mod response_budget_tests {
    use super::*;
    use axum::{Json, Router, body::to_bytes, middleware, routing::get};
    use tower::ServiceExt as _;

    #[test]
    fn oversized_serialized_response_becomes_one_bounded_diagnostic() {
        let request_id = serde_json::json!("request-7");
        let oversized = vec![b'x'; MAX_MCP_RESPONSE_BYTES + 1];

        let bounded = bound_serialized_jsonrpc_response(Some(&request_id), oversized);

        assert!(bounded.len() <= MAX_MCP_RESPONSE_BYTES);
        let diagnostic: serde_json::Value = serde_json::from_slice(&bounded).unwrap();
        assert_eq!(diagnostic["id"], request_id);
        assert_eq!(
            diagnostic["error"]["data"]["code"],
            "response_budget_exceeded"
        );
        assert_eq!(
            diagnostic["error"]["data"]["maximum_bytes"],
            MAX_MCP_RESPONSE_BYTES
        );
        assert!(diagnostic.get("result").is_none());
    }

    #[test]
    fn response_at_the_serialized_limit_is_unchanged() {
        let bytes = vec![b'x'; MAX_MCP_RESPONSE_BYTES];
        assert_eq!(
            bound_serialized_jsonrpc_response(None, bytes.clone()),
            bytes
        );
    }

    #[tokio::test]
    async fn middleware_discards_an_oversized_response_instead_of_sending_a_prefix() {
        let router = Router::new()
            .route(
                "/",
                get(|| async {
                    Json(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": "request-9",
                        "result": "x".repeat(MAX_MCP_RESPONSE_BYTES),
                        "partial_sentinel": "must-not-survive",
                    }))
                }),
            )
            .layer(middleware::from_fn(enforce_serialized_mcp_response));

        let response = router
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let bytes = to_bytes(response.into_body(), MAX_MCP_RESPONSE_BYTES)
            .await
            .unwrap();
        let diagnostic: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(
            diagnostic["error"]["data"]["code"],
            "response_budget_exceeded"
        );
        assert!(diagnostic.get("result").is_none());
        assert!(!String::from_utf8_lossy(&bytes).contains("must-not-survive"));
    }
}
