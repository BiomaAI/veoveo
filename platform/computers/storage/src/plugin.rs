//! The daemon's private volume endpoint. No notification releases writer authority.
use crate::{Result, Service, StorageError, service::Volume};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, OriginalUri, State},
    routing::post,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Named {
    name: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Create {
    name: String,
    #[serde(default)]
    opts: Option<BTreeMap<String, String>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct Reference {
    name: String,
    #[serde(rename = "ID")]
    id: String,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Reply {
    err: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mountpoint: Option<std::path::PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volume: Option<Volume>,
    #[serde(skip_serializing_if = "Option::is_none")]
    volumes: Option<Vec<Volume>>,
}
#[derive(Serialize)]
struct Activation {
    #[serde(rename = "Implements")]
    implements: [&'static str; 1],
}
#[derive(Serialize)]
struct Capabilities {
    #[serde(rename = "Capabilities")]
    capabilities: Scope,
}
#[derive(Serialize)]
struct Scope {
    #[serde(rename = "Scope")]
    scope: &'static str,
}

pub fn router(service: Arc<Service>) -> Router {
    let mut router = Router::new()
        .route(
            "/Plugin.Activate",
            post(|| async {
                Json(Activation {
                    implements: ["VolumeDriver"],
                })
            }),
        )
        .route(
            "/VolumeDriver.Capabilities",
            post(|| async {
                Json(Capabilities {
                    capabilities: Scope { scope: "local" },
                })
            }),
        );
    for method in [
        "Create", "Remove", "Mount", "Unmount", "Path", "Get", "List",
    ] {
        router = router.route(&format!("/VolumeDriver.{method}"), post(dispatch));
    }
    router
        .layer(DefaultBodyLimit::max(4096))
        .with_state(service)
}
async fn dispatch(
    State(service): State<Arc<Service>>,
    OriginalUri(uri): OriginalUri,
    body: Bytes,
) -> Json<Reply> {
    let reply = async {
        // Allocation calls have a separate transport bound so their Docker
        // callbacks can always enter this bounded plugin queue.
        let _permit = service
            .calls
            .try_acquire()
            .map_err(|_| StorageError::Busy)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(180),
            operation(&service, uri.path(), &body),
        )
        .await
        .map_err(|_| StorageError::Unavailable)?
    }
    .await;
    Json(reply.unwrap_or_else(|error| Reply {
        err: error.to_string(),
        ..Reply::default()
    }))
}
fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    if body.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(StorageError::InvalidIdentity);
    }
    serde_json::from_slice(body).map_err(|_| StorageError::InvalidIdentity)
}
async fn operation(service: &Service, path: &str, body: &[u8]) -> Result<Reply> {
    let mut reply = Reply::default();
    match path {
        "/VolumeDriver.List" => {
            // Docker's List request has an empty object.
            let fields: BTreeMap<String, serde::de::IgnoredAny> = parse(body)?;
            if !fields.is_empty() {
                return Err(StorageError::InvalidIdentity);
            }
            reply.volumes = Some(service.volumes().await?);
        }
        "/VolumeDriver.Create" => {
            let request: Create = parse(body)?;
            if request.opts.is_some_and(|opts| !opts.is_empty()) {
                return Err(StorageError::InvalidIdentity);
            }
            service.volume(&request.name, false).await?;
        }
        "/VolumeDriver.Mount" | "/VolumeDriver.Unmount" => {
            let request: Reference = parse(body)?;
            if request.id.is_empty()
                || request.id.len() > 128
                || !request
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return Err(StorageError::InvalidIdentity);
            }
            let volume = service
                .volume(&request.name, path == "/VolumeDriver.Mount")
                .await?;
            if path == "/VolumeDriver.Mount" {
                reply.mountpoint = Some(volume.mountpoint);
            }
        }
        "/VolumeDriver.Remove" => {
            let request: Named = parse(body)?;
            service.volume(&request.name, false).await?;
            return Err(StorageError::PurgeRequired);
        }
        "/VolumeDriver.Get" | "/VolumeDriver.Path" => {
            let request: Named = parse(body)?;
            let volume = service.volume(&request.name, false).await?;
            if path == "/VolumeDriver.Get" {
                reply.volume = Some(volume);
            } else {
                reply.mountpoint = Some(volume.mountpoint);
            }
        }
        _ => return Err(StorageError::InvalidIdentity),
    }
    Ok(reply)
}
