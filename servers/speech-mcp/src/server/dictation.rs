use crate::application::SpeechService;
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use std::sync::Arc;
use uuid::Uuid;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_speech_contract::dictation::{DictationSnapshot, MAX_CHUNK_BYTES, StartDictation};

pub(super) fn router() -> Router<Arc<SpeechService>> {
    Router::new()
        .route("/dictation", post(start))
        .route("/dictation/{id}", get(read).delete(cancel))
        .route("/dictation/{id}/chunks/{sequence}", put(chunk))
        .route("/dictation/{id}/finish", post(finish))
        .layer(DefaultBodyLimit::max(MAX_CHUNK_BYTES))
}

fn response(result: anyhow::Result<DictationSnapshot>) -> Response {
    match result {
        Ok(value) => ([(header::CACHE_CONTROL, "no-store")], Json(value)).into_response(),
        Err(_) => (StatusCode::CONFLICT, [(header::CACHE_CONTROL, "no-store")],
            "Dictation is unavailable or expired. Your existing draft is safe; start a new recording.").into_response(),
    }
}
async fn start(
    State(state): State<Arc<SpeechService>>,
    Extension(caller): Extension<GatewayInternalIdentity>,
    Json(request): Json<StartDictation>,
) -> Response {
    response(state.dictations.start(caller, request).await)
}
async fn read(
    State(state): State<Arc<SpeechService>>,
    Extension(caller): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
) -> Response {
    response(state.dictations.read(&caller, id).await)
}
async fn chunk(
    State(state): State<Arc<SpeechService>>,
    Extension(caller): Extension<GatewayInternalIdentity>,
    Path((id, sequence)): Path<(Uuid, u32)>,
    bytes: Bytes,
) -> Response {
    response(
        state
            .dictations
            .chunk(&caller, id, sequence, bytes.to_vec())
            .await,
    )
}
async fn finish(
    State(state): State<Arc<SpeechService>>,
    Extension(caller): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
) -> Response {
    response(state.dictations.finish(&caller, id, false).await)
}
async fn cancel(
    State(state): State<Arc<SpeechService>>,
    Extension(caller): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
) -> Response {
    response(state.dictations.finish(&caller, id, true).await)
}
