//! Dataset release pages and exact reads, backed by tenant/parent-scoped SQL.
use crate::persistence::MapRepository;
use anyhow::{Result, ensure};
use serde::Deserialize;

use super::{MapAccessContext, MapCatalog, decode};
use crate::contract::{
    ActiveDatasetRelease, ActiveReleasePointer, DatasetRelease, DatasetReleaseId,
    DatasetReleaseState, ListActiveDatasetReleasesOutput, ListActiveDatasetReleasesRequest,
    MapCatalogPage, MapDatasetId, MapSourceId,
};

pub const PAGE_SIZE: usize = 100;

const SELECT_ACTIVE: &str = include_str!("../queries/catalog/releases/releases/statement_1.surql");

#[derive(Deserialize)]
struct ActiveReleaseRow {
    pointer_id: String,
    dataset_id: MapDatasetId,
    release_id: DatasetReleaseId,
    previous_release_id: Option<DatasetReleaseId>,
    record_version: u64,
    activated_at: chrono::DateTime<chrono::Utc>,
    source_id: MapSourceId,
    release_version: u64,
    version_label: String,
    source_digest_sha256: String,
    valid_from: chrono::DateTime<chrono::Utc>,
    valid_until: Option<chrono::DateTime<chrono::Utc>>,
    canonical_json: String,
}

impl ActiveReleaseRow {
    fn checked(self, scope: &MapAccessContext) -> Result<ActiveDatasetRelease> {
        let pointer = ActiveReleasePointer {
            dataset_id: self.dataset_id,
            release_id: self.release_id,
            previous_release_id: self.previous_release_id,
            record_version: self.record_version,
            activated_at: self.activated_at,
        };
        let release: DatasetRelease = decode(&self.canonical_json, "active dataset release")?;
        release.validate()?;
        ensure!(
            pointer.record_version > 0
                && self.pointer_id
                    == format!("{}:{}", scope.identity.tenant_id, pointer.dataset_id)
                && release.release_id == pointer.release_id
                && release.dataset_id == pointer.dataset_id
                && release.source_id == self.source_id
                && release.state == DatasetReleaseState::Active
                && release.record_version == self.release_version
                && release.version_label == self.version_label
                && release.source_digest_sha256 == self.source_digest_sha256
                && release.valid_from == self.valid_from
                && release.valid_until == self.valid_until,
            "active release document disagrees with selected pointer or metadata"
        );
        Ok(ActiveDatasetRelease { pointer, release })
    }
}

pub use crate::contract::ReleasePage;

impl MapCatalog {
    /// Public bounded selection; complete internal pointer inventories use
    /// `list_active_releases`. Filters and the lookahead limit execute in SQL.
    pub async fn active_releases(
        &self,
        scope: &MapAccessContext,
        request: &ListActiveDatasetReleasesRequest,
    ) -> Result<ListActiveDatasetReleasesOutput> {
        ensure!(
            (1..=100).contains(&request.limit),
            "limit must be within 1..=100"
        );
        let mut response = self
            .store()
            .client()
            .query(SELECT_ACTIVE)
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind((
                "dataset",
                request.dataset_id.as_ref().map(MapDatasetId::as_str),
            ))
            .bind((
                "source",
                request.source_id.as_ref().map(MapSourceId::as_str),
            ))
            .bind(("limit", request.limit + 1))
            .await?
            .check()?;
        let mut rows: Vec<serde_json::Value> = response.take(0)?;
        let truncated = rows.len() > request.limit as usize;
        rows.truncate(request.limit as usize);
        let releases = rows
            .into_iter()
            .map(|row| serde_json::from_value::<ActiveReleaseRow>(row)?.checked(scope))
            .collect::<Result<_>>()?;
        Ok(ListActiveDatasetReleasesOutput {
            releases,
            truncated,
        })
    }

    pub async fn release_in_dataset(
        &self,
        scope: &MapAccessContext,
        dataset: &MapDatasetId,
        release: &DatasetReleaseId,
    ) -> Result<Option<DatasetRelease>> {
        MapRepository::new(self.store().clone())
            .map_release_in_dataset(scope.identity.tenant_id, dataset, release)
            .await?
            .map(checked_release)
            .transpose()
    }

    pub async fn release_set(
        &self,
        scope: &MapAccessContext,
        releases: &std::collections::BTreeSet<DatasetReleaseId>,
    ) -> Result<Vec<DatasetRelease>> {
        MapRepository::new(self.store().clone())
            .map_release_set(scope.identity.tenant_id, releases)
            .await?
            .into_iter()
            .map(checked_release)
            .collect()
    }

    pub async fn releases_page(
        &self,
        scope: &MapAccessContext,
        dataset: Option<&MapDatasetId>,
        after: Option<&DatasetReleaseId>,
    ) -> Result<ReleasePage> {
        let mut rows = MapRepository::new(self.store().clone())
            .map_releases_page(scope.identity.tenant_id, dataset, after, PAGE_SIZE + 1)
            .await?;
        let more = rows.len() > PAGE_SIZE;
        rows.truncate(PAGE_SIZE);
        let next_cursor = if more {
            MapCatalogPage::Releases {
                dataset: dataset.cloned(),
                after: Some(
                    rows.last()
                        .expect("nonempty release page")
                        .release_key
                        .parse()?,
                ),
            }
            .cursor()
        } else {
            None
        };
        Ok(ReleasePage {
            items: rows
                .into_iter()
                .map(checked_release)
                .collect::<Result<_>>()?,
            limit: PAGE_SIZE,
            next_cursor,
        })
    }
}

pub(super) fn checked_release(
    row: crate::persistence::MapDatasetReleaseRecord,
) -> Result<DatasetRelease> {
    let release: DatasetRelease = decode(&row.canonical_json, "dataset release")?;
    release.validate()?;
    ensure!(
        row.id == surrealdb::types::RecordId::new("map_dataset_release", row.release_key.clone())
            && release.release_id.as_str() == row.release_key
            && release.dataset_id.as_str() == row.dataset_key
            && release.source_id.as_str() == row.source_key
            && release.source_digest_sha256 == row.source_digest_sha256
            && release.version_label == row.version_label
            && release.valid_from == row.valid_from
            && release.valid_until == row.valid_until
            && i64::try_from(release.record_version)? == row.record_version
            && super::release_state_to_store(release.state) == row.state,
        "release document disagrees with selected identity or metadata"
    );
    Ok(release)
}

#[cfg(test)]
mod tests {
    fn parse_cursor(
        dataset: Option<&MapDatasetId>,
        cursor: Option<&str>,
    ) -> Result<Option<DatasetReleaseId>> {
        let MapCatalogPage::Releases { after, .. } = (MapCatalogPage::Releases {
            dataset: dataset.cloned(),
            after: None,
        })
        .resume(cursor)?
        else {
            unreachable!("release selection")
        };
        Ok(after)
    }

    use super::*;

    use crate::contract::{DatasetLicense, DatasetReleaseState, Wgs84BoundingBox};

    fn key(prefix: &str, n: usize) -> String {
        format!("{prefix}-{n:08x}-0000-7000-8000-000000000000")
    }

    fn release(n: usize, dataset: &MapDatasetId) -> DatasetRelease {
        let now = chrono::Utc::now();
        DatasetRelease::new(crate::contract::DatasetReleaseValue {
            release_id: key("release", n).parse().unwrap(),
            dataset_id: dataset.clone(),
            source_id: key("source", 1).parse().unwrap(),
            version_label: "fixture".into(),
            source_digest_sha256: "a".repeat(64),
            coverage: Wgs84BoundingBox {
                west: -1.0,
                south: -1.0,
                east: 1.0,
                north: 1.0,
            },
            acquired_at: now,
            valid_from: now,
            valid_until: None,
            schema_version: 1,
            normalization_pipeline_version: "fixture".into(),
            routing_build_version: None,
            license: DatasetLicense {
                license_id: "fixture".into(),
                source_terms_uri: crate::contract::HttpsEndpoint::parse(
                    "https://fixture.local/terms",
                )
                .unwrap(),
                attribution: "Fixture".into(),
                redistribution_allowed: true,
                derivatives_allowed: true,
                offline_bundle_allowed: true,
                expires_at: None,
            },
            raw_artifact_uri: "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471"
                .parse()
                .unwrap(),
            normalized_artifact_uris: vec![
                "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4472"
                    .parse()
                    .unwrap(),
            ],
            quality_report_uri: "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4473"
                .parse()
                .unwrap(),
            supersedes_release_id: None,
            state: DatasetReleaseState::Staged,
            record_version: 1,
            updated_at: now,
        })
        .expect("admitted Map fixture")
    }

    #[tokio::test]
    async fn exact_release_set_preserves_lifecycle_and_excludes_unrelated_corruption() {
        tokio::time::timeout(std::time::Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
            let catalog = MapCatalog::new(db.a.clone());
            let owner = active_scope(&db.a, "exact-set").await;
            let foreign = active_scope(&db.a, "foreign-set").await;
            let mut expected = Vec::new();
            for (n, state) in [DatasetReleaseState::Staged, DatasetReleaseState::Retired, DatasetReleaseState::Quarantined, DatasetReleaseState::Active].into_iter().enumerate() {
                let mut value = release(n + 100, &key("dataset", n + 100).parse().unwrap());
                value.state = state;
                expected.push(catalog.create_release(&owner, value).await.unwrap());
            }
            let unrelated = catalog.create_release(&owner, release(200, &MapDatasetId::new())).await.unwrap();
            let excluded = catalog.create_release(&foreign, release(201, &MapDatasetId::new())).await.unwrap();
            for value in [&unrelated, &excluded] {
                db.a.client().query(include_str!("../queries/catalog/releases/active_release_limits_pointer_changes_and_document_agreement/statement_1.surql"))
                    .bind(("record", veoveo_platform_store::RecordId::new("map_dataset_release", value.release_id.as_str())))
                    .bind(("body", "{}".to_owned())).await.unwrap().check().unwrap();
            }
            let mut ids = expected.iter().map(|value| value.release_id.clone()).collect::<std::collections::BTreeSet<_>>();
            ids.insert(excluded.release_id.clone());
            assert_eq!(catalog.release_set(&owner, &ids).await.unwrap(), expected);
            assert!(catalog.release_set(&owner, &std::collections::BTreeSet::new()).await.unwrap().is_empty());
            ids.insert(unrelated.release_id.clone());
            assert!(catalog.release_set(&owner, &ids).await.is_err());
        }).await.expect("exact release set exceeded 90 seconds");
    }

    async fn active_scope(
        store: &veoveo_platform_store::PlatformStore,
        tenant: &str,
    ) -> MapAccessContext {
        MapAccessContext {
            identity: store
                .ensure_identity(
                    tenant,
                    "author",
                    "https://fixture.local",
                    "author",
                    veoveo_platform_store::PrincipalKind::Service,
                )
                .await
                .unwrap(),
        }
    }

    async fn activate_fixture(
        catalog: &MapCatalog,
        scope: &MapAccessContext,
        n: usize,
        source: usize,
    ) -> DatasetRelease {
        let mut value = release(n, &key("dataset", n).parse().unwrap());
        value.source_id = key("source", source).parse().unwrap();
        let value = catalog.create_release(scope, value).await.unwrap();
        catalog.activate_release(scope, value, None).await.unwrap()
    }

    #[tokio::test]
    async fn active_release_filters_precede_limits_and_denied_document_decode() {
        tokio::time::timeout(std::time::Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
            let writer = MapCatalog::new(db.a.clone());
            let reader = MapCatalog::new(db.b.clone());
            let scope = active_scope(&db.a, "active-selection").await;
            let foreign = active_scope(&db.a, "foreign").await;
            // Both rejected sets sort before the requested source/dataset and
            // exceed the maximum page size. Their documents cannot be decoded.
            for n in 0..105 {
                activate_fixture(&writer, &foreign, n, 1000).await;
                activate_fixture(&writer, &scope, n + 105, n + 105).await;
            }
            db.a.client()
                .query(include_str!("../queries/catalog/releases/active_release_filters_precede_limits_and_denied_document_decode/statement_1.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
            let selected = activate_fixture(&writer, &scope, 1000, 1000).await;
            for (source_id, dataset_id) in [
                (Some(selected.source_id.clone()), None),
                (None, Some(selected.dataset_id.clone())),
                (
                    Some(selected.source_id.clone()),
                    Some(selected.dataset_id.clone()),
                ),
            ] {
                let page = reader
                    .active_releases(
                        &scope,
                        &ListActiveDatasetReleasesRequest {
                            source_id,
                            dataset_id,
                            limit: 1,
                        },
                    )
                    .await
                    .unwrap();
                assert!(!page.truncated);
                assert_eq!(page.releases.len(), 1);
                assert_eq!(page.releases[0].release, selected);
            }
            let missing = reader
                .active_releases(
                    &scope,
                    &ListActiveDatasetReleasesRequest {
                        source_id: Some(key("source", 1).parse().unwrap()),
                        dataset_id: Some(selected.dataset_id.clone()),
                        limit: 100,
                    },
                )
                .await
                .unwrap();
            assert!(missing.releases.is_empty());
            assert!(!missing.truncated);
        })
        .await
        .expect("active release filtering exceeded 90 seconds");
    }

    #[tokio::test]
    async fn active_release_limits_pointer_changes_and_document_agreement() {
        tokio::time::timeout(std::time::Duration::from_secs(90), async {
            let db = crate::test_store::TestDb::with_modules(vec![crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap()).unwrap()]).await;
            let writer = MapCatalog::new(db.a.clone());
            let reader = MapCatalog::new(db.b.clone());
            let scope = active_scope(&db.a, "active-limits").await;
            let mut expected = Vec::new();
            for n in 0..105 {
                expected.push(activate_fixture(&writer, &scope, n, n).await);
            }
            for limit in [1, 100] {
                let page = reader.active_releases(&scope, &ListActiveDatasetReleasesRequest {
                    source_id: None, dataset_id: None, limit,
                }).await.unwrap();
                assert!(page.truncated);
                assert_eq!(page.releases.iter().map(|row| &row.release).collect::<Vec<_>>(),
                    expected[..limit as usize].iter().collect::<Vec<_>>());
            }
            for limit in [0, 101, u32::MAX] {
                assert!(reader.active_releases(&scope, &ListActiveDatasetReleasesRequest {
                    source_id: None, dataset_id: None, limit,
                }).await.is_err());
            }
            let previous = &expected[104];
            let mut next = release(1000, &previous.dataset_id);
            next.source_id = previous.source_id.clone();
            let next = writer.create_release(&scope, next).await.unwrap();
            let next = writer.activate_release(&scope, next, Some(1)).await.unwrap();
            let request = ListActiveDatasetReleasesRequest {
                source_id: Some(next.source_id.clone()), dataset_id: Some(next.dataset_id.clone()), limit: 1,
            };
            let page = reader.active_releases(&scope, &request).await.unwrap();
            assert!(!page.truncated);
            assert_eq!(page.releases[0].release, next);
            assert_eq!(page.releases[0].pointer.previous_release_id.as_ref(), Some(&previous.release_id));
            assert_eq!(page.releases[0].pointer.record_version, 2);
            let pointer_wire = serde_json::to_value(&page.releases[0].pointer).unwrap();
            for (current, retired) in [("datasetId", "dataset_id"), ("releaseId", "release_id"), ("previousReleaseId", "previous_release_id"), ("recordVersion", "record_version"), ("activatedAt", "activated_at")] {
                for mixed in [false, true] {
                    let mut bad = pointer_wire.clone();
                    let value = bad[current].clone();
                    if !mixed { bad.as_object_mut().unwrap().remove(current); }
                    bad[retired] = value;
                    assert!(serde_json::from_value::<ActiveReleasePointer>(bad).is_err(), "{retired} mixed={mixed}");
                }
            }


            // An admitted document must agree with the metadata that SQL selected.
            for field in ["releaseId", "datasetId", "sourceId", "recordVersion", "state"] {
                let mut body = serde_json::to_value(&next).unwrap();
                body[field] = match field {
                    "releaseId" => serde_json::json!(previous.release_id),
                    "datasetId" => serde_json::json!(key("dataset", 9000)),
                    "sourceId" => serde_json::json!(key("source", 9000)),
                    "recordVersion" => serde_json::json!(99),
                    "state" => serde_json::json!("staged"),
                    _ => unreachable!(),
                };
                db.a.client().query(include_str!("../queries/catalog/releases/active_release_limits_pointer_changes_and_document_agreement/statement_1.surql"))
                    .bind(("record", veoveo_platform_store::RecordId::new("map_dataset_release", next.release_id.as_str())))
                    .bind(("body", serde_json::to_string(&body).unwrap()))
                    .await.unwrap().check().unwrap();
                let error = reader.active_releases(&scope, &request).await.unwrap_err();
                assert!(error.to_string().contains("disagrees"), "{field}: {error}");
            }
            // Relationships are admission predicates too, before retained-body decoding.
            for mutation in [
                include_str!("../queries/catalog/releases/mutation_01.surql"),
                include_str!("../queries/catalog/releases/mutation_02.surql"),
                include_str!("../queries/catalog/releases/mutation_03.surql"),
                include_str!("../queries/catalog/releases/mutation_04.surql"),
            ] {
                db.a.client().query(mutation)
                    .bind(("record", veoveo_platform_store::RecordId::new("map_dataset_release", next.release_id.as_str())))
                    .await.unwrap().check().unwrap();
                assert!(reader.active_releases(&scope, &request).await.unwrap().releases.is_empty());
                db.a.client().query(include_str!("../queries/catalog/releases/active_release_limits_pointer_changes_and_document_agreement/statement_2.surql"))
                    .bind(("record", veoveo_platform_store::RecordId::new("map_dataset_release", next.release_id.as_str())))
                    .bind(("tenant", scope.identity.tenant_id.record_id()))
                    .bind(("dataset", next.dataset_id.as_str()))
                    .bind(("release", next.release_id.as_str()))
                    .await.unwrap().check().unwrap();
            }
        }).await.expect("active release page qualification exceeded 90 seconds");
    }

    #[test]
    fn artifact_references_require_neutral_plane_addresses() {
        let dataset: MapDatasetId = key("dataset", 1).parse().unwrap();
        let valid = release(1, &dataset);
        valid.validate().unwrap();
        for scheme in ["map", "artifact"] {
            let scheme = veoveo_types::ResourceScheme::parse(scheme).unwrap();
            let presented = veoveo_artifact_contract::ArtifactUri::presented(
                &scheme,
                valid.raw_artifact_uri.artifact_id(),
            );
            // Even a well-formed presentation under the Artifact scheme is not
            // the neutral plane variant required by the release contract.
            for field in 0..3 {
                let mut candidate = valid.clone();
                match field {
                    0 => candidate.raw_artifact_uri = presented.clone(),
                    1 => candidate.normalized_artifact_uris = vec![presented.clone()],
                    2 => candidate.quality_report_uri = presented.clone(),
                    _ => unreachable!(),
                }
                assert_eq!(
                    candidate.validate(),
                    Err(crate::contract::SourceContractError::InvalidArtifactUri)
                );
            }
        }
    }

    #[tokio::test]
    async fn release_pages_apply_tenant_and_dataset_before_limits_and_support_exact_reads() {
        tokio::time::timeout(std::time::Duration::from_secs(120), qualify())
            .await
            .expect("release page qualification exceeded 120 seconds");
    }

    async fn qualify() {
        let db = crate::test_store::TestDb::with_modules(vec![
            crate::schema::module_setup(crate::test_store::module_lanes::execution("map").unwrap())
                .unwrap(),
        ])
        .await;
        let writer = MapCatalog::new(db.a.clone());
        let reader = MapCatalog::new(db.b.clone());
        let scope = MapAccessContext {
            identity: db
                .a
                .ensure_identity(
                    "release-pages",
                    "author",
                    "https://fixture.local",
                    "author",
                    veoveo_platform_store::PrincipalKind::Service,
                )
                .await
                .unwrap(),
        };
        let foreign = MapAccessContext {
            identity: db
                .a
                .ensure_identity(
                    "foreign",
                    "author",
                    "https://fixture.local",
                    "author",
                    veoveo_platform_store::PrincipalKind::Service,
                )
                .await
                .unwrap(),
        };
        let dataset: MapDatasetId = key("dataset", 1).parse().unwrap();
        let other: MapDatasetId = key("dataset", 2).parse().unwrap();
        // Both sets sort ahead of the selected dataset; a limit before admission loses every target row.
        for n in 0..110 {
            writer
                .create_release(&foreign, release(n, &dataset))
                .await
                .unwrap();
            writer
                .create_release(&scope, release(n + 110, &other))
                .await
                .unwrap();
        }
        let mut expected = Vec::new();
        for n in 1000..1125 {
            expected.push(
                writer
                    .create_release(&scope, release(n, &dataset))
                    .await
                    .unwrap(),
            );
        }
        let first = reader
            .releases_page(&scope, Some(&dataset), None)
            .await
            .unwrap();
        assert_eq!(first.limit, 100);
        assert_eq!(first.items, expected[..100]);
        let cursor = first.next_cursor.unwrap();
        assert!(parse_cursor(None, Some(&cursor)).is_err());
        assert!(parse_cursor(Some(&other), Some(&cursor)).is_err());
        let after = parse_cursor(Some(&dataset), Some(&cursor))
            .unwrap()
            .unwrap();
        let second = reader
            .releases_page(&scope, Some(&dataset), Some(&after))
            .await
            .unwrap();
        assert_eq!(second.items, expected[100..]);
        assert!(second.next_cursor.is_none());
        assert!(
            reader
                .releases_page(&scope, Some(&dataset), Some(&expected[124].release_id))
                .await
                .unwrap()
                .items
                .is_empty()
        );
        // Walk the root through three pages; it includes both owned datasets and no foreign releases.
        let mut after = None;
        let mut all = Vec::new();
        for _ in 0..3 {
            let page = reader
                .releases_page(&scope, None, after.as_ref())
                .await
                .unwrap();
            all.extend(page.items);
            after = parse_cursor(None, page.next_cursor.as_deref()).unwrap();
            if after.is_none() {
                break;
            }
        }
        assert!(after.is_none());
        assert_eq!(all.len(), 235);
        assert!(
            all.windows(2)
                .all(|pair| pair[0].release_id < pair[1].release_id)
        );
        assert_eq!(
            all.iter().filter(|row| row.dataset_id == dataset).count(),
            125
        );
        let target = &expected[124];
        assert_eq!(
            reader
                .release_in_dataset(&scope, &dataset, &target.release_id)
                .await
                .unwrap(),
            Some(target.clone())
        );
        assert!(
            reader
                .release_in_dataset(&scope, &other, &target.release_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .release_in_dataset(&foreign, &dataset, &target.release_id)
                .await
                .unwrap()
                .is_none()
        );
        let missing = key("release", 9000).parse().unwrap();
        assert!(
            reader
                .release_in_dataset(&scope, &dataset, &missing)
                .await
                .unwrap()
                .is_none()
        );
        for limit in [0, 102, usize::MAX] {
            assert!(
                MapRepository::new(db.b.clone())
                    .map_releases_page(scope.identity.tenant_id, Some(&dataset), None, limit)
                    .await
                    .is_err()
            );
        }
        assert!(MapDatasetId::parse("dataset-invalid").is_err());
        assert!(crate::contract::DatasetReleaseId::parse("release-invalid").is_err());
    }
}
