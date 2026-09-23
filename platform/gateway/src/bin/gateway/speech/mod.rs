//! Governed native microphone transport. Domain state remains in Speech.
mod authority;

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Extension, MatchedPath, Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, GatewayInternalTokenIssuer, GatewayProfileId, LocalToolName, PolicyTarget,
    ResourceUri, ServerSlug,
};
use veoveo_mcp_gateway::{
    AuthenticatedSubject, GatewayCatalogHandle, GatewayState, GatewayUpstreamHttpClientPool,
};
use veoveo_speech_contract::dictation::{DictationSnapshot, MAX_CHUNK_BYTES, StartDictation};

#[derive(Clone)]
pub(crate) struct SpeechState {
    pub catalog: GatewayCatalogHandle,
    pub gateway_state: GatewayState,
    pub issuer: GatewayInternalTokenIssuer,
    pub upstream: GatewayUpstreamHttpClientPool,
    pub slots: Arc<tokio::sync::Semaphore>,
}
pub(crate) fn router(state: SpeechState) -> Router {
    Router::new()
        .route("/speech/{profile}/dictation", post(proxy))
        .route("/speech/{profile}/dictation/{id}", get(proxy).delete(proxy))
        .route(
            "/speech/{profile}/dictation/{id}/chunks/{sequence}",
            put(proxy),
        )
        .route("/speech/{profile}/dictation/{id}/finish", post(proxy))
        .with_state(state)
}

#[derive(Deserialize)]
struct Parameters {
    profile: GatewayProfileId,
    id: Option<Uuid>,
    sequence: Option<u32>,
}
struct Route {
    profile: GatewayProfileId,
    id: Option<Uuid>,
    path: String,
    tool: Option<&'static str>,
    pcm: bool,
}
impl Route {
    fn authorization(&self) -> (PolicyTarget, &'static [GatewayAction]) {
        let server = ServerSlug::new("speech").expect("static server");
        match self.tool {
            Some(tool) => (
                PolicyTarget::Tool {
                    server,
                    tool: LocalToolName::new(tool).expect("static tool"),
                },
                &[GatewayAction::ToolsCall],
            ),
            None => (
                PolicyTarget::Resource {
                    server,
                    uri: ResourceUri::new(format!(
                        "speech://dictation/{}",
                        self.id.expect("validated read")
                    ))
                    .expect("typed UUID URI"),
                },
                &[GatewayAction::ResourcesRead],
            ),
        }
    }
}

struct Fault(StatusCode);
impl Fault {
    fn unavailable() -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE)
    }
    fn missing() -> Self {
        Self(StatusCode::NOT_FOUND)
    }
    fn denied() -> Self {
        Self(StatusCode::FORBIDDEN)
    }
    fn invalid() -> Self {
        Self(StatusCode::BAD_REQUEST)
    }
}
impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        (
            self.0,
            [(header::CACHE_CONTROL, "no-store")],
            "Speech is unavailable with current access or capacity. Your existing draft is safe.",
        )
            .into_response()
    }
}

async fn proxy(
    State(state): State<SpeechState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Path(params): Path<Parameters>,
    matched: MatchedPath,
    request: Request,
) -> Result<Response, Fault> {
    if request.uri().query().is_some() || params.id.is_some_and(|id| id.is_nil()) {
        return Err(Fault::invalid());
    }
    let _slot = state
        .slots
        .try_acquire()
        .map_err(|_| Fault(StatusCode::TOO_MANY_REQUESTS))?;
    let method = request.method().clone();
    let (path, tool, pcm) = match (params.id, params.sequence) {
        (None, None) => ("/dictation".into(), Some("start_dictation"), false),
        (Some(id), Some(sequence)) => (
            format!("/dictation/{id}/chunks/{sequence}"),
            Some("start_dictation"),
            true,
        ),
        (Some(id), None) if matched.as_str().ends_with("/finish") => (
            format!("/dictation/{id}/finish"),
            Some("finish_dictation"),
            false,
        ),
        (Some(id), None) => (
            format!("/dictation/{id}"),
            if method == axum::http::Method::DELETE {
                Some("cancel_dictation")
            } else {
                None
            },
            false,
        ),
        _ => return Err(Fault::invalid()),
    };
    let route = Route {
        profile: params.profile,
        id: params.id,
        path,
        tool,
        pcm,
    };
    tokio::time::timeout(Duration::from_secs(25), async {
        let admitted = authority::authorize(&state, &route, subject).await?;
        let bytes = to_bytes(request.into_body(), MAX_CHUNK_BYTES)
            .await
            .map_err(|_| Fault(StatusCode::PAYLOAD_TOO_LARGE))?;
        let expected = if let Some(id) = route.id {
            if route.pcm {
                if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
                    return Err(Fault::invalid());
                }
            } else if !bytes.is_empty() {
                return Err(Fault::invalid());
            }
            id
        } else {
            let input: StartDictation =
                serde_json::from_slice(&bytes).map_err(|_| Fault::invalid())?;
            if input.id.is_nil() || !(8000..=48000).contains(&input.sample_rate) {
                return Err(Fault::invalid());
            }
            input.id
        };
        let client = state
            .upstream
            .client(&admitted.catalog, &admitted.manifest)
            .await
            .map_err(|_| Fault::unavailable())?;
        let response = client
            .request(method, admitted.url)
            .header(header::AUTHORIZATION, admitted.authorization)
            .header(
                header::CONTENT_TYPE,
                if route.pcm {
                    "application/octet-stream"
                } else {
                    "application/json"
                },
            )
            .body(bytes)
            .send()
            .await
            .map_err(|_| Fault::unavailable())?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(Fault(if status.is_client_error() {
                status
            } else {
                StatusCode::BAD_GATEWAY
            }));
        }
        let bytes = to_bytes(Body::from_stream(response.bytes_stream()), 4 * 1024 * 1024)
            .await
            .map_err(|_| Fault::unavailable())?;
        let snapshot: DictationSnapshot =
            serde_json::from_slice(&bytes).map_err(|_| Fault::unavailable())?;
        if snapshot.id != expected {
            return Err(Fault::unavailable());
        }
        if let Some(transcript) = &snapshot.transcript {
            transcript.validate(120).map_err(|_| Fault::unavailable())?;
        }
        Ok(([(header::CACHE_CONTROL, "no-store")], Json(snapshot)).into_response())
    })
    .await
    .map_err(|_| Fault::unavailable())?
}
