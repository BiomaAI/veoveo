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
        provider_id: veoveo_computers_runtime::ProviderInstanceId::new(),
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

#[derive(Clone)]
struct VolumeDaemon {
    engine: Uuid,
    replacement: Option<Uuid>,
    info_calls: usize,
    volume: Option<serde_json::Value>,
    created: serde_json::Value,
    create_status: StatusCode,
    creates: Vec<serde_json::Value>,
}

async fn volume_daemon(
    volume: Option<serde_json::Value>,
    created: serde_json::Value,
    replacement: Option<Uuid>,
    create_status: StatusCode,
) -> (
    tempfile::TempDir,
    Docker,
    Arc<Mutex<VolumeDaemon>>,
    tokio::task::JoinHandle<()>,
) {
    use axum::{response::IntoResponse, routing::post};
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("engine.sock");
    let engine = Uuid::now_v7();
    let state = Arc::new(Mutex::new(VolumeDaemon {
        engine,
        replacement,
        info_calls: 0,
        volume,
        created,
        create_status,
        creates: Vec::new(),
    }));
    let router = Router::new()
        .route(
            "/v1.53/info",
            get(|State(state): State<Arc<Mutex<VolumeDaemon>>>| async move {
                let mut state = state.lock().unwrap();
                state.info_calls += 1;
                Json(Engine {
                    id: if state.info_calls > 1 {
                        state.replacement.unwrap_or(state.engine)
                    } else {
                        state.engine
                    },
                })
            }),
        )
        .route(
            "/v1.53/volumes/{name}",
            get(|State(state): State<Arc<Mutex<VolumeDaemon>>>| async move {
                match state.lock().unwrap().volume.clone() {
                    Some(volume) => Json(volume).into_response(),
                    None => StatusCode::NOT_FOUND.into_response(),
                }
            }),
        )
        .route(
            "/v1.53/volumes/create",
            post(
                |State(state): State<Arc<Mutex<VolumeDaemon>>>,
                 Json(request): Json<serde_json::Value>| async move {
                    let mut state = state.lock().unwrap();
                    state.creates.push(request);
                    state.volume = Some(state.created.clone());
                    (state.create_status, Json(state.created.clone()))
                },
            ),
        )
        .with_state(state.clone());
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let docker = Docker::new(&socket, engine, "veoveo-retained".into()).unwrap();
    (directory, docker, state, server)
}

fn approved_volume(name: &str) -> serde_json::Value {
    serde_json::json!({"Name":name,"Driver":"veoveo-retained","Options":null,"Labels":{
        "openshell.ai/sandbox-attachable":"true",
        "openshell.ai/sandbox-attachable-workspace":"default"
    }})
}

#[tokio::test]
async fn created_volume_carries_provider_approval_and_correct_volume_reuses_without_mutation() {
    let name = crate::service::volume_name(Uuid::now_v7()).unwrap();
    let (_directory, docker, state, server) =
        volume_daemon(None, approved_volume(&name), None, StatusCode::CREATED).await;
    docker.ensure_volume(&name).await.unwrap();
    docker.ensure_volume(&name).await.unwrap();
    {
        let state = state.lock().unwrap();
        assert_eq!(
            state.creates,
            [
                serde_json::json!({"Name":name,"Driver":"veoveo-retained","Labels":{
                    "openshell.ai/sandbox-attachable":"true",
                    "openshell.ai/sandbox-attachable-workspace":"default"
                }})
            ]
        );
        assert_eq!(state.info_calls, 4);
    }
    server.abort();
    let _ = server.await;

    let mut reused = approved_volume(&name);
    reused["Labels"]["operator.example/extra"] = serde_json::json!("retained");
    reused["Options"] = serde_json::json!({});
    let (_directory, docker, state, server) =
        volume_daemon(Some(reused.clone()), reused, None, StatusCode::CREATED).await;
    docker.ensure_volume(&name).await.unwrap();
    assert!(state.lock().unwrap().creates.is_empty());
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn inspected_or_created_unapproved_volume_never_adopts_relabels_or_recreates() {
    let name = crate::service::volume_name(Uuid::now_v7()).unwrap();
    let approved = approved_volume(&name);
    let mut missing = approved.clone();
    missing.as_object_mut().unwrap().remove("Labels");
    let mut null = approved.clone();
    null["Labels"] = serde_json::Value::Null;
    let mut absent_claim = approved.clone();
    absent_claim["Labels"]
        .as_object_mut()
        .unwrap()
        .remove("openshell.ai/sandbox-attachable-workspace");
    let mut false_claim = approved.clone();
    false_claim["Labels"]["openshell.ai/sandbox-attachable"] = serde_json::json!("false");
    let mut foreign = approved.clone();
    foreign["Labels"]["openshell.ai/sandbox-attachable-workspace"] = serde_json::json!("foreign");
    for rejected in [missing, null, absent_claim, false_claim, foreign] {
        let (_directory, docker, state, server) = volume_daemon(
            Some(rejected.clone()),
            rejected.clone(),
            None,
            StatusCode::CREATED,
        )
        .await;
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::IdentityMismatch)
        ));
        assert!(state.lock().unwrap().creates.is_empty());
        server.abort();
        let _ = server.await;
        // A daemon's successful Create status cannot admit missing approval claims.
        let (_directory, docker, state, server) =
            volume_daemon(None, rejected, None, StatusCode::CREATED).await;
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::IdentityMismatch)
        ));
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::IdentityMismatch)
        ));
        assert_eq!(state.lock().unwrap().creates.len(), 1);
        server.abort();
        let _ = server.await;
    }
}

#[tokio::test]
async fn approval_labels_do_not_replace_name_driver_options_or_engine_identity() {
    let name = crate::service::volume_name(Uuid::now_v7()).unwrap();
    let approved = approved_volume(&name);
    let mut wrong_name = approved.clone();
    wrong_name["Name"] = serde_json::json!("foreign");
    let mut wrong_driver = approved.clone();
    wrong_driver["Driver"] = serde_json::json!("local");
    let mut options = approved.clone();
    options["Options"] = serde_json::json!({"device":"foreign"});
    for rejected in [wrong_name, wrong_driver, options] {
        let (_directory, docker, state, server) = volume_daemon(
            Some(rejected.clone()),
            rejected.clone(),
            None,
            StatusCode::CREATED,
        )
        .await;
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::IdentityMismatch)
        ));
        assert!(state.lock().unwrap().creates.is_empty());
        server.abort();
        let _ = server.await;
        let (_directory, docker, state, server) =
            volume_daemon(None, rejected, None, StatusCode::CREATED).await;
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::IdentityMismatch)
        ));
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::IdentityMismatch)
        ));
        assert_eq!(state.lock().unwrap().creates.len(), 1);
        server.abort();
        let _ = server.await;
    }
    let (_directory, docker, state, server) = volume_daemon(
        Some(approved.clone()),
        approved,
        Some(Uuid::now_v7()),
        StatusCode::CREATED,
    )
    .await;
    assert!(matches!(
        docker.ensure_volume(&name).await,
        Err(StorageError::IdentityMismatch)
    ));
    assert_eq!(state.lock().unwrap().info_calls, 2);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn uncertain_create_resolves_only_from_an_approved_inspected_volume() {
    let name = crate::service::volume_name(Uuid::now_v7()).unwrap();
    for approved in [true, false] {
        let mut observed = approved_volume(&name);
        if !approved {
            observed["Labels"] = serde_json::Value::Null;
        }
        let (_directory, docker, state, server) =
            volume_daemon(None, observed, None, StatusCode::INTERNAL_SERVER_ERROR).await;
        assert!(matches!(
            docker.ensure_volume(&name).await,
            Err(StorageError::BackendUnavailable)
        ));
        // A mutation error does not prove no effect; only an admitted observation settles it.
        if approved {
            docker.ensure_volume(&name).await.unwrap();
        } else {
            assert!(matches!(
                docker.ensure_volume(&name).await,
                Err(StorageError::IdentityMismatch)
            ));
        }
        assert_eq!(state.lock().unwrap().creates.len(), 1);
        server.abort();
        let _ = server.await;
    }
}
