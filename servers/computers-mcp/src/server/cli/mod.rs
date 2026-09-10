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
        .route("/_ws_tunnel", get(root))
        .route("/{id}/_ws_tunnel", get(scoped))
        .with_state(Transport { app, events, stop })
}
fn denied() -> HttpError {
    HttpError(ApplicationError::Domain(ComputerError::Forbidden))
}
fn credential(headers: &HeaderMap, query: Option<&str>) -> Result<CliGrantCredential, HttpError> {
    if query.is_some()
        || headers.contains_key(header::ORIGIN)
        || headers.contains_key(header::COOKIE)
        || headers.contains_key(header::SEC_WEBSOCKET_PROTOCOL)
        || headers.contains_key(header::SEC_WEBSOCKET_EXTENSIONS)
    {
        return Err(denied());
    }
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return Err(denied());
    };
    let token = value
        .to_str()
        .ok()
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| v.len() == 107)
        .ok_or_else(denied)?;
    Ok(CliGrantCredential::new(token.to_owned()))
}
async fn root(
    State(transport): State<Transport>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    upgrade(transport, None, query, headers, ws).await
}
async fn scoped(
    State(transport): State<Transport>,
    Path(id): Path<Uuid>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    upgrade(transport, Some(id), query, headers, ws).await
}
async fn upgrade(
    transport: Transport,
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
            .open_cli_connection(computer, &credential)
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
    let binding = Binding::new(computer.computer_id, computer.template_fingerprint.clone())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_admission_rejects_browser_authority_and_ambiguous_headers() {
        let mut admitted = HeaderMap::new();
        let value = format!("Bearer {}", "x".repeat(107));
        admitted.insert(header::AUTHORIZATION, value.parse().unwrap());
        // The domain validates the opaque credential; ingress validates framing.
        assert!(credential(&admitted, None).is_ok());
        assert!(credential(&HeaderMap::new(), None).is_err());
        assert!(credential(&admitted, Some("")).is_err());
        for name in [
            header::ORIGIN,
            header::COOKIE,
            header::SEC_WEBSOCKET_PROTOCOL,
            header::SEC_WEBSOCKET_EXTENSIONS,
        ] {
            let mut headers = admitted.clone();
            headers.insert(name, "present".parse().unwrap());
            assert!(credential(&headers, None).is_err());
        }
        let mut duplicate = admitted.clone();
        duplicate.append(header::AUTHORIZATION, value.parse().unwrap());
        assert!(credential(&duplicate, None).is_err());
        for value in ["Bearer ordinary-identity-assertion", "Basic credential", ""] {
            let mut headers = admitted.clone();
            headers.insert(header::AUTHORIZATION, value.parse().unwrap());
            assert!(credential(&headers, None).is_err());
        }
    }
}
