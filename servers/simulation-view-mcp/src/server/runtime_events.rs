use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Router,
    extract::{Path, Request, State},
    http::{StatusCode, header::AUTHORIZATION},
    response::{IntoResponse, Response},
    routing::post,
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{reconciler::ReconcilerHandle, state::SimulationViewService};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RuntimeComponent {
    Renderer,
    PoseIngress,
}

impl RuntimeComponent {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "renderer" => Some(Self::Renderer),
            "pose-ingress" => Some(Self::PoseIngress),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct RuntimeEventState {
    service: Arc<SimulationViewService>,
    reconciler: ReconcilerHandle,
    renderer_token_hash: [u8; 32],
    pose_token_hash: [u8; 32],
    generations: Arc<std::sync::Mutex<BTreeMap<RuntimeComponent, Uuid>>>,
}

pub(super) fn router(
    service: Arc<SimulationViewService>,
    reconciler: ReconcilerHandle,
    renderer_token: &str,
    pose_token: &str,
) -> Router {
    Router::new()
        .route("/runtime-events/{component}/{generation}", post(observe))
        .with_state(RuntimeEventState {
            service,
            reconciler,
            renderer_token_hash: Sha256::digest(renderer_token.as_bytes()).into(),
            pose_token_hash: Sha256::digest(pose_token.as_bytes()).into(),
            generations: Arc::new(std::sync::Mutex::new(BTreeMap::new())),
        })
}

async fn observe(
    State(state): State<RuntimeEventState>,
    Path((component, generation)): Path<(String, String)>,
    request: Request,
) -> Response {
    let Some(component) = RuntimeComponent::parse(&component) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(generation) = generation.parse::<Uuid>() else {
        return (StatusCode::BAD_REQUEST, "invalid runtime generation").into_response();
    };
    let expected = match component {
        RuntimeComponent::Renderer => &state.renderer_token_hash,
        RuntimeComponent::PoseIngress => &state.pose_token_hash,
    };
    let authorized = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token)
        .map(|token| <[u8; 32]>::from(Sha256::digest(token.as_bytes())))
        .is_some_and(|supplied| supplied.ct_eq(expected).unwrap_u8() == 1);
    if !authorized {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let changed = {
        let mut generations = state
            .generations
            .lock()
            .expect("runtime generation lock poisoned");
        if generations.get(&component) == Some(&generation) {
            false
        } else {
            generations.insert(component, generation);
            true
        }
    };
    if changed {
        state.service.request_all_runtime_reconciliation();
        state.reconciler.notify();
        tracing::info!(?component, %generation, "Simulation View runtime generation changed");
    }
    StatusCode::NO_CONTENT.into_response()
}

fn bearer_token(value: &str) -> Option<&str> {
    let (scheme, token) = value.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer")
        && !token.is_empty()
        && !token.chars().any(char::is_whitespace))
    .then_some(token)
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;
    use crate::state::SimulationViewConfig;

    #[tokio::test]
    async fn changed_runtime_generation_wakes_once_and_requires_its_token() {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let (handle, mut events) = ReconcilerHandle::channel();
        let renderer_token = "r".repeat(32);
        let pose_token = "p".repeat(32);
        let app = router(service, handle, &renderer_token, &pose_token);
        let generation = Uuid::new_v4();
        let uri = format!("/runtime-events/renderer/{generation}");

        let response = app
            .clone()
            .oneshot(
                Request::post(&uri)
                    .header(AUTHORIZATION, format!("Bearer {renderer_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        tokio::time::timeout(std::time::Duration::from_millis(50), events.changed())
            .await
            .unwrap()
            .unwrap();
        events.borrow_and_update();

        let duplicate = app
            .clone()
            .oneshot(
                Request::post(&uri)
                    .header(AUTHORIZATION, format!("Bearer {renderer_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::NO_CONTENT);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), events.changed())
                .await
                .is_err()
        );

        let unauthorized = app
            .oneshot(
                Request::post(format!("/runtime-events/pose-ingress/{}", Uuid::new_v4()))
                    .header(AUTHORIZATION, format!("Bearer {renderer_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    }
}
