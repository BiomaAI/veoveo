use super::*;
use crate::{HostIdentity, Journal};
use axum::{Json, Router, extract::State, routing::get};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn enrollment_binds_the_first_engine_and_cannot_adopt_a_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("engine.sock");
    let first = Uuid::now_v7();
    let current = Arc::new(Mutex::new(first));
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let router = Router::new()
        .route(
            "/v1.53/info",
            get(|State(current): State<Arc<Mutex<Uuid>>>| async move {
                Json(Engine {
                    id: *current.lock().unwrap(),
                })
            }),
        )
        .with_state(current.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let docker = Docker::discover(&socket, "veoveo-retained".into())
        .await
        .unwrap();
    assert_eq!(docker.verify_engine().await.unwrap(), first);
    let root = directory.path().join("retained");
    let identity = HostIdentity {
        provider_id: Uuid::now_v7(),
        engine_id: first,
        namespace: "test".into(),
    };
    drop(Journal::open(root.clone(), identity.clone()).unwrap());

    let replacement = Uuid::now_v7();
    *current.lock().unwrap() = replacement;
    assert!(matches!(
        docker.verify_engine().await,
        Err(StorageError::IdentityMismatch)
    ));
    let reopened = Journal::reopen(root.clone(), identity.provider_id, &identity.namespace)
        .unwrap()
        .unwrap();
    let rebound = Docker::new(
        &socket,
        reopened.identity().engine_id,
        "veoveo-retained".into(),
    )
    .unwrap();
    assert!(matches!(
        rebound.verify_engine().await,
        Err(StorageError::IdentityMismatch)
    ));
    drop(reopened);
    let replacement = Docker::discover(&socket, "veoveo-retained".into())
        .await
        .unwrap();
    let identity = HostIdentity {
        engine_id: replacement.verify_engine().await.unwrap(),
        ..identity
    };
    assert!(matches!(
        Journal::open(root, identity),
        Err(StorageError::IdentityMismatch)
    ));

    *current.lock().unwrap() = Uuid::nil();
    assert!(matches!(
        Docker::discover(&socket, "veoveo-retained".into()).await,
        Err(StorageError::InvalidIdentity)
    ));
    server.abort();
    let _ = server.await;
}
