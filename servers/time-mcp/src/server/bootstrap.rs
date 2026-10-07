//! Packaged authority identities bind their family and immutable source bytes.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use veoveo_types::Sha256Digest;

use crate::contract::{
    AuthorityDatasetKind, AuthorityReleaseId, TimeAuthorityReference, TimeAuthorityReleaseUri,
    TimeAuthoritySource,
};

pub(super) async fn reference(
    dataset_kind: AuthorityDatasetKind,
    source_path: &std::path::Path,
) -> Result<TimeAuthorityReference> {
    let source = tokio::fs::read(source_path).await.with_context(|| {
        format!(
            "reading bootstrap authority source {}",
            source_path.display()
        )
    })?;
    let digest = Sha256Digest::from_hex(hex::encode(Sha256::digest(source)))?;
    let family = match dataset_kind {
        AuthorityDatasetKind::Tzdb => "tzdb",
        AuthorityDatasetKind::LeapSeconds => "leaps",
    };
    let release_id =
        AuthorityReleaseId::parse(format!("time-release-bootstrap-{family}-{}", digest.hex()))
            .map_err(anyhow::Error::msg)?;
    let version = digest.to_string();
    Ok(TimeAuthorityReference::new(
        TimeAuthorityReleaseUri::bootstrap(&release_id),
        dataset_kind,
        TimeAuthoritySource::Bootstrap {},
        digest,
        version,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn packaged_authority_identity_survives_relocation_and_changes_with_content_or_family() {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let root = tempfile::TempDir::new().unwrap();
            let original = root.path().join("original");
            let relocated = root.path().join("relocated");
            tokio::fs::write(&original, b"authority fixture one")
                .await
                .unwrap();
            tokio::fs::copy(&original, &relocated).await.unwrap();
            let first = reference(AuthorityDatasetKind::Tzdb, &original)
                .await
                .unwrap();
            assert_eq!(
                first,
                reference(AuthorityDatasetKind::Tzdb, &relocated)
                    .await
                    .unwrap()
            );
            let other_family = reference(AuthorityDatasetKind::LeapSeconds, &original)
                .await
                .unwrap();
            assert_ne!(first.release_id(), other_family.release_id());
            assert_eq!(first.source_digest(), other_family.source_digest());
            tokio::fs::write(&original, b"authority fixture two")
                .await
                .unwrap();
            let changed = reference(AuthorityDatasetKind::Tzdb, &original)
                .await
                .unwrap();
            assert_ne!(first.release_id(), changed.release_id());
            assert_ne!(first.release_uri(), changed.release_uri());
            assert_ne!(first.source_digest(), changed.source_digest());
            assert_eq!(
                serde_json::from_value::<TimeAuthorityReference>(
                    serde_json::to_value(&changed).unwrap()
                )
                .unwrap(),
                changed
            );
        })
        .await
        .expect("bootstrap identity qualification exceeded ten seconds");
    }
}
