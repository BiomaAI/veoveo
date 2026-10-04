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
            ArtifactUri::presented(&ResourceScheme::parse("independent-fixture").unwrap(), id);
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

#[tokio::test]
async fn artifact_http_rejects_unknown_request_fields_and_keeps_metadata_open() {
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        let (base, caller) = spawn_service().await;
        let plane = HttpArtifactPlane::new(&base);
        let metadata = plane
            .put(
                &caller,
                PutArtifactRequest {
                    metadata: serde_json::json!({"providerExtension":{"arbitrary":true}}),
                    ..Default::default()
                },
                b"strict fixture".to_vec(),
            )
            .await
            .unwrap();
        let id = metadata.artifact_id();
        let client = reqwest::Client::new();
        let release = veoveo_mcp_contract::SetArtifactReleaseStateRequest {
            release_state: ArtifactReleaseState::Private,
        };
        let mut release_body = serde_json::to_value(&release).unwrap();
        serde_json::from_value::<veoveo_mcp_contract::SetArtifactReleaseStateRequest>(
            release_body.clone(),
        )
        .unwrap();
        release_body["rootExtra"] = serde_json::json!(true);
        let grant = veoveo_mcp_contract::PutGrantRequest {
            subject: AccessSubject::Principal(caller.identity.actor.id.clone()),
            level: veoveo_types::AccessLevel::Read,
        };
        let mut grant_body = serde_json::to_value(&grant).unwrap();
        serde_json::from_value::<veoveo_mcp_contract::PutGrantRequest>(grant_body.clone()).unwrap();
        grant_body["subject"]["nestedExtra"] = serde_json::json!("secret-sentinel");
        let mut share_body =
            serde_json::to_value(CreateArtifactShareLinkRequest::default()).unwrap();
        serde_json::from_value::<CreateArtifactShareLinkRequest>(share_body.clone()).unwrap();
        share_body["rootExtra"] = serde_json::json!(true);
        for (suffix, body, field) in [
            ("release-state", release_body, "rootExtra"),
            ("grants", grant_body, "nestedExtra"),
            ("share-links", share_body, "rootExtra"),
        ] {
            let response = client
                .request(
                    if suffix == "release-state" {
                        reqwest::Method::PUT
                    } else {
                        reqwest::Method::POST
                    },
                    format!("{base}/artifacts/{id}/{suffix}"),
                )
                .bearer_auth(&caller.bearer_token)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
            let diagnostic = response.text().await.unwrap();
            assert!(
                diagnostic.contains(field),
                "{suffix} must name {field}: {diagnostic}"
            );
            assert!(!diagnostic.contains("secret-sentinel"));
        }
        for (field, invalid) in [
            ("kind", serde_json::json!("unknown field `secret-sentinel`")),
            ("id", serde_json::json!({"secret":"secret-sentinel"})),
            ("id", serde_json::json!("secret-sentinel\u{0}")),
        ] {
            let mut body = serde_json::to_value(&grant).unwrap();
            body["subject"][field] = invalid;
            let response = client
                .post(format!("{base}/artifacts/{id}/grants"))
                .bearer_auth(&caller.bearer_token)
                .json(&body)
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
            assert_eq!(response.text().await.unwrap(), "invalid JSON request body");
        }
        assert_eq!(plane.head(&caller, &id).await.unwrap(), metadata);
    })
    .await
    .expect("strict Artifact HTTP test exceeded fifteen seconds");
}
