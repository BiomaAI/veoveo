//! Artifact byte storage and controlled bulk-download delivery.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use futures::{Stream, StreamExt};
use object_store::signer::Signer;
use object_store::{Attribute, Attributes, ObjectStore, ObjectStoreExt, PutPayload, path::Path};

pub const SIGNED_DOWNLOAD_TTL: Duration = Duration::from_secs(60);

pub type BlobStream = Pin<Box<dyn Stream<Item = Result<Bytes, BlobStoreError>> + Send + 'static>>;

pub enum BlobDownload {
    SignedRedirect(String),
    Stream(BlobStream),
}

/// Storage is addressed only by opaque internal object keys. Public callers
/// never supply a key and never receive one in artifact metadata.
pub trait BlobStore: Send + Sync {
    fn put(
        &self,
        object_key: &str,
        bytes: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(), BlobStoreError>> + Send;

    fn get_bounded(
        &self,
        object_key: &str,
        max_bytes: u64,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, BlobStoreError>> + Send;

    fn download(
        &self,
        object_key: &str,
    ) -> impl std::future::Future<Output = Result<BlobDownload, BlobStoreError>> + Send;

    fn delete(
        &self,
        object_key: &str,
    ) -> impl std::future::Future<Output = Result<(), BlobStoreError>> + Send;
}

#[derive(Debug)]
pub enum BlobStoreError {
    NotFound,
    TooLarge { actual: u64, limit: u64 },
    Backend(String),
}

impl std::fmt::Display for BlobStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("artifact bytes not found"),
            Self::TooLarge { actual, limit } => {
                write!(
                    formatter,
                    "artifact is {actual} bytes; bounded read limit is {limit}"
                )
            }
            Self::Backend(message) => write!(formatter, "object store error: {message}"),
        }
    }
}

impl std::error::Error for BlobStoreError {}

/// Object-store implementation. S3-compatible stores provide a signer;
/// filesystem/memory stores stream the object in bounded chunks.
#[derive(Clone)]
pub struct ArtifactObjectStore {
    inner: Arc<dyn ObjectStore>,
    signer: Option<Arc<dyn Signer>>,
}

impl ArtifactObjectStore {
    pub fn new(inner: Arc<dyn ObjectStore>) -> Self {
        Self {
            inner,
            signer: None,
        }
    }

    pub fn with_signer(inner: Arc<dyn ObjectStore>, signer: Arc<dyn Signer>) -> Self {
        Self {
            inner,
            signer: Some(signer),
        }
    }

    fn path(object_key: &str) -> Result<Path, BlobStoreError> {
        Path::parse(object_key).map_err(|error| BlobStoreError::Backend(error.to_string()))
    }

    async fn get(&self, object_key: &str) -> Result<object_store::GetResult, BlobStoreError> {
        self.inner
            .get(&Self::path(object_key)?)
            .await
            .map_err(map_store_error)
    }
}

impl BlobStore for ArtifactObjectStore {
    async fn put(&self, object_key: &str, bytes: Vec<u8>) -> Result<(), BlobStoreError> {
        let path = Self::path(object_key)?;
        let payload = PutPayload::from(bytes);
        if self.signer.is_some() {
            let attributes = Attributes::from_iter([
                (Attribute::CacheControl, "no-store"),
                (Attribute::ContentDisposition, "attachment"),
                (Attribute::ContentType, "application/octet-stream"),
            ]);
            self.inner
                .put_opts(&path, payload, attributes.into())
                .await
                .map_err(map_store_error)?;
        } else {
            self.inner
                .put(&path, payload)
                .await
                .map_err(map_store_error)?;
        }
        Ok(())
    }

    async fn get_bounded(
        &self,
        object_key: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>, BlobStoreError> {
        let result = self.get(object_key).await?;
        if result.meta.size > max_bytes {
            return Err(BlobStoreError::TooLarge {
                actual: result.meta.size,
                limit: max_bytes,
            });
        }
        result
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(map_store_error)
    }

    async fn download(&self, object_key: &str) -> Result<BlobDownload, BlobStoreError> {
        let path = Self::path(object_key)?;
        if let Some(signer) = &self.signer {
            let url = signer
                .signed_url(axum::http::Method::GET, &path, SIGNED_DOWNLOAD_TTL)
                .await
                .map_err(map_store_error)?;
            return Ok(BlobDownload::SignedRedirect(url.to_string()));
        }
        let stream = self
            .inner
            .get(&path)
            .await
            .map_err(map_store_error)?
            .into_stream()
            .map(|result| result.map_err(map_store_error));
        Ok(BlobDownload::Stream(Box::pin(stream)))
    }

    async fn delete(&self, object_key: &str) -> Result<(), BlobStoreError> {
        match self.inner.delete(&Self::path(object_key)?).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(map_store_error(error)),
        }
    }
}

fn map_store_error(error: object_store::Error) -> BlobStoreError {
    match error {
        object_store::Error::NotFound { .. } => BlobStoreError::NotFound,
        other => BlobStoreError::Backend(other.to_string()),
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;

    #[derive(Clone, Default)]
    pub struct InMemoryBlobStore {
        inner: ArtifactObjectStore,
    }

    impl Default for ArtifactObjectStore {
        fn default() -> Self {
            Self::new(Arc::new(object_store::memory::InMemory::new()))
        }
    }

    impl BlobStore for InMemoryBlobStore {
        async fn put(&self, object_key: &str, bytes: Vec<u8>) -> Result<(), BlobStoreError> {
            self.inner.put(object_key, bytes).await
        }

        async fn get_bounded(
            &self,
            object_key: &str,
            max_bytes: u64,
        ) -> Result<Vec<u8>, BlobStoreError> {
            self.inner.get_bounded(object_key, max_bytes).await
        }

        async fn download(&self, object_key: &str) -> Result<BlobDownload, BlobStoreError> {
            self.inner.download(object_key).await
        }

        async fn delete(&self, object_key: &str) -> Result<(), BlobStoreError> {
            self.inner.delete(object_key).await
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;

    use super::testing::InMemoryBlobStore;
    use super::*;

    #[tokio::test]
    async fn bounded_reads_and_streaming_are_separate_paths() {
        let store = InMemoryBlobStore::default();
        store
            .put("tenants/acme/blobs/opaque", vec![7; 32])
            .await
            .unwrap();
        assert_eq!(
            store
                .get_bounded("tenants/acme/blobs/opaque", 32)
                .await
                .unwrap()
                .len(),
            32
        );
        assert!(matches!(
            store.get_bounded("tenants/acme/blobs/opaque", 31).await,
            Err(BlobStoreError::TooLarge { .. })
        ));
        let BlobDownload::Stream(stream) =
            store.download("tenants/acme/blobs/opaque").await.unwrap()
        else {
            panic!("memory store must use streaming fallback")
        };
        let bytes = stream
            .try_fold(Vec::new(), |mut all, chunk| async move {
                all.extend_from_slice(&chunk);
                Ok(all)
            })
            .await
            .unwrap();
        assert_eq!(bytes, vec![7; 32]);
    }
}
