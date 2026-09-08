use super::*;

async fn payload(bytes: &'static [u8]) -> VerifiedUploadPayload {
    let sha = UploadSha256::parse(hex::encode(Sha256::digest(bytes))).unwrap();
    VerifiedUploadPayload::read(
        Box::pin(futures::stream::iter([Ok(Bytes::from_static(bytes))])),
        bytes.len() as u64,
        &sha,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn restored_multipart_handle_and_receipts_verify_whole_file_and_recover_completion() {
    let backend = Arc::new(object_store::memory::InMemory::new());
    let first = ArtifactObjectStore::with_multipart(backend.clone());
    let key = "uploads/opaque";
    let handle = first
        .create_upload(key, "application/octet-stream")
        .await
        .unwrap();
    let part = first
        .put_upload_part(
            key,
            &handle,
            NonZeroU32::new(1).unwrap(),
            payload(b"one").await,
        )
        .await
        .unwrap();
    drop(first);
    let restored = ArtifactObjectStore::with_multipart(backend);
    let last = restored
        .put_upload_part(
            key,
            &handle,
            NonZeroU32::new(2).unwrap(),
            payload(b"two").await,
        )
        .await
        .unwrap();
    restored
        .complete_upload(key, &handle, vec![part, last], 6)
        .await
        .unwrap();
    // A lost completion response never requires client retransmission.
    restored
        .complete_upload(key, &handle, vec![], 6)
        .await
        .unwrap();
    let verified = restored.verify_upload(key, 6, None).await.unwrap();
    assert_eq!(verified.sha256, hex::encode(Sha256::digest(b"onetwo")));
    assert_eq!(verified.byte_len, 6);
    assert!(restored.verify_upload(key, 5, None).await.is_err());
    let wrong = UploadSha256::parse("a".repeat(64)).unwrap();
    assert!(restored.verify_upload(key, 6, Some(&wrong)).await.is_err());
}

#[tokio::test]
async fn untrusted_part_bodies_are_size_and_digest_checked_before_storage() {
    let sha = UploadSha256::parse(hex::encode(Sha256::digest(b"ok"))).unwrap();
    for bytes in [b"o".as_slice(), b"bad".as_slice(), b"no".as_slice()] {
        assert!(
            VerifiedUploadPayload::read(
                Box::pin(futures::stream::iter([Ok(Bytes::copy_from_slice(bytes))])),
                2,
                &sha
            )
            .await
            .is_err()
        );
    }
    let data = vec![1_u8; 128 * 1024];
    let sha = UploadSha256::parse(hex::encode(Sha256::digest(&data))).unwrap();
    let stream = Box::pin(futures::stream::iter(
        data.into_iter().map(|b| Ok(Bytes::from(vec![b]))),
    ));
    let body = VerifiedUploadPayload::read(stream, 128 * 1024, &sha)
        .await
        .unwrap();
    assert_eq!(
        body.payload.iter().count(),
        2,
        "tiny frames grew chunk metadata without a byte bound"
    );
}
