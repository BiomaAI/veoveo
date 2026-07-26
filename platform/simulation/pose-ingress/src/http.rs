use std::{net::SocketAddr, sync::Arc};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, put},
};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_simulation_pose::{PoseIngressBinding, SessionId};

use crate::state::{PoseIngress, PoseIngressError};

pub(crate) async fn serve(
    address: SocketAddr,
    ingress: Arc<PoseIngress>,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let control = Router::new()
        .route(
            "/bindings/{session_id}",
            put(bind).get(status).delete(revoke),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(
            ingress.clone(),
            authenticate_control,
        ));
    let router = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route(
            "/readyz",
            get({
                let ingress = ingress.clone();
                move || {
                    let readiness = ingress.readiness();
                    async move {
                        let status = if readiness.ready {
                            StatusCode::OK
                        } else {
                            StatusCode::SERVICE_UNAVAILABLE
                        };
                        (status, Json(readiness))
                    }
                }
            }),
        )
        .nest("/v1", control)
        .with_state(ingress)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "Simulation View pose control listening");
    axum::serve(listener, router)
        .with_graceful_shutdown(cancellation.cancelled_owned())
        .await?;
    Ok(())
}

async fn authenticate_control(
    State(ingress): State<Arc<PoseIngress>>,
    request: Request,
    next: Next,
) -> Response {
    let authorized = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
        .is_some_and(|token| ingress.authorize_control(token));
    if authorized {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn bind(
    State(ingress): State<Arc<PoseIngress>>,
    Path(session_id): Path<String>,
    Json(declaration): Json<PoseIngressBinding>,
) -> Response {
    let Ok(session_id) = session_id.parse::<SessionId>() else {
        return (StatusCode::BAD_REQUEST, "invalid session identity").into_response();
    };
    if declaration.session_id != session_id {
        return (StatusCode::BAD_REQUEST, "session identity mismatch").into_response();
    }
    match ingress.bind(declaration).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn status(
    State(ingress): State<Arc<PoseIngress>>,
    Path(session_id): Path<String>,
) -> Response {
    let Ok(session_id) = session_id.parse::<SessionId>() else {
        return (StatusCode::NOT_FOUND, "pose session not found").into_response();
    };
    match ingress.status(&session_id).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn revoke(
    State(ingress): State<Arc<PoseIngress>>,
    Path(session_id): Path<String>,
) -> Response {
    let Ok(session_id) = session_id.parse::<SessionId>() else {
        return (StatusCode::NOT_FOUND, "pose session not found").into_response();
    };
    match ingress.revoke(&session_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error),
    }
}

fn error_response(error: PoseIngressError) -> Response {
    let status = match error {
        PoseIngressError::NotFound => StatusCode::NOT_FOUND,
        PoseIngressError::Capacity => StatusCode::TOO_MANY_REQUESTS,
        PoseIngressError::Producer => StatusCode::FORBIDDEN,
        PoseIngressError::Binding | PoseIngressError::Pose(_) => StatusCode::BAD_REQUEST,
    };
    (status, error.to_string()).into_response()
}

fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}
