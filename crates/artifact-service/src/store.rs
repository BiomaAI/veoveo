//! Artifact byte storage: tenant-scoped keys over an S3-compatible object store,
//! with per-tenant encryption applied transparently.

use std::sync::Arc;

use object_store::{ObjectStore, ObjectStoreExt, PutPayload, path::Path};
use veoveo_mcp_contract::{ArtifactSha256, TenantId, tenant_scoped_object_key};

use crate::crypto::{CryptoError, TenantCipher};

/// Byte-level storage keyed by `(tenant, sha)`. The trait keeps the service
/// testable without a real object store or S3.
pub trait BlobStore: Send + Sync {
    fn put(
        &self,
        tenant: &TenantId,
        sha: &ArtifactSha256,
        plaintext: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(), BlobStoreError>> + Send;

    fn get(
        &self,
        tenant: &TenantId,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, BlobStoreError>> + Send;

    fn delete(
        &self,
        tenant: &TenantId,
        sha: &ArtifactSha256,
    ) -> impl std::future::Future<Output = Result<(), BlobStoreError>> + Send;
}

#[derive(Debug)]
pub enum BlobStoreError {
    NotFound,
    Crypto(CryptoError),
    Backend(String),
}

impl std::fmt::Display for BlobStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("artifact bytes not found"),
            Self::Crypto(e) => write!(f, "crypto error: {e}"),
            Self::Backend(m) => write!(f, "object store error: {m}"),
        }
    }
}

impl std::error::Error for BlobStoreError {}

/// The production store: encrypt per tenant, then write to the object store at
/// the tenant-scoped key.
#[derive(Clone)]
pub struct EncryptedObjectStore {
    inner: Arc<dyn ObjectStore>,
    cipher: TenantCipher,
}

impl EncryptedObjectStore {
    pub fn new(inner: Arc<dyn ObjectStore>, cipher: TenantCipher) -> Self {
        Self { inner, cipher }
    }

    fn path(tenant: &TenantId, sha: &ArtifactSha256) -> Path {
        Path::from(tenant_scoped_object_key(tenant, sha))
    }
}

impl BlobStore for EncryptedObjectStore {
    async fn put(
        &self,
        tenant: &TenantId,
        sha: &ArtifactSha256,
        plaintext: Vec<u8>,
    ) -> Result<(), BlobStoreError> {
        let blob = self
            .cipher
            .encrypt(tenant, &plaintext)
            .map_err(BlobStoreError::Crypto)?;
        self.inner
            .put(&Self::path(tenant, sha), PutPayload::from(blob))
            .await
            .map_err(|e| BlobStoreError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(
        &self,
        tenant: &TenantId,
        sha: &ArtifactSha256,
    ) -> Result<Vec<u8>, BlobStoreError> {
        let path = Self::path(tenant, sha);
        let get = self.inner.get(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => BlobStoreError::NotFound,
            other => BlobStoreError::Backend(other.to_string()),
        })?;
        let bytes = get
            .bytes()
            .await
            .map_err(|e| BlobStoreError::Backend(e.to_string()))?;
        self.cipher
            .decrypt(tenant, &bytes)
            .map_err(BlobStoreError::Crypto)
    }

    async fn delete(
        &self,
        tenant: &TenantId,
        sha: &ArtifactSha256,
    ) -> Result<(), BlobStoreError> {
        match self.inner.delete(&Self::path(tenant, sha)).await {
            Ok(()) => Ok(()),
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(other) => Err(BlobStoreError::Backend(other.to_string())),
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// In-memory blob store for unit tests (no encryption seam needed since the
    /// crypto layer is tested separately).
    #[derive(Default)]
    pub struct InMemoryBlobStore {
        blobs: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl InMemoryBlobStore {
        fn key(tenant: &TenantId, sha: &ArtifactSha256) -> String {
            tenant_scoped_object_key(tenant, sha)
        }
    }

    impl BlobStore for InMemoryBlobStore {
        async fn put(
            &self,
            tenant: &TenantId,
            sha: &ArtifactSha256,
            plaintext: Vec<u8>,
        ) -> Result<(), BlobStoreError> {
            self.blobs
                .lock()
                .unwrap()
                .insert(Self::key(tenant, sha), plaintext);
            Ok(())
        }

        async fn get(
            &self,
            tenant: &TenantId,
            sha: &ArtifactSha256,
        ) -> Result<Vec<u8>, BlobStoreError> {
            self.blobs
                .lock()
                .unwrap()
                .get(&Self::key(tenant, sha))
                .cloned()
                .ok_or(BlobStoreError::NotFound)
        }

        async fn delete(
            &self,
            tenant: &TenantId,
            sha: &ArtifactSha256,
        ) -> Result<(), BlobStoreError> {
            self.blobs.lock().unwrap().remove(&Self::key(tenant, sha));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::InMemoryBlobStore;
    use super::*;
    use crate::crypto::MasterKey;
    use object_store::memory::InMemory;

    fn sha() -> ArtifactSha256 {
        ArtifactSha256::new("b".repeat(64)).unwrap()
    }

    #[tokio::test]
    async fn encrypted_store_round_trips_and_isolates_tenants() {
        let cipher =
            TenantCipher::new(MasterKey::new(b"a-32-byte-master-key-for-testing".to_vec()).unwrap());
        let store = EncryptedObjectStore::new(Arc::new(InMemory::new()), cipher);
        let acme = TenantId::new("acme").unwrap();
        store.put(&acme, &sha(), b"payload".to_vec()).await.unwrap();
        assert_eq!(store.get(&acme, &sha()).await.unwrap(), b"payload");

        // Another tenant has nothing at its (distinct) key.
        let other = TenantId::new("beta").unwrap();
        assert!(matches!(
            store.get(&other, &sha()).await,
            Err(BlobStoreError::NotFound)
        ));
    }

    #[tokio::test]
    async fn in_memory_store_round_trips() {
        let store = InMemoryBlobStore::default();
        let acme = TenantId::new("acme").unwrap();
        store.put(&acme, &sha(), b"x".to_vec()).await.unwrap();
        assert_eq!(store.get(&acme, &sha()).await.unwrap(), b"x");
        store.delete(&acme, &sha()).await.unwrap();
        assert!(matches!(
            store.get(&acme, &sha()).await,
            Err(BlobStoreError::NotFound)
        ));
    }
}
