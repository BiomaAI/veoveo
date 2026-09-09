//! Native installed-S3 qualification. Owns its port-forward and unique test objects.

mod s3_upload_fixture;

use anyhow::{Context, Result, ensure};
use axum::body::Bytes;
use sha2::{Digest, Sha256};
use std::num::NonZeroU32;
use veoveo_artifact_service::{
    ArtifactObjectStore, BlobStore,
    store::multipart::{StoredUploadPart, VerifiedUploadPayload},
};
use veoveo_mcp_contract::UploadSha256;

async fn payload(bytes: &[u8]) -> Result<VerifiedUploadPayload> {
    let sha = UploadSha256::parse(hex::encode(Sha256::digest(bytes)))?;
    Ok(VerifiedUploadPayload::read(
        Box::pin(futures::stream::iter(
            bytes
                .chunks(64 * 1024)
                .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
                .collect::<Vec<_>>(),
        )),
        bytes.len() as u64,
        &sha,
    )
    .await?)
}

async fn qualify(config: &veoveo_artifact_service::ObjectStoreConfig, key: &str) -> Result<()> {
    let first = config.build()?;
    let orphan = first.create_upload(key, "application/octet-stream").await?;
    // Simulate the missing ledger handle after an acknowledged remote creation.
    first.remove_upload_orphans(key, None).await?;
    ensure!(
        first
            .put_upload_part(
                key,
                &orphan,
                NonZeroU32::new(1).context("part number")?,
                payload(b"abandoned").await?
            )
            .await
            .is_err(),
        "orphan upload remained writable after reconciliation"
    );
    let multipart = first.create_upload(key, "application/octet-stream").await?;
    let bytes = vec![37_u8; 16 * 1024 * 1024];
    let mut whole = Sha256::new();
    whole.update(&bytes);
    let one = first
        .put_upload_part(
            key,
            &multipart,
            NonZeroU32::new(1).context("part number")?,
            payload(&bytes).await?,
        )
        .await?;
    drop(first);
    let restored = config.build()?;
    // Keep the committed provider handle while removing a second orphan at the
    // same unique key. The accepted first part must survive that cleanup.
    let _second_orphan = restored
        .create_upload(key, "application/octet-stream")
        .await?;
    restored
        .remove_upload_orphans(key, Some(&multipart))
        .await?;
    whole.update(&bytes);
    let _lost_ack = restored
        .put_upload_part(
            key,
            &multipart,
            NonZeroU32::new(2).context("part number")?,
            payload(&bytes).await?,
        )
        .await?;
    let tail = b"verified final part";
    whole.update(tail);
    let three = restored
        .put_upload_part(
            key,
            &multipart,
            NonZeroU32::new(3).context("part number")?,
            payload(tail).await?,
        )
        .await?;
    let parts = restored
        .reconciled_upload_parts(
            key,
            &multipart,
            vec![
                StoredUploadPart {
                    number: NonZeroU32::new(1).context("part number")?,
                    byte_len: bytes.len() as u64,
                    content_id: one,
                },
                StoredUploadPart {
                    number: NonZeroU32::new(2).context("part number")?,
                    byte_len: bytes.len() as u64,
                    content_id: "lost-response".into(),
                },
                StoredUploadPart {
                    number: NonZeroU32::new(3).context("part number")?,
                    byte_len: tail.len() as u64,
                    content_id: three,
                },
            ],
        )
        .await?;
    ensure!(parts.len() == 3, "part reconciliation lost accepted bytes");
    let total = bytes.len() as u64 * 2 + tail.len() as u64;
    restored
        .complete_upload(key, &multipart, parts, total)
        .await?;
    // Completion response loss recovers the immutable object at this key.
    restored
        .complete_upload(key, &multipart, Vec::new(), total)
        .await?;
    let expected = UploadSha256::parse(hex::encode(whole.finalize()))?;
    let verified = restored.verify_upload(key, total, Some(&expected)).await?;
    ensure!(
        verified.byte_len == total && verified.sha256 == expected.as_str(),
        "whole-file identity changed after recovery"
    );
    restored.remove_upload_orphans(key, None).await?;
    ensure!(
        restored.upload_object_len(key).await? == Some(total),
        "orphan cleanup deleted a completed object"
    );
    println!(
        "S3 accepted and verified {total} bytes across restored handles, reconciled parts, and repeated completion"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires VEOVEO_UPLOAD_S3_CONTEXT; owns a RustFS port-forward and isolated object prefix"]
async fn installed_s3_multipart_survives_uncertain_acknowledgements_and_preserves_completed_bytes()
-> Result<()> {
    let fixture = s3_upload_fixture::Fixture::start().await?;
    let key = format!("tenants/upload-acceptance/uploads/{}", uuid::Uuid::now_v7());
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        qualify(&fixture.config, &key),
    )
    .await
    .context("S3 transfer qualification exceeded its deadline")
    .and_then(|result| result);
    let cleanup: ArtifactObjectStore = fixture.config.build()?;
    cleanup
        .remove_upload_orphans(&key, None)
        .await
        .context("removing acceptance multipart uploads")?;
    cleanup
        .delete(&key)
        .await
        .context("removing acceptance object")?;
    ensure!(
        cleanup.upload_object_len(&key).await?.is_none(),
        "acceptance object was retained"
    );
    result
}
