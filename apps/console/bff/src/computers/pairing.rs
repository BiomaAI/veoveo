//! This entry document alone may contact the validated stock CLI loopback port.
use super::fault;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use serde::Deserialize;
use veoveo_computers_contract::CliPairingInput;
use veoveo_computers_contract::ComputerId;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    callback_port: u16,
    code: String,
}
pub(super) async fn entry(
    State(state): State<AppState>,
    Path(_): Path<ComputerId>,
    Query(parameters): Query<Parameters>,
) -> Response {
    if !(CliPairingInput {
        name: "CLI".into(),
        code: parameters.code,
        callback_port: parameters.callback_port,
    })
    .is_valid()
    {
        return fault(StatusCode::BAD_REQUEST);
    }
    let Ok(index) = tokio::fs::read_to_string(state.config.asset_dir().join("index.html")).await
    else {
        return fault(StatusCode::SERVICE_UNAVAILABLE);
    };
    if index.len() > 2 * 1024 * 1024 {
        return fault(StatusCode::SERVICE_UNAVAILABLE);
    }
    let policy = format!(
        "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self' http://127.0.0.1:{}; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'",
        parameters.callback_port
    );
    (
        [
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::REFERRER_POLICY, "no-referrer".to_owned()),
            (header::CONTENT_SECURITY_POLICY, policy),
        ],
        Html(index),
    )
        .into_response()
}
