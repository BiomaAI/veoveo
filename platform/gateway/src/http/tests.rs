use super::*;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;
use veoveo_gateway_contract::ModuleBindingState;

#[tokio::test]
async fn close_fences_concurrent_reservations_and_owns_delayed_upgrades() {
    let scope = ModuleTaskScope::new();
    let permit = scope.reserve().unwrap();
    scope.cancel();
    assert!(scope.spawn(async {}).is_err());
    assert!(scope.reserve().is_err());
    let (ready, receiver) = tokio::sync::oneshot::channel();
    let worker = permit.spawn(async move {
        receiver.await.unwrap();
    });
    assert!(
        tokio::time::timeout(Duration::from_millis(20), scope.wait())
            .await
            .is_err()
    );
    ready.send(()).unwrap();
    worker.await.unwrap();
    scope.wait().await;
    for _ in 0..64 {
        let scope = ModuleTaskScope::new();
        let copy = scope.clone();
        let thread = std::thread::spawn(move || copy.reserve());
        scope.close();
        let admitted = thread.join().unwrap();
        drop(admitted);
        assert!(scope.reserve().is_err());
        scope.wait().await;
    }
}

#[tokio::test]
async fn cleanup_wait_is_cancellation_safe_and_observes_preexisting_owners() {
    let supervisor = ModuleCleanupSupervisor::default();
    let mut guard = super::lifecycle::CleanupGuard::new(supervisor.clone()).unwrap();
    let scope = ModuleTaskScope::new();
    guard.scopes.push(scope.clone());
    let token = scope.cancellation_token();
    scope
        .spawn(async move {
            token.cancelled().await;
        })
        .unwrap();
    let waiter = tokio::spawn({
        let supervisor = supervisor.clone();
        async move { supervisor.wait().await }
    });
    tokio::task::yield_now().await;
    assert!(!waiter.is_finished());
    waiter.abort();
    let _ = waiter.await;
    drop(guard);
    supervisor.wait().await.unwrap();
}

#[tokio::test]
async fn cleanup_timeout_is_one_persisted_outcome() {
    let supervisor = ModuleCleanupSupervisor::default();
    let mut guard = super::lifecycle::CleanupGuard::new(supervisor.clone()).unwrap();
    let scope = ModuleTaskScope::new();
    guard.scopes.push(scope.clone());
    let permit = scope.reserve().unwrap();
    *guard.deadline.lock() = Some(tokio::time::Instant::now() + Duration::from_millis(30));
    let error = guard.finish().await.unwrap_err().to_string();
    assert!(error.contains("unresolved"));
    drop(guard);
    drop(permit);
    assert!(
        supervisor
            .wait()
            .await
            .unwrap_err()
            .to_string()
            .contains("unresolved")
    );
}

async fn context() -> (crate::test_store::TestDb, GatewayHttpContext) {
    let db = crate::test_store::TestDb::new().await;
    let control = serde_json::from_str(include_str!(
        "../../../computers/tests/support/gateway.json"
    ))
    .unwrap();
    let catalog = crate::GatewayCatalog::from_control_plane(
        control,
        crate::test_catalog_admission::binding(),
    )
    .unwrap();
    let issuer = veoveo_mcp_contract::GatewayInternalTokenIssuer::new(
        veoveo_mcp_contract::TokenIssuer::parse(veoveo_mcp_contract::GATEWAY_INTERNAL_TOKEN_ISSUER)
            .unwrap(),
        veoveo_mcp_contract::GatewayInternalSigningKey::new(
            "fixture",
            rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519)
                .unwrap()
                .serialize_der(),
        )
        .unwrap(),
    );
    let context = GatewayHttpContext {
        deployment: veoveo_mcp_contract::PublicDeployment::new("https://computers.test").unwrap(),
        catalog: crate::GatewayCatalogHandle::new(Arc::new(catalog)),
        gateway_state: crate::GatewayState::new(db.a.clone()),
        internal_token_issuer: issuer,
        upstream_http: crate::GatewayUpstreamHttpClientPool::new(),
        auth_http: Arc::new(parking_lot::RwLock::new(reqwest::Client::new())),
    };
    (db, context)
}

#[tokio::test]
async fn independent_module_declarations_and_startup_cleanup() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (_db, context) = context().await;
        let invented = veoveo_modules::ModuleName::new("independent-example").unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let mut modules = GatewayModules::new();
        let observed = counter.clone();
        modules
            .register(invented.clone(), move |_, _| {
                observed.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Ok(GatewayModuleRoutes::new(
                        Router::new(),
                        Router::new().route(
                            "/extension/{profile}/nested",
                            get(|request: axum::extract::Request| async move {
                                profile_from_request(&request)
                                    .map(|profile| profile.to_string())
                                    .ok_or(StatusCode::NOT_FOUND)
                            }),
                        ),
                    ))
                })
            })
            .unwrap();
        assert!(modules.declare_unbound(invented.clone()).is_err());
        let missing = veoveo_modules::ModuleName::new("missing").unwrap();
        assert!(modules.build(context.clone(), &[missing]).await.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        let mut profile_modules = GatewayModules::new();
        profile_modules
            .register(invented.clone(), |_, _| {
                Box::pin(async {
                    Ok(GatewayModuleRoutes::new(
                        Router::new().nest(
                            "/arbitrary",
                            Router::new().route(
                                "/extension/{profile}/nested",
                                get(|| async { StatusCode::NO_CONTENT }),
                            ),
                        ),
                        Router::new(),
                    ))
                })
            })
            .unwrap();
        let supervised = profile_modules.supervisor();
        let profile_routes = profile_modules
            .build(context.clone(), std::slice::from_ref(&invented))
            .await
            .unwrap();
        for (path, status) in [
            (
                "/arbitrary/extension/operator/nested",
                StatusCode::UNAUTHORIZED,
            ),
            ("/arbitrary/extension/unknown/nested", StatusCode::NOT_FOUND),
            (
                "/arbitrary/extension/%6fperator/nested",
                StatusCode::NOT_FOUND,
            ),
        ] {
            let response = profile_routes
                .router()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            drop(response);
        }
        profile_routes.shutdown().await.unwrap();
        supervised.wait().await.unwrap();
        let mut modules = GatewayModules::new();
        modules
            .declare_unbound(veoveo_modules::ModuleName::new("unused").unwrap())
            .unwrap();
        modules
            .register(invented.clone(), |_, _| {
                Box::pin(async {
                    Ok(GatewayModuleRoutes::new(
                        Router::new(),
                        Router::new().nest(
                            "/extra",
                            Router::new().route(
                                "/extension/{profile}/nested",
                                get(|request: axum::extract::Request| async move {
                                    profile_from_request(&request)
                                        .map(|profile| profile.to_string())
                                        .ok_or(StatusCode::NOT_FOUND)
                                }),
                            ),
                        ),
                    ))
                })
            })
            .unwrap();
        let supervisor = modules.supervisor();
        let built = modules
            .build(context.clone(), std::slice::from_ref(&invented))
            .await
            .unwrap();
        assert_eq!(
            built
                .bindings()
                .iter()
                .find(|binding| !binding.required)
                .unwrap()
                .state,
            ModuleBindingState::Unbound
        );
        for (path, status) in [
            ("/extra/extension/operator/nested", StatusCode::OK),
            ("/extra/extension/%6fperator/nested", StatusCode::NOT_FOUND),
            ("/extra/extension/operator/unknown", StatusCode::NOT_FOUND),
        ] {
            let response = built
                .router()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status);
            drop(response);
        }
        built.shutdown().await.unwrap();
        supervisor.wait().await.unwrap();
        let mut occupied_modules = GatewayModules::new();
        let settled = Arc::new(AtomicUsize::new(0));
        let observed = settled.clone();
        occupied_modules
            .register(invented.clone(), move |_, scope| {
                Box::pin(async move {
                    let token = scope.cancellation_token();
                    scope
                        .spawn(async move {
                            token.cancelled().await;
                            observed.fetch_add(1, Ordering::SeqCst);
                        })
                        .unwrap();
                    Ok(GatewayModuleRoutes::new(Router::new(), Router::new()))
                })
            })
            .unwrap();
        let occupied_supervisor = occupied_modules.supervisor();
        let occupied_built = occupied_modules.build(context.clone(), &[]).await.unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let error = occupied_built
            .run(async move {
                let _second = tokio::net::TcpListener::bind(address).await?;
                Ok(())
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("serving/setup"));
        assert_eq!(settled.load(Ordering::SeqCst), 1);
        occupied_supervisor.wait().await.unwrap();
        drop(listener);
        for panic_before_future in [false, true] {
            let mut modules = GatewayModules::new();
            modules
                .register(invented.clone(), move |_, scope| {
                    let token = scope.cancellation_token();
                    scope
                        .spawn(async move {
                            token.cancelled().await;
                        })
                        .unwrap();
                    assert!(!panic_before_future, "fixture factory panic");
                    Box::pin(async { anyhow::bail!("fixture startup error") })
                })
                .unwrap();
            let supervisor = modules.supervisor();
            let error = modules.build(context.clone(), &[]).await.err().unwrap();
            assert!(
                error.to_string().contains("startup failed")
                    || error.to_string().contains("factory task failed")
            );
            supervisor.wait().await.unwrap();
        }
        let mut modules = GatewayModules::new();
        let (ready, seen) = tokio::sync::oneshot::channel();
        modules
            .register(invented, move |_, scope| {
                Box::pin(async move {
                    let token = scope.cancellation_token();
                    ready.send(()).unwrap();
                    token.cancelled().await;
                    anyhow::bail!("cancelled startup")
                })
            })
            .unwrap();
        let supervisor = modules.supervisor();
        let build = tokio::spawn(async move { modules.build(context, &[]).await });
        seen.await.unwrap();
        build.abort();
        let _ = build.await;
        supervisor.wait().await.unwrap();
    })
    .await
    .expect("module fixture timed out");
}
