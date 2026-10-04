//! Dataset release pages and exact reads, backed by tenant/parent-scoped SQL.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{MapAccessContext, MapCatalog, decode};
use crate::contract::{
    ActiveDatasetRelease, ActiveReleasePointer, DatasetRelease, DatasetReleaseId,
    DatasetReleaseState, ListActiveDatasetReleasesOutput, ListActiveDatasetReleasesRequest,
    MapCatalogPage, MapDatasetId, MapSourceId,
};

pub const PAGE_SIZE: usize = 100;

const SELECT_ACTIVE: &str = "SELECT record::id(id) AS pointer_id,
    dataset_key AS dataset_id, release_key AS release_id,
    previous_release_key AS previous_release_id, record_version, activated_at,
    release.source_key AS source_id, release.record_version AS release_version,
    release.version_label AS version_label,
    release.source_digest_sha256 AS source_digest_sha256,
    release.valid_from AS valid_from, release.valid_until AS valid_until,
    release.canonical_json AS canonical_json
FROM (
    SELECT *, type::record('map_dataset_release', release_key) AS release
    FROM map_active_release
    WHERE tenant = $tenant AND ($dataset = NONE OR dataset_key = $dataset)
) WHERE release.tenant = $tenant AND release.release_key = release_key
    AND release.dataset_key = dataset_key AND release.state = 'active'
    AND ($source = NONE OR release.source_key = $source)
ORDER BY dataset_id ASC LIMIT $limit TIMEOUT 5s;";

#[derive(Deserialize)]
struct ActiveReleaseRow {
    pointer_id: String,
    #[serde(flatten)]
    pointer: ActiveReleasePointer,
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
        let release: DatasetRelease = decode(&self.canonical_json, "active dataset release")?;
        release.validate()?;
        ensure!(
            self.pointer.record_version > 0
                && self.pointer_id
                    == format!("{}:{}", scope.identity.tenant_id, self.pointer.dataset_id)
                && release.release_id == self.pointer.release_id
                && release.dataset_id == self.pointer.dataset_id
                && release.source_id == self.source_id
                && release.state == DatasetReleaseState::Active
                && release.record_version == self.release_version
                && release.version_label == self.version_label
                && release.source_digest_sha256 == self.source_digest_sha256
                && release.valid_from == self.valid_from
                && release.valid_until == self.valid_until,
            "active release document disagrees with selected pointer or metadata"
        );
        Ok(ActiveDatasetRelease {
            pointer: self.pointer,
            release,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct ReleasePage {
    pub items: Vec<DatasetRelease>,
    pub limit: usize,
    pub next_cursor: Option<String>,
}

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
        self.store()
            .map_release_in_dataset(scope.identity.tenant_id, dataset.as_str(), release.as_str())
            .await?
            .map(checked_release)
            .transpose()
    }

    pub async fn releases_page(
        &self,
        scope: &MapAccessContext,
        dataset: Option<&MapDatasetId>,
        after: Option<&DatasetReleaseId>,
    ) -> Result<ReleasePage> {
        let mut rows = self
            .store()
            .map_releases_page(
                scope.identity.tenant_id,
                dataset.map(MapDatasetId::as_str),
                after.map(DatasetReleaseId::as_str),
                PAGE_SIZE + 1,
            )
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

fn checked_release(row: veoveo_platform_store::MapDatasetReleaseRecord) -> Result<DatasetRelease> {
    let release: DatasetRelease = decode(&row.canonical_json, "dataset release")?;
    release.validate()?;
    ensure!(
        release.release_id.as_str() == row.release_key
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
        DatasetRelease {
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
        }
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
            let db = crate::test_store::TestDb::new().await;
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
                .query("UPDATE map_dataset_release SET canonical_json = '{' RETURN NONE;")
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
            let db = crate::test_store::TestDb::new().await;
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

            // An admitted document must agree with the metadata that SQL selected.
            for field in ["release_id", "dataset_id", "source_id", "record_version", "state"] {
                let mut body = serde_json::to_value(&next).unwrap();
                body[field] = match field {
                    "release_id" => serde_json::json!(previous.release_id),
                    "dataset_id" => serde_json::json!(key("dataset", 9000)),
                    "source_id" => serde_json::json!(key("source", 9000)),
                    "record_version" => serde_json::json!(99),
                    "state" => serde_json::json!("staged"),
                    _ => unreachable!(),
                };
                db.a.client().query("UPDATE ONLY $record SET canonical_json = $body RETURN NONE;")
                    .bind(("record", veoveo_platform_store::RecordId::new("map_dataset_release", next.release_id.as_str())))
                    .bind(("body", serde_json::to_string(&body).unwrap()))
                    .await.unwrap().check().unwrap();
                let error = reader.active_releases(&scope, &request).await.unwrap_err();
                assert!(error.to_string().contains("disagrees"), "{field}: {error}");
            }
            // Relationships are admission predicates too, before retained-body decoding.
            for mutation in [
                "tenant = tenant:other",
                "dataset_key = 'dataset-ffffffff-0000-7000-8000-000000000000'",
                "release_key = 'release-ffffffff-0000-7000-8000-000000000000'",
                "state = 'retired'",
            ] {
                db.a.client().query(format!("UPDATE ONLY $record SET {mutation}, canonical_json = '{{' RETURN NONE;"))
                    .bind(("record", veoveo_platform_store::RecordId::new("map_dataset_release", next.release_id.as_str())))
                    .await.unwrap().check().unwrap();
                assert!(reader.active_releases(&scope, &request).await.unwrap().releases.is_empty());
                db.a.client().query("UPDATE ONLY $record SET tenant = $tenant, dataset_key = $dataset, release_key = $release, state = 'active' RETURN NONE;")
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
        let db = crate::test_store::TestDb::new().await;
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
                db.b.map_releases_page(
                    scope.identity.tenant_id,
                    Some(dataset.as_str()),
                    None,
                    limit
                )
                .await
                .is_err()
            );
        }
        assert!(
            db.b.map_releases_page(scope.identity.tenant_id, Some("dataset-invalid"), None, 1)
                .await
                .is_err()
        );
        assert!(
            db.b.map_releases_page(scope.identity.tenant_id, None, Some("release-invalid"), 1)
                .await
                .is_err()
        );
    }
}
