use super::*;
use crate::AuthorityCursor;
use veoveo_mcp_knowledge_extension::ReadPolicy;

#[tokio::test]
async fn authority_observations_and_pages_preserve_stored_tenant_provenance() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let files = AuthorityFiles::new().await;
        let catalog = TimeCatalog::new(db.b.clone());
        let owner = scope(&db.a, "authority-knowledge").await;
        let foreign = scope(&db.a, "authority-foreign").await;
        let (release, _) = stage(
            &catalog,
            &owner,
            AuthorityDatasetKind::LeapSeconds,
            &files.first_leaps,
        )
        .await;
        let mut reader = owner.clone();
        reader.work_context = "another-selected-context".parse().unwrap();
        let observed = catalog
            .observed_release(&reader, &release.release_id)
            .await
            .unwrap()
            .unwrap();
        let (text, observation) = observed.document().unwrap();
        assert!(!observed.value().release_uri().is_bootstrap());
        assert_eq!(
            observation.access().unwrap().work_context,
            owner.work_context
        );
        assert_eq!(
            observation.access().unwrap().read_policy,
            ReadPolicy::Tenant {}
        );
        assert_eq!(observation.modified_at(), Some(release.validated_at));
        assert!(text.contains(release.source_digest_sha256.canonical().as_str()));
        assert!(
            catalog
                .observed_release(&foreign, &release.release_id)
                .await
                .unwrap()
                .is_none()
        );

        for index in 0..101 {
            let mut copy = release.clone();
            copy.source_digest_sha256 = format!("{index:064x}").parse().unwrap();
            copy.release_id =
                AuthorityReleaseId::parse(format!("time-release-{}", Uuid::now_v7())).unwrap();
            catalog.create_release(&owner, copy).await.unwrap();
        }
        let page = catalog.releases_page(&reader, None).await.unwrap();
        assert_eq!(page.items.len(), 100);
        let cursor = page.next_cursor.unwrap();
        assert_eq!(cursor.release_id(), &page.items.last().unwrap().release_id);
        let last = catalog.releases_page(&reader, Some(&cursor)).await.unwrap();
        assert_eq!(last.items.len(), 2);
        assert!(last.next_cursor.is_none());
        assert!(
            catalog
                .releases_page(&foreign, Some(&AuthorityCursor::new(&release.release_id)))
                .await
                .unwrap()
                .items
                .is_empty()
        );

        let registry = files.registry();
        let bootstrap = registry.bootstrap_references();
        assert_eq!(bootstrap.len(), 2);
        assert!(bootstrap.iter().all(|r| r.release_uri().is_bootstrap()));
        assert!(bootstrap[0].release_id() < bootstrap[1].release_id());
    })
    .await
    .expect("Time authority knowledge qualification exceeded 90 seconds");
}
