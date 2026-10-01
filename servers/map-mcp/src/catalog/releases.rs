//! Dataset release pages and exact reads, backed by tenant/parent-scoped SQL.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

use super::{MapAccessContext, MapCatalog, decode};
use crate::contract::{DatasetRelease, DatasetReleaseId, MapDatasetId};

pub const PAGE_SIZE: usize = 100;

#[derive(Debug, Serialize)]
pub struct ReleasePage {
    pub items: Vec<DatasetRelease>,
    pub limit: usize,
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    collection: String,
    dataset: Option<MapDatasetId>,
    after: DatasetReleaseId,
}

pub fn parse_cursor(
    dataset: Option<&MapDatasetId>,
    cursor: Option<&str>,
) -> Result<Option<DatasetReleaseId>> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    ensure!(
        !cursor.is_empty() && cursor.len() <= 2048,
        "invalid release cursor"
    );
    let cursor: Cursor = serde_json::from_slice(&hex::decode(cursor)?)?;
    ensure!(
        cursor.version == 1
            && cursor.collection == "releases"
            && cursor.dataset.as_ref() == dataset,
        "release cursor belongs to another collection or version"
    );
    Ok(Some(cursor.after))
}

impl MapCatalog {
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
            Some(hex::encode(serde_json::to_vec(&Cursor {
                version: 1,
                collection: "releases".into(),
                dataset: dataset.cloned(),
                after: rows
                    .last()
                    .expect("nonempty release page")
                    .release_key
                    .parse()?,
            })?))
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

    #[test]
    fn artifact_references_require_neutral_plane_addresses() {
        let dataset: MapDatasetId = key("dataset", 1).parse().unwrap();
        let valid = release(1, &dataset);
        valid.validate().unwrap();
        for scheme in ["map", "artifact"] {
            let scheme = veoveo_types::ResourceScheme::new(scheme).unwrap();
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

    #[test]
    fn release_cursors_bind_the_dataset_and_validate_the_key() {
        let dataset: MapDatasetId = format!("dataset-{}", uuid::Uuid::now_v7()).parse().unwrap();
        let after: DatasetReleaseId = format!("release-{}", uuid::Uuid::now_v7()).parse().unwrap();
        let cursor = hex::encode(
            serde_json::to_vec(&Cursor {
                version: 1,
                collection: "releases".into(),
                dataset: Some(dataset.clone()),
                after: after.clone(),
            })
            .unwrap(),
        );
        assert_eq!(
            parse_cursor(Some(&dataset), Some(&cursor)).unwrap(),
            Some(after)
        );
        assert!(parse_cursor(None, Some(&cursor)).is_err());
        let other = format!("dataset-{}", uuid::Uuid::now_v7()).parse().unwrap();
        assert!(parse_cursor(Some(&other), Some(&cursor)).is_err());
        for invalid in ["".into(), "gg".into(), "a".repeat(2049), hex::encode(br#"{"version":1,"collection":"releases","dataset":null,"after":"release-invalid"}"#)] {
            assert!(parse_cursor(None, Some(&invalid)).is_err());
        }
        assert!(parse_cursor(None, None).unwrap().is_none());
    }
}
