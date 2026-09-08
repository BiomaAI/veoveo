use super::*;
use tokio::io::AsyncReadExt;
use veoveo_mcp_contract::{
    ArtifactReadCapabilityId, ArtifactReadCapabilitySecret, ArtifactTaskId,
    IssuedArtifactReadCapability,
};

struct NoValidation;
impl RrdIdentityValidator for NoValidation {
    fn validate(&self, _: &Path, _: u64, _: &str) -> Result<()> {
        anyhow::bail!("partial download must not reach RRD validation")
    }
}

#[tokio::test]
async fn cancelled_download_releases_disk_reservation_and_partial_file() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let id = ArtifactId::new();
    let capability = IssuedArtifactReadCapability {
        capability_id: ArtifactReadCapabilityId::new(),
        task_id: ArtifactTaskId::new(),
        secret: ArtifactReadCapabilitySecret::new("cancel-fixture-secret-012345678901").unwrap(),
        expires_at: Utc::now() + chrono::TimeDelta::hours(1),
    };
    let (started, receiving) = tokio::sync::oneshot::channel();
    let endpoint = tokio::spawn(async move {
        for index in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut headers = Vec::new();
            while !headers.ends_with(b"\r\n\r\n") {
                assert!(headers.len() < 8192);
                headers.push(socket.read_u8().await.unwrap());
            }
            if index == 0 {
                let metadata = format!(
                    r#"{{"artifact_id":"{id}","artifact_uri":"{}","byte_len":5,"created_at":"2026-09-08T00:00:00Z"}}"#,
                    id.plane_uri()
                );
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{metadata}", metadata.len()).as_bytes()).await.unwrap();
            } else {
                socket
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nv",
                    )
                    .await
                    .unwrap();
                started.send(()).unwrap();
                // Hold the response open until the test aborts this endpoint.
                std::future::pending::<()>().await;
                return;
            }
        }
    });
    let cache = LayerCache::new(
        directory.path().to_owned(),
        LayerCacheLimits {
            managed_bytes: 5,
            minimum_free_bytes: 1,
        },
        HttpArtifactPlane::new(url),
    )
    .unwrap();
    let worker_cache = cache.clone();
    let download = tokio::spawn(async move {
        worker_cache
            .materialize_with_validator(
                ArtifactReadAuthority::Task {
                    task_id: capability.task_id,
                    capability: &capability,
                },
                id,
                5,
                &hex::encode(Sha256::digest(b"valid")),
                Arc::new(NoValidation),
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), receiving)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if std::fs::read_dir(directory.path())
                .unwrap()
                .any(|entry| entry.unwrap().metadata().unwrap().len() == 1)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(cache.stats().unwrap().reserved_bytes, 5);
    download.abort();
    assert!(download.await.unwrap_err().is_cancelled());
    assert_eq!(cache.stats().unwrap().reserved_bytes, 0);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    cache
        .reserve(5)
        .expect("cancelled transfer retained cache capacity");
    cache.release_reservation(5);
    endpoint.abort();
    assert!(endpoint.await.unwrap_err().is_cancelled());
}
