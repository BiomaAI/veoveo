mod presentation;

use crate::runtime::SharedCatalog;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
pub(crate) use presentation::{console_display_name, presentation};
use veoveo_mcp_contract::{
    ConsoleBootstrap, GatewayAction, GatewayProfileId, PolicyEffect, PolicyTarget, TraceId,
};
use veoveo_mcp_gateway::{AuthenticatedSubject, PolicyRequest};

#[derive(Clone)]
pub(crate) struct ConsoleState {
    pub catalog: SharedCatalog,
    pub offline_mode: bool,
}

/// Authentication supplies current identity and Work Context before this handler.
/// The permission flag is presentation data; each administrative request authorizes again.
pub(crate) async fn bootstrap(
    State(state): State<ConsoleState>,
    Path(profile): Path<GatewayProfileId>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let catalog = state.catalog.current();
    if catalog.profile(&profile).is_none() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let trace_id = TraceId::new(uuid::Uuid::now_v7().to_string()).expect("UUID trace");
    let can_read_installation = catalog
        .decide(PolicyRequest {
            principal: &subject.principal,
            profile: &profile,
            action: GatewayAction::AdminRead,
            target: &PolicyTarget::Gateway,
            trace_id: &trace_id,
        })
        .effect
        == PolicyEffect::Allow;
    let (installation, session) =
        match presentation(catalog.control_plane(), &subject, state.offline_mode) {
            Ok(value) => value,
            Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
        };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(ConsoleBootstrap {
            profile,
            installation,
            session,
            can_read_installation,
        }),
    )
        .into_response()
}
