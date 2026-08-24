use std::{fs, path::Path, sync::Arc};

use anyhow::Context as _;
use axum::{
    Router,
    extract::{OriginalUri, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use veoveo_mcp_contract::ServerSlug;

use crate::AppState;

const APP_ROUTE_PREFIX: &str = "/apps/";
const APP_HOST_ROUTE: &str = "/apps/{server}/{*page}";
const APP_HOST_DOCUMENT: &str = "app-host.html";
const MAX_APP_ROUTE_BYTES: usize = 4096;
const MAX_APP_PAGE_SEGMENTS: usize = 32;
const MAX_APP_SEGMENT_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StandaloneAppRoute {
    server: ServerSlug,
    page: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StandaloneAppRouteError {
    InvalidRoot,
    QueryOrFragment,
    InvalidEncoding,
    InvalidServer,
    InvalidSegment,
    InvalidDocument,
    TooLong,
}

impl StandaloneAppRoute {
    pub(crate) fn parse(candidate: &str) -> Result<Self, StandaloneAppRouteError> {
        if candidate.len() > MAX_APP_ROUTE_BYTES {
            return Err(StandaloneAppRouteError::TooLong);
        }
        if candidate.contains(['?', '#']) {
            return Err(StandaloneAppRouteError::QueryOrFragment);
        }
        let path = candidate
            .strip_prefix(APP_ROUTE_PREFIX)
            .ok_or(StandaloneAppRouteError::InvalidRoot)?;
        let mut raw_segments = path.split('/');
        let server = raw_segments
            .next()
            .ok_or(StandaloneAppRouteError::InvalidServer)
            .and_then(decode_segment)
            .and_then(|value| {
                ServerSlug::new(value).map_err(|_| StandaloneAppRouteError::InvalidServer)
            })?;
        let page = raw_segments
            .map(decode_segment)
            .collect::<Result<Vec<_>, _>>()?;
        if page.is_empty() || page.len() > MAX_APP_PAGE_SEGMENTS {
            return Err(StandaloneAppRouteError::InvalidSegment);
        }
        let document = page
            .last()
            .ok_or(StandaloneAppRouteError::InvalidDocument)?;
        if document.strip_suffix(".html").is_none_or(str::is_empty) {
            return Err(StandaloneAppRouteError::InvalidDocument);
        }
        Ok(Self { server, page })
    }

    pub(crate) fn resource_uri(&self) -> String {
        format!("ui://{}/{}", self.server, self.page.join("/"))
    }

    pub(crate) fn path(&self) -> String {
        format!("{APP_ROUTE_PREFIX}{}/{}", self.server, self.page.join("/"))
    }

    pub(crate) fn from_resource_uri(uri: &str) -> Result<Self, StandaloneAppRouteError> {
        let projected = uri
            .strip_prefix("ui://")
            .ok_or(StandaloneAppRouteError::InvalidRoot)?;
        Self::parse(&format!("{APP_ROUTE_PREFIX}{projected}"))
    }
}

fn decode_segment(raw: &str) -> Result<String, StandaloneAppRouteError> {
    if raw.is_empty() || raw.len() > MAX_APP_SEGMENT_BYTES * 3 {
        return Err(StandaloneAppRouteError::InvalidSegment);
    }
    let raw = raw.as_bytes();
    let mut decoded = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'%' {
            let high = raw
                .get(index + 1)
                .and_then(|value| hex_digit(*value))
                .ok_or(StandaloneAppRouteError::InvalidEncoding)?;
            let low = raw
                .get(index + 2)
                .and_then(|value| hex_digit(*value))
                .ok_or(StandaloneAppRouteError::InvalidEncoding)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(raw[index]);
            index += 1;
        }
    }
    if decoded.len() > MAX_APP_SEGMENT_BYTES {
        return Err(StandaloneAppRouteError::InvalidSegment);
    }
    let decoded =
        String::from_utf8(decoded).map_err(|_| StandaloneAppRouteError::InvalidEncoding)?;
    if matches!(decoded.as_str(), "" | "." | "..")
        || decoded.chars().any(|character| {
            character.is_control() || matches!(character, '/' | '\\' | '%' | '?' | '#' | '@')
        })
    {
        return Err(StandaloneAppRouteError::InvalidSegment);
    }
    Ok(decoded)
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

pub(crate) fn with_app_host_route(
    router: Router<AppState>,
    asset_dir: &Path,
) -> anyhow::Result<Router<AppState>> {
    let document = Arc::<str>::from(
        fs::read_to_string(asset_dir.join(APP_HOST_DOCUMENT)).with_context(|| {
            format!(
                "reading standalone MCP App host document in {}",
                asset_dir.display()
            )
        })?,
    );
    Ok(router.route(
        APP_HOST_ROUTE,
        get(
            move |state: State<AppState>, uri: OriginalUri, headers: HeaderMap| {
                standalone_app(state, uri, headers, document.clone())
            },
        ),
    ))
}

async fn standalone_app(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    document: Arc<str>,
) -> Response {
    let Some(path_and_query) = uri.path_and_query().map(|value| value.as_str()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(route) = StandaloneAppRoute::parse(path_and_query) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if accepts_json(&headers) {
        return crate::apps::standalone_app_bootstrap(&state, &headers, &route).await;
    }
    app_host_document(document)
}

fn app_host_document(document: Arc<str>) -> Response {
    (
        [(CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Html(document.to_string()),
    )
        .into_response()
}

fn accepts_json(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .filter_map(|item| item.split(';').next())
                .any(|item| item.trim() == "application/json")
        })
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;

    use super::*;

    fn parse(value: &str) -> Result<StandaloneAppRoute, StandaloneAppRouteError> {
        StandaloneAppRoute::parse(value)
    }

    #[test]
    fn nested_standalone_route_maps_to_one_exact_app_resource() {
        let route = parse("/apps/charts/views/main.html").expect("valid nested App route");
        assert_eq!(route.resource_uri(), "ui://charts/views/main.html");
        assert_eq!(route.path(), "/apps/charts/views/main.html");
        let encoded = parse("/apps/map/workspace%2Ehtml").expect("valid decoded document name");
        assert_eq!(encoded.resource_uri(), "ui://map/workspace.html");
    }

    #[test]
    fn projected_resource_round_trips_through_the_route_authority() {
        let route = StandaloneAppRoute::from_resource_uri("ui://map/workspace.html").unwrap();
        assert_eq!(route.path(), "/apps/map/workspace.html");
        assert_eq!(route.resource_uri(), "ui://map/workspace.html");
        assert!(StandaloneAppRoute::from_resource_uri("map://admin.html").is_err());
    }

    #[test]
    fn bootstrap_content_negotiation_is_explicit() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::ACCEPT,
            HeaderValue::from_static("text/html,application/xhtml+xml"),
        );
        assert!(!accepts_json(&headers));
        headers.insert(
            axum::http::header::ACCEPT,
            HeaderValue::from_static("application/json; charset=utf-8"),
        );
        assert!(accepts_json(&headers));
    }

    #[tokio::test]
    async fn standalone_entry_document_is_public_and_never_cached() {
        let response = app_host_document(Arc::from("<main>host</main>"));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(&body[..], b"<main>host</main>");
    }

    #[test]
    fn standalone_route_rejects_noncanonical_and_hostile_paths() {
        for rejected in [
            "/apps/map/",
            "/apps/map",
            "/apps//admin.html",
            "/apps/Map/admin.html",
            "/apps/map/./admin.html",
            "/apps/map/../admin.html",
            "/apps/map/%2e%2e/admin.html",
            "/apps/map/%2Fadmin.html",
            "/apps/map/%5cadmin.html",
            "/apps/map/admin%252ehtml",
            "/apps/map/user@example.html",
            "/apps/map/admin.html?view=other.html",
            "/apps/map/admin.html#other",
            "/apps/map/%00admin.html",
            "/apps/map/%E0%A4%A.html",
            "/apps/map/admin",
        ] {
            assert!(parse(rejected).is_err(), "{rejected} must be rejected");
        }
    }

    #[test]
    fn standalone_route_enforces_path_and_segment_bounds() {
        let oversized_segment = format!("/apps/map/{}.html", "a".repeat(256));
        assert!(parse(&oversized_segment).is_err());
        let oversized_path = format!("/apps/map/{}/view.html", "a/".repeat(32));
        assert!(parse(&oversized_path).is_err());
        let oversized_bytes = format!("/apps/map/{}.html", "a".repeat(4096));
        assert!(parse(&oversized_bytes).is_err());
    }
}
