mod access_events;
mod admin;
mod attachment_authority;
pub(crate) mod auth;
mod automation;
mod cli;
mod http_error;
mod origins;
mod pairing;
mod provider;
mod run;
mod terminal;
use crate::{Application, ApplicationError, protocol::ComputersMcp};
use axum::{Router, extract::DefaultBodyLimit, http::StatusCode, middleware, routing::get};
pub use origins::BrowserOrigins;
use rmcp::transport::streamable_http_server::StreamableHttpService;
pub use run::serve;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalTokenVerifier, parse_allowed_host_authority};

/// One canonical HTTP mount, shared by the installation binary and wire fixtures.
/// Only the health probe is anonymous. Domain and docs routes require an assertion
/// signed for the Computers audience by the installation gateway.
pub fn router(
    app: Arc<Application>,
    verifier: GatewayInternalTokenVerifier,
    allowed_hosts: Vec<String>,
    allowed_origins: BrowserOrigins,
    shutdown: CancellationToken,
) -> Result<Router, ApplicationError> {
    if allowed_hosts.is_empty()
        || allowed_hosts
            .iter()
            .any(|h| parse_allowed_host_authority(h).is_none())
    {
        return Err(ApplicationError::Configuration);
    }
    let service = StreamableHttpService::new(
        {
            let app = app.clone();
            move || Ok(ComputersMcp::new(app.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_max_request_body_bytes(2 * 1024 * 1024)
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(shutdown.clone()),
    );
    let mcp = Router::new()
        .route_service("/", service.clone())
        .route_service("/{*path}", service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ));
    let events =
        access_events::AccessEvents::start(app.tasks.platform_store().clone(), shutdown.clone());
    let cli = cli::router(app.clone(), events.clone(), shutdown.clone());
    let secured = Router::new()
        .nest("/mcp", mcp)
        .nest(
            "/admin",
            admin::router(app.clone())
                .merge(automation::router(app.clone()))
                .merge(pairing::router(app.clone(), allowed_origins.clone()))
                .merge(terminal::router(
                    app.clone(),
                    allowed_origins,
                    shutdown,
                    events,
                )),
        )
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn(auth::deadline))
        .layer(middleware::from_fn_with_state(verifier, auth::internal));
    let health = get(move || {
        let app = app.clone();
        async move {
            if matches!(
                tokio::time::timeout(
                    Duration::from_secs(5),
                    app.tasks.platform_store().healthcheck()
                )
                .await,
                Ok(Ok(()))
            ) {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        }
    });
    Ok(Router::new()
        .nest(
            "/computers",
            secured.route("/healthz", health).nest("/cli", cli),
        )
        .layer(middleware::from_fn_with_state(
            Arc::new(allowed_hosts),
            auth::host,
        )))
}
