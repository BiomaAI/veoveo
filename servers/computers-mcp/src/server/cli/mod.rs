//! Narrow session-bound CLI ingress. Browser cookies and internal identity
//! assertions cannot substitute for a paired Computer credential.
mod facade;
mod pump;
use super::{
    access_events::AccessEvents,
    attachment_authority::{self, Activity, Grant},
    http_error::HttpError,
};
use crate::{Application, ApplicationError};
use axum::{
    Router,
    extract::{
        Path, RawQuery, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, header},
    response::Response,
    routing::get,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers::{
    ComputerError,
    cli_grants::{CliConnectionHandle, CliGrantCredential},
};
use veoveo_computers_runtime::{Binding, LeaseAuthority};
use veoveo_mcp_contract::GatewayProfileId;

#[derive(Clone)]
struct Transport {
    app: Arc<Application>,
    events: Arc<AccessEvents>,
    stop: CancellationToken,
}
pub(super) fn router(
    app: Arc<Application>,
    events: Arc<AccessEvents>,
    stop: CancellationToken,
) -> Router {
    Router::new()
        .route("/{profile}/_ws_tunnel", get(root))
        .route("/{profile}/{id}/_ws_tunnel", get(scoped))
        .with_state(Transport { app, events, stop })
}
fn denied() -> HttpError {
    HttpError(ApplicationError::Domain(ComputerError::Forbidden))
}
fn credential(headers: &HeaderMap, query: Option<&str>) -> Result<CliGrantCredential, HttpError> {
    let value = veoveo_computers_transport::cli_authorization(
        headers,
        veoveo_computers_transport::CliCredentialFraming::Internal,
        query,
    )
    .map_err(|_| denied())?;
    let value = value
        .to_str()
        .map_err(|_| denied())?
        .strip_prefix("Bearer ")
        .ok_or_else(denied)?;
    Ok(CliGrantCredential::new(value.to_owned()))
}
async fn root(
    State(transport): State<Transport>,
    Path(profile): Path<GatewayProfileId>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    upgrade(transport, profile, None, query, headers, ws).await
}
async fn scoped(
    State(transport): State<Transport>,
    Path((profile, id)): Path<(GatewayProfileId, Uuid)>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    upgrade(transport, profile, Some(id), query, headers, ws).await
}
async fn upgrade(
    transport: Transport,
    profile: GatewayProfileId,
    computer: Option<Uuid>,
    query: Option<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    let credential = credential(&headers, query.as_deref())?;
    transport.app.runtime.current()?;
    let slot = transport
        .app
        .terminal_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| HttpError(ApplicationError::Unavailable))?;
    let handle = Arc::new(
        transport
            .app
            .store
            .open_cli_connection(computer, profile, &credential)
            .await
            .map_err(ApplicationError::from)?,
    );
    drop(credential);
    let failed_store = transport.app.store.clone();
    let failed_handle = handle.clone();
    let mut response = ws
        .max_frame_size(65536)
        .max_message_size(65536)
        .write_buffer_size(0)
        .max_write_buffer_size(131072)
        .on_failed_upgrade(move |_| {
            tokio::spawn(async move {
                let _ = failed_store.close_cli_connection(&failed_handle).await;
            });
        })
        .on_upgrade(move |socket| async move {
            let _slot = slot;
            tokio::select! {
                biased;
                _ = transport.stop.cancelled() => {},
                _ = attached(socket, &transport, &handle) => {},
            }
            let _ = transport.app.store.close_cli_connection(&handle).await;
        });
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    Ok(response)
}
async fn attached(
    socket: WebSocket,
    transport: &Transport,
    handle: &CliConnectionHandle,
) -> Result<(), ()> {
    let events = transport.events.listen().await?;
    let baseline = transport
        .app
        .store
        .renew_cli_grant(handle, false)
        .await
        .map_err(|_| ())?;
    let (authority, lease) = LeaseAuthority::issue(
        baseline.checked_at().into(),
        baseline.valid_until() - baseline.checked_at(),
    )
    .map_err(|_| ())?;
    let (activity, updates) = Activity::new();
    let activity = Arc::new(activity);
    let computer = baseline.computer();
    let binding = Binding::from_instance(
        computer.computer_id,
        computer.instance_id(),
        computer.template_fingerprint.clone(),
    )
    .map_err(|_| ())?;
    let family =
        veoveo_platform_store::gateway_refresh_family_record_id(baseline.session_family_id());
    let work = async {
        let runtime = transport.app.runtime.current().map_err(|_| ())?;
        if runtime.provider_instance_id() != computer.provider_instance_id {
            return Err(());
        }
        let access = runtime
            .open_shell_access(&binding, lease.clone())
            .await
            .map_err(|_| ())?;
        if Some(access.sandbox_id()) != computer.provider_resource_id.as_deref()
            || Some(access.main_process_id()) != computer.process_id.as_deref()
        {
            return Err(());
        }
        pump::serve(
            socket,
            facade::Restricted {
                access,
                computer: computer.computer_id,
                activity: activity.clone(),
            },
            lease.clone(),
            updates,
        )
        .await
    };
    tokio::select! {
        biased;
        _ = lease.closed() => {},
        _ = attachment_authority::renew(&transport.app, Grant::Cli(handle), &authority, &activity, events, computer.computer_id, family) => {},
        _ = work => {},
    }
    authority.revoke();
    Ok(())
}
