use crate::{AppState, api};
use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::time::Duration;
use veoveo_mcp_contract::ConsoleBootstrap;

pub(crate) async fn session(State(state): State<AppState>, request: Request) -> Response {
    if request.uri().query().is_some() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let (parts, body) = request.into_parts();
    if !matches!(
        tokio::time::timeout(Duration::from_secs(5), to_bytes(body, 0)).await,
        Ok(Ok(_))
    ) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let session = match api::upstream_session(&state, &parts.headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let headers = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let upstream = state
            .http
            .get(state.config.session_url())
            .header(header::HOST, state.config.gateway_host())
            .bearer_auth(&session.session.access_token)
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        match upstream.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED => return Err(StatusCode::UNAUTHORIZED),
            StatusCode::FORBIDDEN => return Err(StatusCode::FORBIDDEN),
            _ => return Err(StatusCode::BAD_GATEWAY),
        }
        let body = to_bytes(Body::from_stream(upstream.bytes_stream()), 256 * 1024)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        let bootstrap: ConsoleBootstrap =
            serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_GATEWAY)?;
        if bootstrap.profile.as_str() != state.config.profile() {
            return Err(StatusCode::BAD_GATEWAY);
        }
        Ok(bootstrap)
    })
    .await;
    let mut response = match result {
        Ok(Ok(bootstrap)) => Json(bootstrap).into_response(),
        Ok(Err(StatusCode::UNAUTHORIZED)) => return api::unauthorized(&state),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response.headers_mut().extend(headers);
    response
}
