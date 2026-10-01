use super::*;
use veoveo_artifact_contract::ArtifactUri;
use veoveo_types::ResourceScheme;

#[tokio::test]
async fn typed_resolution_round_trips_and_http_rejects_malformed_addresses() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let (base, caller) = spawn_service().await;
        let plane = HttpArtifactPlane::new(&base);
        let metadata = plane
            .put(
                &caller,
                PutArtifactRequest::default(),
                b"uri fixture".to_vec(),
            )
            .await
            .unwrap();
        let id = metadata.artifact_id();
        let snapshot = plane.metadata_snapshot(&caller, &id).await.unwrap();
        assert_eq!(snapshot.metadata(), &metadata);
        assert_eq!(snapshot.read_grants().len(), 1);
        assert_eq!(plane.head(&caller, &id).await.unwrap(), metadata);
        assert_eq!(
            plane.metadata_snapshot(&caller, &ArtifactId::new()).await,
            Err(ArtifactPlaneError::NotFound)
        );
        let unauthenticated = reqwest::Client::new()
            .get(format!("{base}/artifacts/{id}/snapshot"))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), reqwest::StatusCode::UNAUTHORIZED);

        let presentation =
            ArtifactUri::presented(&ResourceScheme::new("independent-fixture").unwrap(), id);
        for uri in [id.plane_uri(), presentation] {
            let object = plane.resolve(&caller, &uri).await.unwrap();
            assert_eq!(object.metadata, metadata);
            assert_eq!(object.bytes, b"uri fixture");
            let download = plane.download(&caller, &uri).await.unwrap();
            assert_eq!(download.metadata, metadata);
            assert_eq!(
                download.response.bytes().await.unwrap().as_ref(),
                b"uri fixture"
            );
        }
        let client = reqwest::Client::new();
        for uri in [
            format!("artifact://{id}?query=forbidden"),
            format!("fixture://artifact/{id}/extra"),
            format!("fixture://user:secret-sentinel@artifact/{id}"),
            "artifact://0197f78e-f2f0-7a6e-ca5d-f41c691e4471".into(),
        ] {
            let response = client
                .get(format!("{base}/resolve"))
                .bearer_auth(&caller.bearer_token)
                .query(&[("uri", uri)])
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
            assert!(!response.text().await.unwrap().contains("secret-sentinel"));
        }
    })
    .await
    .expect("Artifact HTTP URI fixture exceeded 15 seconds");
}
