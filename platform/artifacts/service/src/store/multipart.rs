//! Restartable native multipart operations and bounded part/whole-object integrity.

use super::*;
use object_store::{PutMultipartOptions, PutPayload, PutPayloadMut, multipart::PartId};
use std::num::NonZeroU32;
use veoveo_mcp_contract::UploadSha256;

const COALESCE_BYTES: usize = 64 * 1024;

/// Constructed only after checking the complete bounded request body. Tiny HTTP
/// chunks are coalesced so metadata allocations cannot grow independently of bytes.
pub struct VerifiedUploadPayload {
    payload: PutPayload,
}

impl VerifiedUploadPayload {
    pub async fn read(
        mut stream: BlobStream,
        expected_len: u64,
        expected_sha256: &UploadSha256,
    ) -> Result<Self, BlobStoreError> {
        let mut payload = PutPayloadMut::new().with_block_size(COALESCE_BYTES);
        let mut digest = Sha256::new();
        let mut byte_len = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            byte_len =
                byte_len
                    .checked_add(chunk.len() as u64)
                    .ok_or(BlobStoreError::TooLarge {
                        actual: u64::MAX,
                        limit: expected_len,
                    })?;
            if byte_len > expected_len {
                return Err(BlobStoreError::TooLarge {
                    actual: byte_len,
                    limit: expected_len,
                });
            }
            if chunk.is_empty() {
                continue;
            }
            digest.update(&chunk);
            if chunk.len() >= COALESCE_BYTES {
                payload.push(chunk);
            } else {
                payload.extend_from_slice(&chunk);
            }
        }
        let sha256 = hex::encode(digest.finalize());
        if byte_len != expected_len || sha256 != expected_sha256.as_str() {
            return Err(BlobStoreError::VerificationFailed {
                actual_byte_len: byte_len,
                expected_byte_len: expected_len,
                actual_sha256: sha256,
                expected_sha256: expected_sha256.as_str().to_owned(),
            });
        }
        Ok(Self {
            payload: payload.freeze(),
        })
    }
}

impl ArtifactObjectStore {
    fn multipart_backend(
        &self,
    ) -> Result<&dyn object_store::multipart::MultipartStore, BlobStoreError> {
        self.multipart.as_deref().ok_or_else(|| {
            BlobStoreError::Backend("durable multipart storage is unavailable".into())
        })
    }

    pub async fn create_upload(
        &self,
        object_key: &str,
        mime_type: &str,
    ) -> Result<String, BlobStoreError> {
        self.multipart_backend()?
            .create_multipart_opts(
                &Self::path(object_key)?,
                PutMultipartOptions {
                    attributes: Attributes::from_iter([(
                        Attribute::ContentType,
                        mime_type.to_owned(),
                    )]),
                    ..Default::default()
                },
            )
            .await
            .map_err(map_store_error)
    }

    pub async fn put_upload_part(
        &self,
        object_key: &str,
        multipart_id: &str,
        part_number: NonZeroU32,
        payload: VerifiedUploadPayload,
    ) -> Result<String, BlobStoreError> {
        self.multipart_backend()?
            .put_part(
                &Self::path(object_key)?,
                &multipart_id.to_owned(),
                usize::try_from(part_number.get() - 1)
                    .map_err(|_| BlobStoreError::Backend("invalid part number".into()))?,
                payload.payload,
            )
            .await
            .map(|part| part.content_id)
            .map_err(map_store_error)
    }

    pub async fn upload_object_len(&self, object_key: &str) -> Result<Option<u64>, BlobStoreError> {
        match self.inner.head(&Self::path(object_key)?).await {
            Ok(meta) => Ok(Some(meta.size)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(error) => Err(map_store_error(error)),
        }
    }

    /// The caller freezes ordered ledger receipts before invoking this operation.
    /// A matching object at the session's unique key recovers a lost completion ack;
    /// publication still requires a separate whole-file verification pass.
    pub async fn complete_upload(
        &self,
        object_key: &str,
        multipart_id: &str,
        content_ids: Vec<String>,
        expected_len: u64,
    ) -> Result<(), BlobStoreError> {
        if self.upload_object_len(object_key).await? == Some(expected_len) {
            return Ok(());
        }
        let result = self
            .multipart_backend()?
            .complete_multipart(
                &Self::path(object_key)?,
                &multipart_id.to_owned(),
                content_ids
                    .into_iter()
                    .map(|content_id| PartId { content_id })
                    .collect(),
            )
            .await;
        if let Err(error) = result {
            if self.upload_object_len(object_key).await? != Some(expected_len) {
                return Err(map_store_error(error));
            }
        }
        if self.upload_object_len(object_key).await? != Some(expected_len) {
            return Err(BlobStoreError::Backend(
                "multipart completion length differs from sealed manifest".into(),
            ));
        }
        Ok(())
    }

    pub async fn abort_upload(
        &self,
        object_key: &str,
        multipart_id: &str,
    ) -> Result<(), BlobStoreError> {
        match self
            .multipart_backend()?
            .abort_multipart(&Self::path(object_key)?, &multipart_id.to_owned())
            .await
        {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) if error.to_string().contains("NoSuchUpload") => Ok(()),
            Err(error) => Err(map_store_error(error)),
        }
    }

    pub async fn verify_upload(
        &self,
        object_key: &str,
        expected_len: u64,
        expected_sha256: Option<&UploadSha256>,
    ) -> Result<VerifiedBlob, BlobStoreError> {
        let mut stream = self.stream(object_key, None).await?;
        let mut digest = Sha256::new();
        let mut byte_len = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            byte_len =
                byte_len
                    .checked_add(chunk.len() as u64)
                    .ok_or(BlobStoreError::TooLarge {
                        actual: u64::MAX,
                        limit: expected_len,
                    })?;
            if byte_len > expected_len {
                return Err(BlobStoreError::TooLarge {
                    actual: byte_len,
                    limit: expected_len,
                });
            }
            digest.update(&chunk);
        }
        let sha256 = hex::encode(digest.finalize());
        if byte_len != expected_len
            || expected_sha256.is_some_and(|expected| expected.as_str() != sha256)
        {
            return Err(BlobStoreError::VerificationFailed {
                actual_byte_len: byte_len,
                expected_byte_len: expected_len,
                actual_sha256: sha256,
                expected_sha256: expected_sha256
                    .map(|sha| sha.as_str().to_owned())
                    .unwrap_or_default(),
            });
        }
        Ok(VerifiedBlob { byte_len, sha256 })
    }
}

#[cfg(test)]
mod tests;
