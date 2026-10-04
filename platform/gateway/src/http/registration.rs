use super::{
    GatewayHttpContext, ModuleCleanupSupervisor, ModuleTaskScope, lifecycle::CleanupGuard,
};
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use futures::{FutureExt, future::BoxFuture};
use serde::Serialize;
use std::collections::BTreeMap;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
use veoveo_modules::ModuleName;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleBindingState {
    Bound,
    Unbound,
}
#[derive(Debug, Clone, Serialize)]
pub struct ModuleBindingSnapshot {
    pub module: ModuleName,
    pub state: ModuleBindingState,
    pub required: bool,
}

pub struct GatewayModuleRoutes {
    pub profile_authenticated: Router,
    pub owner_authenticated: Router,
}
impl GatewayModuleRoutes {
    pub fn new(profile_authenticated: Router, owner_authenticated: Router) -> Self {
        Self {
            profile_authenticated,
            owner_authenticated,
        }
    }
}
type Factory = Box<
    dyn FnOnce(
            GatewayHttpContext,
            ModuleTaskScope,
        ) -> BoxFuture<'static, anyhow::Result<GatewayModuleRoutes>>
        + Send,
>;
#[derive(Default)]
pub struct GatewayModules {
    factories: BTreeMap<ModuleName, Option<Factory>>,
    supervisor: ModuleCleanupSupervisor,
}
impl GatewayModules {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn supervisor(&self) -> ModuleCleanupSupervisor {
        self.supervisor.clone()
    }
    pub fn register<F>(&mut self, module: ModuleName, factory: F) -> anyhow::Result<()>
    where
        F: FnOnce(
                GatewayHttpContext,
                ModuleTaskScope,
            ) -> BoxFuture<'static, anyhow::Result<GatewayModuleRoutes>>
            + Send
            + 'static,
    {
        self.insert(module, Some(Box::new(factory)))
    }
    pub fn declare_unbound(&mut self, module: ModuleName) -> anyhow::Result<()> {
        self.insert(module, None)
    }
    fn insert(&mut self, module: ModuleName, factory: Option<Factory>) -> anyhow::Result<()> {
        if self.factories.contains_key(&module) {
            anyhow::bail!("duplicate gateway module binding `{module}`");
        }
        self.factories.insert(module, factory);
        Ok(())
    }
    pub async fn build(
        self,
        context: GatewayHttpContext,
        required: &[ModuleName],
    ) -> anyhow::Result<BuiltGatewayModules> {
        for module in required {
            if !self.factories.get(module).is_some_and(Option::is_some) {
                anyhow::bail!("required gateway module `{module}` is unbound");
            }
        }
        let mut guard = CleanupGuard::new(self.supervisor)?;
        let mut router = Router::new();
        let mut bindings = vec![];
        for (module, factory) in self.factories {
            bindings.push(ModuleBindingSnapshot {
                required: required.contains(&module),
                state: if factory.is_some() {
                    ModuleBindingState::Bound
                } else {
                    ModuleBindingState::Unbound
                },
                module: module.clone(),
            });
            let Some(factory) = factory else {
                continue;
            };
            let scope = ModuleTaskScope::new();
            guard.scopes.push(scope.clone());
            let factory_context = context.clone();
            let factory_scope = scope.clone();
            let result = scope
                .spawn(async move { factory(factory_context, factory_scope).await })?
                .await;
            let routes = match result {
                Ok(Ok(routes)) => routes,
                Ok(Err(error)) => {
                    let cleanup = guard.finish().await;
                    anyhow::bail!(
                        "gateway module `{module}` startup failed: {error:#}; cleanup: {cleanup:?}"
                    );
                }
                Err(error) => {
                    let cleanup = guard.finish().await;
                    anyhow::bail!(
                        "gateway module `{module}` factory task failed: {error}; cleanup: {cleanup:?}"
                    );
                }
            };
            let merge = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let routes = routes
                    .profile_authenticated
                    .layer(middleware::from_fn_with_state(
                        super::ProfileAuthState::from(&context),
                        super::authenticate_profile,
                    ))
                    .merge(routes.owner_authenticated);
                router
                    .clone()
                    .merge(routes.layer(middleware::from_fn_with_state(scope, admit_request)))
            }));
            router = match merge {
                Ok(router) => router,
                Err(_) => {
                    let cleanup = guard.finish().await;
                    anyhow::bail!(
                        "gateway module `{module}` route declaration conflicts; cleanup: {cleanup:?}"
                    );
                }
            };
        }
        Ok(BuiltGatewayModules {
            router,
            bindings,
            guard,
        })
    }
}
pub struct BuiltGatewayModules {
    router: Router,
    bindings: Vec<ModuleBindingSnapshot>,
    guard: CleanupGuard,
}
impl BuiltGatewayModules {
    pub fn router(&self) -> Router {
        self.router.clone()
    }
    pub fn shutdown_signal(&self) -> super::ModuleShutdownSignal {
        super::ModuleShutdownSignal {
            scopes: self.guard.scopes.clone(),
            deadline: self.guard.deadline.clone(),
        }
    }
    pub fn bindings(&self) -> &[ModuleBindingSnapshot] {
        &self.bindings
    }
    /// Supervise all post-build setup and serving, including a failed listener
    /// bind or route composition panic, before returning to the process.
    pub async fn run<F, T>(self, future: F) -> anyhow::Result<T>
    where
        F: std::future::Future<Output = anyhow::Result<T>>,
    {
        let supervisor = self.guard.supervisor.clone();
        let result = std::panic::AssertUnwindSafe(future).catch_unwind().await;
        let cleanup = self.shutdown().await;
        let observed = supervisor.wait().await;
        match (result, cleanup, observed) {
            (Ok(Ok(value)), Ok(()), Ok(())) => Ok(value),
            (result, cleanup, observed) => {
                let startup = match result {
                    Ok(Ok(_)) => "completed".to_owned(),
                    Ok(Err(error)) => format!("{error:#}"),
                    Err(_) => "post-build route/setup panic".to_owned(),
                };
                anyhow::bail!(
                    "gateway module serving/setup: {startup}; cleanup: {cleanup:?}; supervisor: {observed:?}"
                )
            }
        }
    }
    pub async fn shutdown(mut self) -> anyhow::Result<()> {
        self.guard.finish().await
    }
}
async fn admit_request(
    State(scope): State<ModuleTaskScope>,
    request: Request,
    next: Next,
) -> Response {
    let permit = match scope.reserve() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let mut request = request;
    request.extensions_mut().insert(scope);
    let (response, permit) = match permit.handoff(async move { next.run(request).await }).await {
        Ok(result) => result,
        Err(error) => {
            tracing::error!(%error,"gateway module handler failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let (parts, body) = response.into_parts();
    // The reservation covers streaming bodies as well as the handler future.
    Response::from_parts(
        parts,
        Body::new(ScopedBody {
            body,
            cancelled: Box::pin(permit.cancellation_token().cancelled_owned()),
            _permit: Some(permit),
        }),
    )
}
struct ScopedBody {
    body: Body,
    cancelled: BoxFuture<'static, ()>,
    _permit: Option<super::ModuleTaskPermit>,
}
impl http_body::Body for ScopedBody {
    type Data = <Body as http_body::Body>::Data;
    type Error = <Body as http_body::Body>::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self._permit.is_none() {
            return Poll::Ready(None);
        }
        if self.cancelled.as_mut().poll(cx).is_ready() {
            self.body = Body::empty();
            self._permit.take();
            return Poll::Ready(Some(Err(axum::Error::new(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "gateway module stream interrupted by shutdown",
            )))));
        }
        let result = Pin::new(&mut self.body).poll_frame(cx);
        if matches!(result, Poll::Ready(None)) {
            self._permit.take();
        }
        result
    }
    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
}

#[cfg(test)]
mod framing_tests {
    use super::*;
    use axum::body::HttpBody;
    use axum::http::{HeaderMap, HeaderValue};
    use futures::{StreamExt, future::poll_fn, stream};
    struct Trailers(Option<HeaderMap>);
    impl HttpBody for Trailers {
        type Data = axum::body::Bytes;
        type Error = std::convert::Infallible;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(
                self.0
                    .take()
                    .map(|headers| Ok(http_body::Frame::trailers(headers))),
            )
        }
    }
    #[tokio::test]
    async fn trailers_survive_and_pending_streams_release_on_shutdown() {
        let scope = ModuleTaskScope::new();
        let permit = scope.reserve().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-owner-finish", HeaderValue::from_static("retained"));
        let mut body = ScopedBody {
            body: Body::new(Trailers(Some(headers))),
            cancelled: Box::pin(permit.cancellation_token().cancelled_owned()),
            _permit: Some(permit),
        };
        let frame = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.into_trailers().unwrap()["x-owner-finish"], "retained");
        assert!(
            poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .is_none()
        );
        scope.cancel();
        scope.wait().await;
        let scope = ModuleTaskScope::new();
        let permit = scope.reserve().unwrap();
        let mut body = ScopedBody {
            body: Body::from_stream(stream::pending::<Result<axum::body::Bytes, std::io::Error>>()),
            cancelled: Box::pin(permit.cancellation_token().cancelled_owned()),
            _permit: Some(permit),
        };
        scope.cancel();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            )
            .await
            .unwrap()
            .unwrap()
            .is_err()
        );
        scope.wait().await;
        let scope = ModuleTaskScope::new();
        let permit = scope.reserve().unwrap();
        let stream = stream::iter([Ok::<_, std::io::Error>(axum::body::Bytes::from_static(
            b"prefix",
        ))])
        .chain(stream::pending());
        let mut body = ScopedBody {
            body: Body::from_stream(stream),
            cancelled: Box::pin(permit.cancellation_token().cancelled_owned()),
            _permit: Some(permit),
        };
        assert_eq!(
            poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .unwrap()
                .unwrap()
                .into_data()
                .unwrap(),
            "prefix"
        );
        scope.cancel();
        assert!(
            poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .unwrap()
                .is_err()
        );
        assert!(
            poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
                .await
                .is_none()
        );
        scope.wait().await;
    }
}
