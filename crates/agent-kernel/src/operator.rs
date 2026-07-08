//! The operator channel: a loopback HTTP surface for humans (and harnesses).
//!
//! Binds 127.0.0.1 (ephemeral port by default), writes the bound port to
//! `{data_dir}/operator.port`, and speaks four routes: prompt injection,
//! agent status, parked elicitations, and elicitation answers. An optional
//! static bearer from `AGENT_OPERATOR_TOKEN` gates every route.

use std::{net::SocketAddr, path::Path, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use uuid::Uuid;

use crate::{
    elicitation::{ElicitAnswer, ElicitationWaiters, deliver_answer},
    ledger::KernelLedger,
    wake::{WakeBus, WakeEvent},
};

#[derive(Clone)]
pub struct OperatorState {
    ledger: KernelLedger,
    bus: WakeBus,
    waiters: ElicitationWaiters,
    token: Option<Arc<str>>,
}

pub const OPERATOR_PORT_FILE: &str = "operator.port";
pub const OPERATOR_TOKEN_ENV: &str = "AGENT_OPERATOR_TOKEN";

pub async fn serve(
    ledger: KernelLedger,
    bus: WakeBus,
    waiters: ElicitationWaiters,
    data_dir: &Path,
    port: Option<u16>,
) -> Result<SocketAddr> {
    let state = OperatorState {
        ledger,
        bus,
        waiters,
        token: std::env::var(OPERATOR_TOKEN_ENV).ok().map(Arc::from),
    };
    let router = Router::new()
        .route("/v1/prompt", post(post_prompt))
        .route("/v1/status", get(get_status))
        .route("/v1/elicitations", get(get_elicitations))
        .route(
            "/v1/elicitations/{id}/answer",
            post(post_elicitation_answer),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port.unwrap_or(0)))
        .await
        .context("binding the operator endpoint")?;
    let addr = listener.local_addr()?;
    std::fs::write(data_dir.join(OPERATOR_PORT_FILE), addr.port().to_string())?;
    tokio::spawn(async move {
        if let Err(err) = axum::serve(listener, router).await {
            tracing::error!(%err, "operator endpoint failed");
        }
    });
    tracing::info!(%addr, "operator endpoint listening");
    Ok(addr)
}

fn authorize(state: &OperatorState, headers: &HeaderMap) -> Result<(), Response> {
    let Some(expected) = &state.token else {
        return Ok(());
    };
    let presented = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if presented == Some(expected.as_ref()) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "operator token required").into_response())
    }
}

#[derive(Debug, serde::Deserialize)]
struct PromptRequest {
    text: String,
}

async fn post_prompt(
    State(state): State<OperatorState>,
    headers: HeaderMap,
    Json(request): Json<PromptRequest>,
) -> Response {
    if let Err(denied) = authorize(&state, &headers) {
        return denied;
    }
    if request.text.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "prompt text is empty").into_response();
    }
    let event = WakeEvent::operator(&request.text);
    let wake_id = event.wake_id;
    state.bus.send(event).await;
    Json(serde_json::json!({ "wake_id": wake_id })).into_response()
}

async fn get_status(State(state): State<OperatorState>, headers: HeaderMap) -> Response {
    if let Err(denied) = authorize(&state, &headers) {
        return denied;
    }
    let status = (|| -> Result<serde_json::Value> {
        let episodes = state.ledger.query_json(
            "SELECT seq, outcome, wake_note, summary FROM kernel.episodes
             ORDER BY seq DESC LIMIT 5",
            5,
        )?;
        let pending_tasks = state.ledger.tasks_to_watch()?.len();
        let unconsumed_results = state.ledger.unconsumed_results()?.len();
        let parked = state.ledger.parked_elicitations()?.len();
        Ok(serde_json::json!({
            "pending_tasks": pending_tasks,
            "unconsumed_results": unconsumed_results,
            "parked_elicitations": parked,
            "recent_episodes": episodes,
        }))
    })();
    match status {
        Ok(status) => Json(status).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")).into_response(),
    }
}

async fn get_elicitations(State(state): State<OperatorState>, headers: HeaderMap) -> Response {
    if let Err(denied) = authorize(&state, &headers) {
        return denied;
    }
    match state.ledger.parked_elicitations() {
        Ok(parked) => Json(serde_json::json!({
            "parked": parked
                .into_iter()
                .map(|item| {
                    serde_json::json!({
                        "elicitation_id": item.elicitation_id,
                        "related_task_id": item.related_task_id,
                        "message": item.message,
                        "schema": item
                            .schema_json
                            .and_then(|schema| serde_json::from_str::<serde_json::Value>(&schema).ok()),
                    })
                })
                .collect::<Vec<_>>()
        }))
        .into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("{err:#}")).into_response(),
    }
}

async fn post_elicitation_answer(
    State(state): State<OperatorState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<String>,
    Json(answer): Json<ElicitAnswer>,
) -> Response {
    if let Err(denied) = authorize(&state, &headers) {
        return denied;
    }
    let Ok(elicitation_id) = Uuid::parse_str(&id) else {
        return (StatusCode::BAD_REQUEST, "invalid elicitation id").into_response();
    };
    match deliver_answer(
        &state.ledger,
        &state.bus,
        &state.waiters,
        elicitation_id,
        answer,
        "operator-http",
    )
    .await
    {
        Ok(()) => Json(serde_json::json!({ "answered": elicitation_id })).into_response(),
        Err(err) => (StatusCode::CONFLICT, format!("{err:#}")).into_response(),
    }
}
