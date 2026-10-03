//! The single entry point that hosts an MCP server behind the Veoveo gateway.
//!
//! [`HostedServer::for_domain`] starts from the server's [`DomainServer`] type,
//! which names its checked setup. The builder only offers `build` once the three
//! required inputs are present, so a server cannot start without them:
//!
//! - [`deployment`](HostedServerBuilder::deployment): the public deployment, which
//!   fixes the mount path and the allowed hosts;
//! - [`internal_trust`](HostedServerBuilder::internal_trust): the gateway trust
//!   bundle, bound to this server's slug as the token audience;
//! - [`handler`](HostedServerBuilder::handler): the factory for that domain's
//!   [`Hosted`] handler. A hand-written `ServerHandler`, or a domain other than the
//!   one the builder started from, does not compile.
//!
//! The built server always has the same shape:
//!
//! | Route | Authentication | Purpose |
//! |---|---|---|
//! | `{mount}/healthz` | none | liveness |
//! | `{mount}/readyz` | none | readiness, when a check is configured |
//! | `{mount}/admin/docs/llms.txt`, `{mount}/admin/docs/{doc_id}` | gateway | embedded documents (C20, C21) |
//! | `{mount}/mcp` | gateway | stateless Streamable HTTP with the 8 MiB response budget |
//! | additional authenticated routes | gateway | server-specific HTTP, such as playback |
//!
//! Every request passes host validation, then tracing. Shutdown follows SIGTERM or
//! Ctrl-C and cancels in-flight MCP work.
//!
//! The handler is the domain's own [`Hosted`] value:
//!
//! ```
//! # use veoveo_mcp_contract::hosting::{DomainServer, Hosted, HostedServer};
//! # fn check<D: DomainServer + Clone>(domain: D) {
//! let _ = HostedServer::for_domain::<D>().handler(move || Hosted::new(domain.clone()));
//! # }
//! ```
//!
//! A server without a deployment, trust bundle, and handler has no `build`:
//!
//! ```compile_fail
//! # use veoveo_mcp_contract::hosting::{DomainServer, HostedServer};
//! # fn check<D: DomainServer>() {
//! let _ = HostedServer::for_domain::<D>().build();
//! # }
//! ```
//!
//! A hand-written `ServerHandler` cannot replace the host:
//!
//! ```compile_fail
//! # use veoveo_mcp_contract::hosting::{DomainServer, HostedServer};
//! struct Handwritten;
//! impl rmcp::ServerHandler for Handwritten {}
//! # fn check<D: DomainServer>() {
//! let _ = HostedServer::for_domain::<D>().handler(|| Handwritten);
//! # }
//! ```
//!
//! Nor can another domain's handler:
//!
//! ```compile_fail
//! # use veoveo_mcp_contract::hosting::{DomainServer, Hosted, HostedServer};
//! # fn check<D: DomainServer, Other: DomainServer + Clone>(other: Other) {
//! let _ = HostedServer::for_domain::<D>().handler(move || Hosted::new(other.clone()));
//! # }
//! ```

use std::{future::Future, net::SocketAddr, sync::Arc};

use axum::{Router, http::StatusCode, middleware, response::IntoResponse, routing::get};
use rmcp::transport::streamable_http_server::StreamableHttpService;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};

use crate::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    PublicDeployment, ServerPublicEndpoint, ServerSlug, TokenIssuer,
    canonical_streamable_http_server_config, enforce_serialized_mcp_response, public_allowed_hosts,
    server_contract::McpServerContract, stateless_session_manager,
};

use super::{
    DomainServer, Hosted, TaskSupport, admin,
    auth::{self, InternalAuth},
    host::{self, AllowedHosts},
};

/// A required builder input that has not been supplied yet.
pub struct Missing;

/// A required builder input that has been supplied.
pub struct Provided<T>(T);

/// The deployment inputs: the server's public endpoint and its allowed hosts.
pub struct Deployment {
    endpoint: ServerPublicEndpoint,
    allowed_hosts: Vec<String>,
}

type Readiness = Arc<dyn Fn() -> futures::future::BoxFuture<'static, bool> + Send + Sync>;

/// Builds a [`HostedServer`] for the domain `D`. See the [module
/// documentation](crate::hosting).
pub struct HostedServerBuilder<D: DomainServer, Dep, Trust, H> {
    domain: std::marker::PhantomData<fn() -> D>,
    deployment: Dep,
    trust: Trust,
    handler: H,
    extra_hosts: Vec<String>,
    authenticated_routes: Router,
    public_routes: Router,
    readiness: Option<Readiness>,
}

/// A hosted MCP server, ready to serve.
pub struct HostedServer {
    pub(super) router: Router,
    pub(super) slug: ServerSlug,
    pub(super) endpoint: ServerPublicEndpoint,
    cancel: CancellationToken,
}

impl HostedServer {
    /// Starts a builder for the domain `D`, whose checked setup fixes the slug,
    /// documents, and discovery.
    pub fn for_domain<D: DomainServer>() -> HostedServerBuilder<D, Missing, Missing, Missing> {
        HostedServerBuilder {
            domain: std::marker::PhantomData,
            deployment: Missing,
            trust: Missing,
            handler: Missing,
            extra_hosts: Vec::new(),
            authenticated_routes: Router::new(),
            public_routes: Router::new(),
            readiness: None,
        }
    }

    /// The complete router, for in-process tests.
    pub fn into_router(self) -> Router {
        self.router
    }

    /// Cancels in-flight MCP work when triggered; `serve` triggers it on shutdown.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Serves until SIGTERM or Ctrl-C.
    pub async fn serve(self, address: SocketAddr) -> anyhow::Result<()> {
        self.serve_with_shutdown(address, shutdown_signal()).await
    }

    /// Serves until `shutdown` completes.
    pub async fn serve_with_shutdown(
        self,
        address: SocketAddr,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let listener = tokio::net::TcpListener::bind(address).await?;
        tracing::info!(
            server = self.slug.as_str(),
            address = %address,
            mcp_path = self.endpoint.path("mcp"),
            public_url = self.endpoint.public_url(),
            "listening"
        );
        let cancel = self.cancel.clone();
        axum::serve(listener, self.router)
            .with_graceful_shutdown(async move {
                shutdown.await;
                cancel.cancel();
            })
            .await?;
        Ok(())
    }
}

async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "SIGTERM handler unavailable");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}

impl<D: DomainServer, Dep, Trust, H> HostedServerBuilder<D, Dep, Trust, H> {
    /// Sets the public deployment. The endpoint for this server's slug fixes the
    /// mount path; the deployment's host, plus loopback when allowed, forms the
    /// allowed hosts.
    pub fn deployment(
        self,
        deployment: &PublicDeployment,
        allow_loopback_hosts: bool,
    ) -> anyhow::Result<HostedServerBuilder<D, Provided<Deployment>, Trust, H>> {
        let endpoint = deployment.server(D::Contract::slug().as_str())?;
        let allowed_hosts = public_allowed_hosts(deployment, allow_loopback_hosts);
        Ok(HostedServerBuilder {
            domain: self.domain,
            deployment: Provided(Deployment {
                endpoint,
                allowed_hosts,
            }),
            trust: self.trust,
            handler: self.handler,
            extra_hosts: self.extra_hosts,
            authenticated_routes: self.authenticated_routes,
            public_routes: self.public_routes,
            readiness: self.readiness,
        })
    }

    /// Admits the gateway's internal tokens addressed to this server's slug.
    pub fn internal_trust(
        self,
        trust: GatewayInternalTrustBundle,
    ) -> anyhow::Result<HostedServerBuilder<D, Dep, Provided<Arc<GatewayInternalTokenVerifier>>, H>>
    {
        let verifier = GatewayInternalTokenVerifier::new(
            TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
            D::Contract::slug(),
            trust,
        );
        Ok(HostedServerBuilder {
            domain: self.domain,
            deployment: self.deployment,
            trust: Provided(Arc::new(verifier)),
            handler: self.handler,
            extra_hosts: self.extra_hosts,
            authenticated_routes: self.authenticated_routes,
            public_routes: self.public_routes,
            readiness: self.readiness,
        })
    }

    /// Sets the factory for this domain's [`Hosted`] handler, with its task
    /// support. The transport calls it for each stateless request.
    pub fn handler<F, T>(self, factory: F) -> HostedServerBuilder<D, Dep, Trust, Provided<F>>
    where
        F: Fn() -> Hosted<D, T> + Send + Sync + 'static,
        T: TaskSupport,
    {
        HostedServerBuilder {
            domain: self.domain,
            deployment: self.deployment,
            trust: self.trust,
            handler: Provided(factory),
            extra_hosts: self.extra_hosts,
            authenticated_routes: self.authenticated_routes,
            public_routes: self.public_routes,
            readiness: self.readiness,
        }
    }

    /// Adds allowed host authorities beyond the deployment's own.
    pub fn allowed_hosts(mut self, hosts: impl IntoIterator<Item = String>) -> Self {
        self.extra_hosts.extend(hosts);
        self
    }

    /// Adds server-specific routes under the mount, behind gateway authentication.
    pub fn authenticated_routes(mut self, routes: Router) -> Self {
        self.authenticated_routes = self.authenticated_routes.merge(routes);
        self
    }

    /// Adds server-specific routes under the mount without gateway
    /// authentication. Use only for endpoints that verify their own callers, such
    /// as signed provider webhooks, or that serve no caller data.
    pub fn public_routes(mut self, routes: Router) -> Self {
        self.public_routes = self.public_routes.merge(routes);
        self
    }

    /// Serves `{mount}/readyz`: 200 while `check` reports ready, 503 otherwise.
    pub fn readiness<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        self.readiness = Some(Arc::new(move || Box::pin(check())));
        self
    }
}

impl<D, F, T>
    HostedServerBuilder<
        D,
        Provided<Deployment>,
        Provided<Arc<GatewayInternalTokenVerifier>>,
        Provided<F>,
    >
where
    D: DomainServer,
    F: Fn() -> Hosted<D, T> + Send + Sync + 'static,
    T: TaskSupport,
{
    /// Assembles the router. Every hosted server has the same routes and layers.
    pub fn build(self) -> HostedServer {
        let slug = D::Contract::slug();
        let Provided(Deployment {
            endpoint,
            mut allowed_hosts,
        }) = self.deployment;
        allowed_hosts.extend(self.extra_hosts);
        let allowed_hosts = Arc::new(allowed_hosts);
        let Provided(verifier) = self.trust;
        let Provided(factory) = self.handler;
        let cancel = CancellationToken::new();

        let mcp_service = StreamableHttpService::new(
            move || Ok(factory()),
            stateless_session_manager(),
            canonical_streamable_http_server_config()
                .with_allowed_hosts(allowed_hosts.iter().cloned())
                .with_cancellation_token(cancel.child_token()),
        );
        let auth_state = InternalAuth {
            verifier,
            slug: slug.clone(),
        };
        let authenticate =
            || middleware::from_fn_with_state(auth_state.clone(), auth::authenticate);

        let mcp = Router::new()
            .route_service("/", mcp_service.clone())
            .route_service("/{*path}", mcp_service)
            .layer(middleware::from_fn(enforce_serialized_mcp_response))
            .layer(authenticate());
        let admin = admin::docs_router(D::setup().documents()).layer(authenticate());
        let authenticated = self.authenticated_routes.layer(authenticate());

        let mut server = Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .nest("/admin", admin)
            .nest("/mcp", mcp)
            .merge(authenticated)
            .merge(self.public_routes);
        if let Some(readiness) = self.readiness {
            server = server.route(
                "/readyz",
                get(move || {
                    let readiness = readiness.clone();
                    async move {
                        if readiness().await {
                            (StatusCode::OK, "ready").into_response()
                        } else {
                            (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
                        }
                    }
                }),
            );
        }
        let router = Router::new()
            .nest(endpoint.mount_path(), server)
            .layer(middleware::from_fn_with_state(
                AllowedHosts {
                    hosts: allowed_hosts,
                    slug: slug.clone(),
                },
                host::validate_host,
            ))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
            );
        HostedServer {
            router,
            slug,
            endpoint,
            cancel,
        }
    }
}
