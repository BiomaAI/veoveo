mod admin;
mod attachment_authority;
mod automation;
mod cli;
mod files;
mod http_error;
mod maintenance;
mod origins;
mod pairing;
mod provider;
mod run;
mod terminal;
use crate::{
    Application, ApplicationError,
    protocol::{ComputerTasks, ComputersMcp},
};
use axum::{Router, extract::DefaultBodyLimit};
pub use origins::BrowserOrigins;
pub use run::serve;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle,
    hosting::{Hosted, HostedServer},
    parse_allowed_host_authority,
};

/// One hosted server under `/computers`, shared by the installation binary and
/// wire fixtures. Health, readiness and the CLI routes, which verify their own
/// grants, carry no gateway authentication. MCP and admin routes require an
/// assertion signed for the Computers audience by the installation gateway.
pub fn hosted(
    app: Arc<Application>,
    trust: GatewayInternalTrustBundle,
    allowed_hosts: Vec<String>,
    allowed_origins: BrowserOrigins,
    shutdown: CancellationToken,
) -> anyhow::Result<HostedServer> {
    if allowed_hosts.is_empty()
        || allowed_hosts
            .iter()
            .any(|h| parse_allowed_host_authority(h).is_none())
    {
        return Err(ApplicationError::Configuration.into());
    }
    let domain = ComputersMcp::new(app.clone());
    let admin = admin::router(app.clone())
        .merge(files::router(app.clone()))
        .merge(maintenance::router(app.clone()))
        .merge(automation::router(app.clone()))
        .merge(pairing::router(app.clone(), allowed_origins.clone()))
        .merge(terminal::router(
            app.clone(),
            allowed_origins,
            shutdown.clone(),
        ))
        .layer(DefaultBodyLimit::max(64 * 1024));
    let store = app.tasks.platform_store().clone();
    Ok(HostedServer::for_domain::<ComputersMcp>()
        .internal(allowed_hosts)
        .internal_trust(trust)?
        .handler(move || Hosted::new(domain.clone()).with_tasks(ComputerTasks::new(domain.clone())))
        .admin_routes(admin)
        .public_routes(Router::new().nest("/cli", cli::router(app, shutdown)))
        .readiness(move || {
            let store = store.clone();
            async move {
                matches!(
                    tokio::time::timeout(Duration::from_secs(5), store.healthcheck()).await,
                    Ok(Ok(()))
                )
            }
        })
        .mcp_request_limit(2 * 1024 * 1024)
        // Bounds admission and database work; a subscription stream continues.
        .request_timeout(Duration::from_secs(30))
        .build())
}
