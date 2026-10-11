//! Preserve definitive MCP errors without treating host failures as protocol replies.
use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum ErrorKind {
    McpProtocol,
}
#[derive(Serialize)]
struct ProtocolFailure<'a> {
    kind: ErrorKind,
    error: &'a rmcp::ErrorData,
}

pub(super) fn category(error: &rmcp::ServiceError) -> &'static str {
    if matches!(error, rmcp::ServiceError::McpError(_)) {
        "mcpProtocol"
    } else {
        "hostOrTransport"
    }
}

pub(super) fn log_subscription_failure(error: &ResourceSubscriptionError) {
    let category = match error {
        ResourceSubscriptionError::Capacity { .. } => "capacity",
        ResourceSubscriptionError::IdentityConflict => "identityConflict",
        ResourceSubscriptionError::NotAdmitted => "notAdmitted",
        ResourceSubscriptionError::ClientClosing => "clientClosing",
        ResourceSubscriptionError::Upstream(_) => "upstream",
    };
    tracing::error!(
        error_category = category,
        "console App resource subscription batch failed"
    );
}

pub(super) fn service_error(error: rmcp::ServiceError, host_message: &'static str) -> Response {
    let rmcp::ServiceError::McpError(error) = error else {
        return call_error(StatusCode::BAD_GATEWAY, host_message);
    };
    let body = match serde_json::to_vec(&ProtocolFailure {
        kind: ErrorKind::McpProtocol,
        error: &error,
    }) {
        Ok(body) if body.len() <= MAX_CALL_RESULT_BYTES => body,
        _ => {
            return call_error(
                StatusCode::BAD_GATEWAY,
                "MCP error response exceeds the cap",
            );
        }
    };
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        [(CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    #[test]
    fn subscription_failure_log_does_not_format_upstream_protocol_payload() {
        use std::sync::{Arc, Mutex};
        use tracing::{
            Event, Subscriber,
            field::{Field, Visit},
            span::{Attributes, Id, Record},
        };
        #[derive(Clone)]
        struct Capture(Arc<Mutex<String>>);
        struct Fields<'a>(&'a mut String);
        impl Visit for Fields<'_> {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                use std::fmt::Write;
                write!(self.0, "{}={value:?} ", field.name()).unwrap();
            }
        }
        impl Subscriber for Capture {
            fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
                true
            }
            fn new_span(&self, _: &Attributes<'_>) -> Id {
                Id::from_u64(1)
            }
            fn record(&self, _: &Id, _: &Record<'_>) {}
            fn record_follows_from(&self, _: &Id, _: &Id) {}
            fn event(&self, event: &Event<'_>) {
                event.record(&mut Fields(&mut self.0.lock().unwrap()));
            }
            fn enter(&self, _: &Id) {}
            fn exit(&self, _: &Id) {}
        }
        let protocol = rmcp::ErrorData::invalid_params(
            "private-protocol-message-sentinel",
            Some(serde_json::json!({"privateData":"opaque-data-sentinel"})),
        );
        let upstream = ResourceSubscriptionError::Upstream(
            anyhow::Error::new(rmcp::ServiceError::McpError(protocol))
                .context("upstream-chain-sentinel"),
        );
        // The real owning Display exposes this chain; production logging must not use it.
        assert!(upstream.to_string().contains("opaque-data-sentinel"));
        let output = Arc::new(Mutex::new(String::new()));
        tracing::subscriber::with_default(Capture(output.clone()), || {
            log_subscription_failure(&upstream);
        });
        let output = output.lock().unwrap();
        assert!(output.contains("error_category=\"upstream\""));
        for forbidden in [
            "-32602",
            "private-protocol-message-sentinel",
            "opaque-data-sentinel",
            "upstream-chain-sentinel",
            "privateData",
        ] {
            assert!(
                !output.contains(forbidden),
                "private upstream detail reached tracing output"
            );
        }
    }

    #[tokio::test]
    async fn definitive_protocol_error_preserves_standard_data_but_transport_stays_generic() {
        let expected = rmcp::ErrorData::invalid_params(
            "request refused",
            Some(serde_json::json!({"opaque":"sentinel"})),
        );
        assert!(!is_transport_error(&rmcp::ServiceError::McpError(
            expected.clone()
        )));
        assert!(is_transport_error(&rmcp::ServiceError::TransportClosed));
        let response = service_error(
            rmcp::ServiceError::McpError(expected.clone()),
            "tool call failed",
        );
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), MAX_CALL_RESULT_BYTES)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["kind"], "mcpProtocol");
        assert_eq!(value["error"], serde_json::to_value(expected).unwrap());
        let response = service_error(rmcp::ServiceError::TransportClosed, "tool call failed");
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let body = to_bytes(response.into_body(), MAX_CALL_RESULT_BYTES)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"], "tool call failed");
        assert!(value.get("kind").is_none());
    }
    #[tokio::test]
    async fn oversized_protocol_error_is_generic_and_keeps_session_headers() {
        let error = rmcp::ErrorData::internal_error(
            "refused",
            Some(serde_json::json!({"opaque":"x".repeat(MAX_CALL_RESULT_BYTES)})),
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-veoveo-csrf-token",
            HeaderValue::from_static("fixture-rotation"),
        );
        let response = with_session_headers(
            service_error(rmcp::ServiceError::McpError(error), "tool call failed"),
            headers,
        );
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            response.headers()["x-veoveo-csrf-token"],
            "fixture-rotation"
        );
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("opaque"));
    }
}
