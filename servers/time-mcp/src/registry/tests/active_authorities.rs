//! An authenticated operator uses the public active pointer guard for the next cut.
use super::hosted::{Fixture, token};
use super::*;
#[tokio::test]
async fn operator_reads_pointer_guard_for_next_activation() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let fixture = Fixture::new().await;
        let (gateway, state, files, db) = (
            &fixture.gateway,
            &fixture.state,
            &fixture.files,
            &fixture.db,
        );
        let owner_token = token("tenant-a");
        // The HTTP operator reads the actual pointer guard, rather than the
        // release's independently advancing record version, for the next cut.
        let request = gateway
            .request("/admin/active-authorities")
            .header("authorization", format!("Bearer {owner_token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let (status, body) = gateway.send(request).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let empty: AdminPage<crate::ActiveAuthoritySelection> =
            serde_json::from_str(&body).unwrap();
        assert!(empty.items.is_empty());
        let operator = scope(&db.a, "tenant-a").await;
        let mut guard = TimeWriteGuard::Absent;
        for version in 1..=2 {
            let (candidate, _) = stage(
                &state.catalog,
                &operator,
                AuthorityDatasetKind::Tzdb,
                &files.tzdb,
            )
            .await;
            let activation = ActivateReleaseRequest {
                expected_release_record_version: candidate.record_version,
                expected_active_pointer_version: guard,
            };
            let request = gateway
                .request(&format!(
                    "/admin/releases/{}/activate",
                    candidate.release_id
                ))
                .method("POST")
                .header("authorization", format!("Bearer {owner_token}"))
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&activation).unwrap(),
                ))
                .unwrap();
            let (status, body) = gateway.send(request).await;
            assert_eq!(status, axum::http::StatusCode::OK, "{body}");
            let request = gateway
                .request("/admin/active-authorities")
                .header("authorization", format!("Bearer {owner_token}"))
                .body(axum::body::Body::empty())
                .unwrap();
            let (status, body) = gateway.send(request).await;
            assert_eq!(status, axum::http::StatusCode::OK, "{body}");
            let selected: AdminPage<crate::ActiveAuthoritySelection> =
                serde_json::from_str(&body).unwrap();
            assert_eq!(selected.items.len(), 1);
            let selected = &selected.items[0];
            assert_eq!(selected.pointer_version.get(), version);
            assert_eq!(selected.release.record_version.get(), 2);
            assert_eq!(selected.release.release_id, candidate.release_id);
            guard = selected.write_guard();
        }
        let request = gateway
            .request("/admin/active-authorities")
            .header("authorization", format!("Bearer {}", token("tenant-b")))
            .body(axum::body::Body::empty())
            .unwrap();
        let (status, body) = gateway.send(request).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert!(
            serde_json::from_str::<AdminPage<crate::ActiveAuthoritySelection>>(&body)
                .unwrap()
                .items
                .is_empty()
        );
    })
    .await
    .expect("Time active selection HTTP control exceeded90seconds");
}
