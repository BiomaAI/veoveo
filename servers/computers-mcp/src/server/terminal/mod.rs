//! Browser-only attachment. The gateway supplies verified identity and preserves Origin.
mod authority;
mod pump;
use super::{
    BrowserOrigins,
    access_events::AccessEvents,
    http_error::{HttpError, actor},
};
use crate::{Application, ApplicationError};
use axum::{
    Extension, Json, Router,
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{sync::Arc, time::Duration};
use tokio::sync::OwnedSemaphorePermit;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers::{ComputerActor, ComputerError, api::*};
use veoveo_computers_runtime::{Binding, LeaseAuthority, TerminalSize};
use veoveo_mcp_contract::GatewayInternalIdentity;

#[derive(Clone)]
struct Transport {
    app: Arc<Application>,
    origins: BrowserOrigins,
    events: Arc<AccessEvents>,
    stop: CancellationToken,
}
pub(super) fn router(
    app: Arc<Application>,
    origins: BrowserOrigins,
    stop: CancellationToken,
) -> Router {
    let events = AccessEvents::start(app.tasks.platform_store().clone(), stop.clone());
    Router::new()
        .route("/computers/{id}/terminal-ticket", post(ticket))
        .route("/computers/{id}/terminal", get(upgrade))
        .with_state(Transport {
            app,
            origins,
            events,
            stop,
        })
}
fn forbidden() -> HttpError {
    HttpError(ApplicationError::Domain(ComputerError::Forbidden))
}
async fn ticket(
    State(transport): State<Transport>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(_request): Json<TerminalTicketInput>,
) -> Result<Response, HttpError> {
    if !transport.origins.permits(&headers) {
        return Err(forbidden());
    }
    transport.app.runtime.current()?;
    let grant = transport
        .app
        .store
        .issue_browser_grant(&actor(&identity)?, id)
        .await
        .map_err(ApplicationError::from)?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(TerminalTicket {
            computer_id: grant.computer_id,
            token: grant.token,
            expires_at: grant.expires_at,
            endpoint: format!("/computers/admin/computers/{id}/terminal"),
        }),
    )
        .into_response())
}
async fn upgrade(
    State(transport): State<Transport>,
    Extension(identity): Extension<GatewayInternalIdentity>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    if !transport.origins.permits(&headers) {
        return Err(forbidden());
    }
    let actor = actor(&identity)?;
    // Reject unauthorized upgrades before allocating a stream. Redemption repeats authority.
    let control = transport
        .app
        .store
        .control_authority(&actor)
        .await
        .map_err(ApplicationError::from)?;
    control.require_attach(id).map_err(ApplicationError::from)?;
    transport
        .app
        .store
        .get(actor.owner(), id)
        .await
        .map_err(ApplicationError::from)?;
    control.require_attach(id).map_err(ApplicationError::from)?;
    transport.app.runtime.current()?;
    let slot = transport
        .app
        .terminal_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| HttpError(ApplicationError::Unavailable))?;
    Ok(ws
        .max_frame_size(64 * 1024)
        .max_message_size(64 * 1024)
        .write_buffer_size(0)
        .max_write_buffer_size(128 * 1024)
        .on_upgrade(move |socket| run(socket, transport, actor, id, slot)))
}
async fn first(socket: &mut WebSocket, id: Uuid) -> Result<TerminalAttach, ()> {
    let Some(Ok(Message::Text(text))) = socket.recv().await else {
        return Err(());
    };
    if text.len() > 1024 {
        return Err(());
    }
    let attach: TerminalAttach = serde_json::from_str(&text).map_err(|_| ())?;
    if attach.version != TERMINAL_VERSION || attach.computer_id != id || id.is_nil() {
        return Err(());
    }
    TerminalSize::new(attach.cols, attach.rows).map_err(|_| ())?;
    Ok(attach)
}
async fn run(
    mut socket: WebSocket,
    transport: Transport,
    actor: ComputerActor,
    id: Uuid,
    _slot: OwnedSemaphorePermit,
) {
    let attach = tokio::select! {
        biased;
        _ = transport.stop.cancelled() => return,
        result = tokio::time::timeout(Duration::from_secs(5), first(&mut socket, id)) => {
            let Ok(Ok(attach)) = result else { return };
            attach
        }
    };
    let store = &transport.app.store;
    let Ok(handle) = store.redeem_browser_grant(&actor, &attach.token).await else {
        return;
    };
    let size = TerminalSize::new(attach.cols, attach.rows).expect("validated first frame");
    drop(attach);
    // Every successfully redeemed handle is closed after any later setup/transport failure.
    // Cancellation of the process leaves the immutable absolute/idle deadlines in force.
    tokio::select! {
        biased;
        _ = transport.stop.cancelled() => {},
        _ = attached(socket, &transport, id, size, &handle) => {},
    }
    let _ = store.close_browser_grant(&handle).await;
}
async fn attached(
    socket: WebSocket,
    transport: &Transport,
    id: Uuid,
    size: TerminalSize,
    handle: &veoveo_computers::session_grants::SessionGrantHandle,
) -> Result<(), ()> {
    let events = transport.events.listen().await?;
    let baseline = transport
        .app
        .store
        .renew_browser_grant(handle, false)
        .await
        .map_err(|_| ())?;
    if baseline.computer().computer_id != id {
        return Err(());
    }
    let checked = baseline.checked_at().into();
    let (authority, lease) =
        LeaseAuthority::issue(checked, baseline.valid_until() - baseline.checked_at())
            .map_err(|_| ())?;
    let activity = authority::Activity::default();
    let binding =
        Binding::new(id, baseline.computer().template_fingerprint.clone()).map_err(|_| ())?;
    let family =
        veoveo_platform_store::gateway_refresh_family_record_id(baseline.session_family_id());
    let work = async {
        let runtime = transport.app.runtime.current().map_err(|_| ())?;
        if runtime.provider_instance_id() != baseline.computer().provider_instance_id {
            return Err(());
        }
        let terminal = runtime
            .attach(&binding, size, lease.clone())
            .await
            .map_err(|_| ())?;
        if Some(terminal.sandbox_id()) != baseline.computer().provider_resource_id.as_deref()
            || Some(terminal.main_process_instance_id())
                != baseline.computer().process_id.as_deref()
        {
            return Err(());
        }
        pump::run(socket, terminal, &activity).await
    };
    tokio::select! {
        biased;
        _ = lease.closed() => {},
        _ = authority::renew(&transport.app, handle, &authority, &activity, events, id, family) => {},
        _ = work => {},
    }
    authority.revoke();
    Ok(())
}
