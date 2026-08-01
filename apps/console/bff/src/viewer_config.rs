use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::Serialize;

use crate::{AppState, config::RerunMapProvider, session::read_session};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RerunMapConfig<'a> {
    provider: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic: Option<&'a str>,
}

impl<'a> From<&'a RerunMapProvider> for RerunMapConfig<'a> {
    fn from(provider: &'a RerunMapProvider) -> Self {
        match provider {
            RerunMapProvider::OpenStreetMap => Self {
                provider: "openStreetMap",
                access_token: None,
                diagnostic: None,
            },
            RerunMapProvider::Mapbox {
                access_token,
                diagnostic,
            } => Self {
                provider: "mapbox",
                access_token: access_token.as_deref(),
                diagnostic: *diagnostic,
            },
        }
    }
}

pub(crate) async fn rerun_map_config(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let Some(session) = read_session(&request_headers, &state.sessions) else {
        return crate::api::unauthorized(&state);
    };
    if session.is_expired(Utc::now().timestamp()) {
        return crate::api::unauthorized(&state);
    }
    let session = match crate::oauth::upstream_session(&state, session).await {
        Ok(session) => session,
        Err(error) => {
            tracing::warn!(%error, "console session refresh failed");
            return crate::api::unauthorized(&state);
        }
    };
    let mut response_headers = match crate::api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    response_headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (
        response_headers,
        axum::Json(RerunMapConfig::from(state.config.rerun_map_provider())),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_map_payload_contains_the_token_only_for_mapbox() {
        let open_street_map =
            serde_json::to_value(RerunMapConfig::from(&RerunMapProvider::OpenStreetMap)).unwrap();
        assert_eq!(
            open_street_map,
            serde_json::json!({"provider": "openStreetMap"})
        );

        let mapbox = RerunMapProvider::Mapbox {
            access_token: Some("pk.browser-token".to_owned()),
            diagnostic: None,
        };
        assert_eq!(
            serde_json::to_value(RerunMapConfig::from(&mapbox)).unwrap(),
            serde_json::json!({
                "provider": "mapbox",
                "accessToken": "pk.browser-token"
            })
        );

        let unavailable = RerunMapProvider::Mapbox {
            access_token: None,
            diagnostic: Some("installation token is unavailable"),
        };
        assert_eq!(
            serde_json::to_value(RerunMapConfig::from(&unavailable)).unwrap(),
            serde_json::json!({
                "provider": "mapbox",
                "diagnostic": "installation token is unavailable"
            })
        );
    }
}
