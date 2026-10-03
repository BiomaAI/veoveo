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
//! | `{mount}/healthz` | none | liveness, from a check when one is configured |
//! | `{mount}/readyz` | none | readiness, when a check is configured |
//! | `{mount}/admin/docs/llms.txt`, `{mount}/admin/docs/{doc_id}` | gateway | embedded documents (C20, C21) |
//! | `{mount}/mcp` | gateway | stateless Streamable HTTP with the 8 MiB response budget |
//! | additional authenticated routes | gateway | server-specific HTTP, such as playback |
//!
//! `{mount}` is the public endpoint's path from [`deployment`], `/{slug}` for an
//! [`internal`] server, or empty for an [`internal_root`] server.
//! [`mcp_request_limit`] and [`request_timeout`] bound the MCP request size and
//! the time an authenticated request may take to start its response.
//!
//! [`deployment`]: HostedServerBuilder::deployment
//! [`internal`]: HostedServerBuilder::internal
//! [`internal_root`]: HostedServerBuilder::internal_root
//! [`mcp_request_limit`]: HostedServerBuilder::mcp_request_limit
//! [`request_timeout`]: HostedServerBuilder::request_timeout
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

use std::{future::Future, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
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

/// The deployment inputs: where the server is mounted and its allowed hosts.
pub struct Deployment {
    mount: Mount,
    allowed_hosts: Vec<String>,
}

/// Where a hosted server's routes are mounted.
#[derive(Clone)]
pub(super) enum Mount {
    /// Under the public endpoint for the server's slug, such as `/frames`.
    Public(ServerPublicEndpoint),
    /// Under `/{slug}` on a cluster-internal listener.
    Internal(String),
    /// At the root of a cluster-internal listener.
    InternalRoot,
}

impl Mount {
    /// The path prefix of every route: the mount path, or empty at the root.
    pub(super) fn prefix(&self) -> &str {
        match self {
            Self::Public(endpoint) => endpoint.mount_path(),
            Self::Internal(path) => path,
            Self::InternalRoot => "",
        }
    }
}

type Probe = Arc<dyn Fn() -> futures::future::BoxFuture<'static, bool> + Send + Sync>;

/// The optional builder inputs, carried unchanged through the required ones.
struct Options {
    extra_hosts: Vec<String>,
    authenticated_routes: Router,
    admin_routes: Router,
    public_routes: Router,
    liveness: Option<Probe>,
    readiness: Option<Probe>,
    mcp_request_limit: Option<usize>,
    request_timeout: Option<Duration>,
}

/// Builds a [`HostedServer`] for the domain `D`. See the [module
/// documentation](crate::hosting).
pub struct HostedServerBuilder<D: DomainServer, Dep, Trust, H> {
    domain: std::marker::PhantomData<fn() -> D>,
    deployment: Dep,
    trust: Trust,
    handler: H,
    options: Options,
}

/// A hosted MCP server, ready to serve.
pub struct HostedServer {
    pub(super) router: Router,
    pub(super) slug: ServerSlug,
    pub(super) mount: Mount,
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
            options: Options {
                extra_hosts: Vec::new(),
                authenticated_routes: Router::new(),
                admin_routes: Router::new(),
                public_routes: Router::new(),
                liveness: None,
                readiness: None,
                mcp_request_limit: None,
                request_timeout: None,
            },
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
        match &self.mount {
            Mount::Public(endpoint) => tracing::info!(
                server = self.slug.as_str(),
                address = %address,
                mcp_path = endpoint.path("mcp"),
                public_url = endpoint.public_url(),
                "listening"
            ),
            Mount::Internal(_) | Mount::InternalRoot => tracing::info!(
                server = self.slug.as_str(),
                address = %address,
                mcp_path = format!("{}/mcp", self.mount.prefix()),
                "listening on a cluster-internal mount"
            ),
        }
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
    fn with_deployment(
        self,
        mount: Mount,
        allowed_hosts: Vec<String>,
    ) -> HostedServerBuilder<D, Provided<Deployment>, Trust, H> {
        HostedServerBuilder {
            domain: self.domain,
            deployment: Provided(Deployment {
                mount,
                allowed_hosts,
            }),
            trust: self.trust,
            handler: self.handler,
            options: self.options,
        }
    }

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
        Ok(self.with_deployment(Mount::Public(endpoint), allowed_hosts))
    }

    /// Serves under `/{slug}` on a cluster-internal listener, for a server the
    /// gateway reaches at an internal authority. Only `allowed_hosts` pass host
    /// validation.
    pub fn internal(
        self,
        allowed_hosts: impl IntoIterator<Item = String>,
    ) -> HostedServerBuilder<D, Provided<Deployment>, Trust, H> {
        let mount = Mount::Internal(format!("/{}", D::Contract::slug().as_str()));
        self.with_deployment(mount, allowed_hosts.into_iter().collect())
    }

    /// Serves at the root of a cluster-internal listener, for a server whose
    /// gateway upstream and protocol paths are root-relative. Only
    /// `allowed_hosts`, the internal authorities clients use, pass host validation.
    pub fn internal_root(
        self,
        allowed_hosts: impl IntoIterator<Item = String>,
    ) -> HostedServerBuilder<D, Provided<Deployment>, Trust, H> {
        self.with_deployment(Mount::InternalRoot, allowed_hosts.into_iter().collect())
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
            options: self.options,
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
            options: self.options,
        }
    }

    /// Adds allowed host authorities beyond the deployment's own.
    pub fn allowed_hosts(mut self, hosts: impl IntoIterator<Item = String>) -> Self {
        self.options.extra_hosts.extend(hosts);
        self
    }

    /// Adds server-specific routes under the mount, behind gateway authentication.
    pub fn authenticated_routes(mut self, routes: Router) -> Self {
        self.options.authenticated_routes = self.options.authenticated_routes.merge(routes);
        self
    }

    /// Adds administrative routes under `{mount}/admin`, beside the document
    /// routes and behind gateway authentication. Layer any additional
    /// authorization, such as an administrative scope, onto `routes`.
    pub fn admin_routes(mut self, routes: Router) -> Self {
        self.options.admin_routes = self.options.admin_routes.merge(routes);
        self
    }

    /// Adds server-specific routes under the mount without gateway
    /// authentication. Use only for endpoints that verify their own callers, such
    /// as signed provider webhooks, or that serve no caller data.
    pub fn public_routes(mut self, routes: Router) -> Self {
        self.options.public_routes = self.options.public_routes.merge(routes);
        self
    }

    /// Makes `{mount}/healthz` report `check`: 200 while it reports alive, 503
    /// otherwise. Use it only for a failure a restart repairs, such as a dead
    /// worker process; without a check, `healthz` reports the process alive.
    pub fn liveness<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        self.options.liveness = Some(Arc::new(move || Box::pin(check())));
        self
    }

    /// Serves `{mount}/readyz`: 200 while `check` reports ready, 503 otherwise.
    pub fn readiness<F, Fut>(mut self, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        self.options.readiness = Some(Arc::new(move || Box::pin(check())));
        self
    }

    /// Rejects an MCP request body larger than `bytes` with 413. Without a limit,
    /// the transport admits 4 MiB.
    pub fn mcp_request_limit(mut self, bytes: usize) -> Self {
        self.options.mcp_request_limit = Some(bytes);
        self
    }

    /// Answers 504 when an authenticated request has not started its response
    /// within `timeout`. A streamed body, such as a subscription, continues past
    /// it once its response has started.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.options.request_timeout = Some(timeout);
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
            mount,
            mut allowed_hosts,
        }) = self.deployment;
        let options = self.options;
        allowed_hosts.extend(options.extra_hosts);
        let allowed_hosts = Arc::new(allowed_hosts);
        let Provided(verifier) = self.trust;
        let Provided(factory) = self.handler;
        let cancel = CancellationToken::new();

        let factory = Arc::new(factory);
        let authorize: admin::DocumentAuthorizer = {
            let factory = factory.clone();
            Arc::new(move |identity, address| {
                let hosted = factory();
                Box::pin(async move {
                    hosted
                        .domain()
                        .authorize_documents(&identity, &address)
                        .await
                })
            })
        };
        let mut transport = canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancel.child_token());
        if let Some(bytes) = options.mcp_request_limit {
            transport = transport.with_max_request_body_bytes(bytes);
        }
        let mcp_service = StreamableHttpService::new(
            move || Ok(factory()),
            stateless_session_manager(),
            transport,
        );
        let auth_state = InternalAuth {
            verifier,
            slug: slug.clone(),
        };
        let timeout = options.request_timeout;
        let authenticate = |routes: Router| {
            let routes = match timeout {
                Some(timeout) => routes.layer(middleware::from_fn_with_state(timeout, bounded)),
                None => routes,
            };
            routes.layer(middleware::from_fn_with_state(
                auth_state.clone(),
                auth::authenticate,
            ))
        };

        let mcp = authenticate(
            Router::new()
                .route_service("/", mcp_service.clone())
                .route_service("/{*path}", mcp_service)
                .layer(middleware::from_fn(enforce_serialized_mcp_response)),
        );
        let admin = authenticate(
            admin::docs_router(D::setup().documents(), D::Contract::scheme(), authorize)
                .merge(options.admin_routes),
        );
        let authenticated = authenticate(options.authenticated_routes);

        let mut server = Router::new()
            .route("/healthz", get(probe(options.liveness, "ok", "not alive")))
            .nest("/admin", admin)
            .nest("/mcp", mcp)
            .merge(authenticated)
            .merge(options.public_routes);
        if let Some(readiness) = options.readiness {
            server = server.route("/readyz", get(probe(Some(readiness), "ready", "not ready")));
        }
        let router = match &mount {
            Mount::InternalRoot => server,
            mount => Router::new().nest(mount.prefix(), server),
        };
        let router = router
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
            mount,
            cancel,
        }
    }
}

/// Answers 504 when the response has not started within `timeout`.
async fn bounded(State(timeout): State<Duration>, request: Request, next: Next) -> Response {
    tokio::time::timeout(timeout, next.run(request))
        .await
        .unwrap_or_else(|_| {
            (
                StatusCode::GATEWAY_TIMEOUT,
                "request timed out before its response started",
            )
                .into_response()
        })
}

/// A probe handler: `pass` with 200 while `check` holds, or without a check.
fn probe(
    check: Option<Probe>,
    pass: &'static str,
    fail: &'static str,
) -> impl Fn() -> futures::future::BoxFuture<'static, axum::response::Response> + Clone + Send + 'static
{
    move || {
        let check = check.clone();
        Box::pin(async move {
            match check {
                Some(check) if !check().await => {
                    (StatusCode::SERVICE_UNAVAILABLE, fail).into_response()
                }
                _ => (StatusCode::OK, pass).into_response(),
            }
        })
    }
}
