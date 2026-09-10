use super::{fault, origin};
use crate::{AppState, api};
use axum::{
    body::{Body, to_bytes},
    extract::{MatchedPath, Path, Query, Request, State},
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;
use veoveo_computers_contract::TerminalTicket;

#[derive(Deserialize)]
pub(super) struct Route {
    id: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Page {
    after: Option<Uuid>,
}

pub(super) async fn proxy(
    State(state): State<AppState>,
    route: Result<Path<Route>, axum::extract::rejection::PathRejection>,
    matched: MatchedPath,
    page: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
    request: Request,
) -> Response {
    let (Ok(Path(route)), Ok(Query(page))) = (route, page) else {
        return fault(StatusCode::BAD_REQUEST);
    };
    let Some(path) = upstream_path(matched.as_str(), route.id) else {
        return fault(StatusCode::BAD_REQUEST);
    };
    if (request.uri().query().is_some() && !(path.is_empty() && request.method() == Method::GET))
        || page.after.is_some_and(|id| id.is_nil())
    {
        return fault(StatusCode::BAD_REQUEST);
    }
    let ticket = matched.as_str().ends_with("/terminal-ticket");
    let origin = if ticket {
        match origin(&state, request.headers()) {
            Ok(value) => Some(value),
            Err(status) => return fault(status),
        }
    } else {
        None
    };
    // Body admission precedes refresh; stalled input cannot consume a new refresh token.
    let (parts, body) = request.into_parts();
    let body = match tokio::time::timeout(Duration::from_secs(5), to_bytes(body, 64 * 1024)).await {
        Ok(Ok(body)) => body,
        _ => return fault(StatusCode::PAYLOAD_TOO_LARGE),
    };
    let session = match api::upstream_session(&state, &parts.headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let response_headers = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return fault(status),
    };
    let mut url = state.config.computers_url(&path);
    if let Some(after) = page.after {
        url.query_pairs_mut()
            .append_pair("after", &after.to_string());
    }
    let mut request = state
        .http
        .request(parts.method, url)
        .header(header::HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token)
        .body(body);
    for value in parts.headers.get_all(header::CONTENT_TYPE) {
        request = request.header(header::CONTENT_TYPE, value);
    }
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let upstream = request.send().await.map_err(|_| ())?;
        let status = upstream.status();
        if status.is_redirection() {
            return Err(());
        }
        let body = to_bytes(Body::from_stream(upstream.bytes_stream()), 2 * 1024 * 1024)
            .await
            .map_err(|_| ())?;
        let body = if ticket && status == StatusCode::CREATED {
            let id = route.id.ok_or(())?;
            let mut ticket: TerminalTicket = serde_json::from_slice(&body).map_err(|_| ())?;
            if ticket.computer_id != id {
                return Err(());
            }
            ticket.endpoint = format!("/console/api/computers/{id}/terminal");
            serde_json::to_vec(&ticket).map_err(|_| ())?.into()
        } else {
            body
        };
        Ok((status, body))
    })
    .await;
    let mut response = match result {
        Ok(Ok((StatusCode::UNAUTHORIZED, _))) => return api::unauthorized(&state),
        Ok(Ok((status, body))) => {
            (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
        _ => fault(StatusCode::SERVICE_UNAVAILABLE),
    };
    // A completed refresh must reach the browser even when the following request fails.
    response.headers_mut().extend(response_headers);
    response
}

fn upstream_path(matched: &str, id: Option<Uuid>) -> Option<String> {
    if id.is_some_and(|id| id.is_nil()) {
        return None;
    }
    match (matched, id) {
        ("/console/api/computers", None) => Some(String::new()),
        ("/console/api/computers/{id}", Some(id)) => Some(format!("/{id}")),
        ("/console/api/computers/{id}/start", Some(id)) => Some(format!("/{id}/start")),
        ("/console/api/computers/{id}/stop", Some(id)) => Some(format!("/{id}/stop")),
        ("/console/api/computers/{id}/terminal-ticket", Some(id)) => {
            Some(format!("/{id}/terminal-ticket"))
        }
        _ => None,
    }
}
