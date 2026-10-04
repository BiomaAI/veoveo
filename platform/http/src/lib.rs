//! Shared admission for controlled HTTP JSON request bodies.
use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;

/// Decodes an owner-controlled JSON body using Axum's content and size limits.
/// Owners close their DTOs with `deny_unknown_fields`; opaque fields stay open.
pub struct RequestJson<T>(pub T);

impl<S, T> FromRequest<S> for RequestJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection| match rejection {
                JsonRejection::JsonDataError(error) => {
                    (StatusCode::BAD_REQUEST, axum_json_diagnostic(&error)).into_response()
                }
                rejection => rejection.into_response(),
            })
    }
}

fn axum_json_diagnostic(error: &axum::extract::rejection::JsonDataError) -> String {
    let mut source: &(dyn std::error::Error + 'static) = error;
    loop {
        if let Some(error) = source.downcast_ref::<serde_path_to_error::Error<serde_json::Error>>()
        {
            return json_request_diagnostic(&error.inner().to_string());
        }
        if let Some(error) = source.downcast_ref::<serde_json::Error>() {
            return json_request_diagnostic(&error.to_string());
        }
        match source.source() {
            Some(inner) => source = inner,
            None => return "invalid JSON request body".into(),
        }
    }
}

/// Reports an undeclared key without exposing values submitted in other fields.
pub fn json_request_diagnostic(error: &str) -> String {
    if let Some(tail) = error.strip_prefix("unknown field `")
        && let Some((field, _)) = tail.split_once('`')
    {
        return format!("unknown field `{field}`");
    }
    "invalid JSON request body".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        body::{Body, to_bytes},
        extract::DefaultBodyLimit,
        http::Request,
        routing::post,
    };
    use serde::Deserialize;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tower::ServiceExt;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        nested: Nested,
        payload: serde_json::Value,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Nested {
        name: String,
        #[serde(default)]
        mode: Mode,
    }

    #[derive(Default, Deserialize)]
    enum Mode {
        #[default]
        Valid,
    }

    #[tokio::test]
    async fn closed_requests_fail_before_handler_and_opaque_payloads_pass() {
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let app = Router::new()
            .route(
                "/",
                post(move |RequestJson(input): RequestJson<Input>| {
                    let calls = observed.clone();
                    async move {
                        assert_eq!(input.nested.name, "valid");
                        assert!(matches!(input.nested.mode, Mode::Valid));
                        assert_eq!(input.payload["providerExtension"]["arbitrary"], true);
                        calls.fetch_add(1, Ordering::SeqCst);
                        StatusCode::NO_CONTENT
                    }
                }),
            )
            .layer(DefaultBodyLimit::max(1024));
        for (body, field) in [
            (
                r#"{"nested":{"name":"valid"},"payload":{},"rootExtra":true}"#,
                "rootExtra",
            ),
            (
                r#"{"nested":{"name":"valid","nestedExtra":true},"payload":{}}"#,
                "nestedExtra",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/")
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            assert!(String::from_utf8_lossy(&bytes).contains(field));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        let response = app
            .clone()
            .oneshot(
                Request::post("/")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"nested":"secret-sentinel","payload":{}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("secret-sentinel"));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        for body in [
            r#"{"nested":{"name":"valid","mode":"unknown field `secret-sentinel`"},"payload":{}}"#,
            r#"{"nested":"unknown field `secret-sentinel`","payload":{}}"#,
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/")
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            assert_eq!(String::from_utf8_lossy(&bytes), "invalid JSON request body");
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        for (body, content_type, status) in [
            (
                "{}".to_owned(),
                "text/plain",
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            (
                "x".repeat(1025),
                "application/json",
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
            ("{".to_owned(), "application/json", StatusCode::BAD_REQUEST),
            (
                r#"{"nested":{"name":"valid"},"payload":{"providerExtension":{"arbitrary":true}}}"#
                    .to_owned(),
                "application/json",
                StatusCode::NO_CONTENT,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post("/")
                        .header("content-type", content_type)
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
