//! Per-tenant envelope encryption for artifact bytes.
//!
//! Tenant isolation is a hard partition (see `TECH_DESIGN.md`). Beyond the
//! tenant-scoped object key, bytes are encrypted at rest under a key derived
//! *per tenant* from a master key, so a leak of one tenant's key — or of the
//! raw object store — never exposes another tenant's content, and byte-identical
//! content in two tenants produces unrelated ciphertext (no cross-tenant dedup
//! existence oracle).
//!
//! This is the local/reference cipher. A regulated deployment swaps the master
//! key for a KMS/HSM-backed per-tenant key and FIPS-validated AEAD; the
//! `TenantCipher` seam is where that substitution happens.

use chacha20poly1305::aead::Aead;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sha2::digest::KeyInit as HmacKeyInit;
use veoveo_mcp_contract::TenantId;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

const NONCE_LEN: usize = 12;
const MASTER_KEY_MIN_BYTES: usize = 32;

/// A master key from which per-tenant keys are derived. Never stored alongside
/// ciphertext; supplied at boot from a secret source.
#[derive(Clone)]
pub struct MasterKey(Zeroizing<Vec<u8>>);

/// Errors from key setup or the AEAD.
#[derive(Debug)]
pub enum CryptoError {
    MasterKeyTooShort { actual: usize, minimum: usize },
    Encrypt,
    Decrypt,
    CiphertextTooShort,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MasterKeyTooShort { actual, minimum } => {
                write!(f, "master key is {actual} byte(s); minimum is {minimum}")
            }
            Self::Encrypt => f.write_str("artifact encryption failed"),
            Self::Decrypt => f.write_str("artifact decryption failed"),
            Self::CiphertextTooShort => f.write_str("stored ciphertext is truncated"),
        }
    }
}

impl std::error::Error for CryptoError {}

impl MasterKey {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, CryptoError> {
        let bytes = bytes.into();
        if bytes.len() < MASTER_KEY_MIN_BYTES {
            return Err(CryptoError::MasterKeyTooShort {
                actual: bytes.len(),
                minimum: MASTER_KEY_MIN_BYTES,
            });
        }
        Ok(Self(Zeroizing::new(bytes)))
    }

    /// Derive the 32-byte tenant key: `HMAC-SHA256(master, "artifact:" || tenant)`.
    /// Domain-separated so the same master can key other subsystems safely.
    fn tenant_key(&self, tenant: &TenantId) -> Zeroizing<[u8; 32]> {
        let mut mac = <HmacSha256 as HmacKeyInit>::new_from_slice(&self.0)
            .expect("hmac accepts any key length");
        mac.update(b"artifact-plane:tenant-key:");
        mac.update(tenant.as_str().as_bytes());
        let out = mac.finalize().into_bytes();
        let mut key = [0u8; 32];
        key.copy_from_slice(&out);
        Zeroizing::new(key)
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(<redacted>)")
    }
}

/// Encrypts/decrypts artifact bytes under a tenant-derived key.
#[derive(Clone, Debug)]
pub struct TenantCipher {
    master: MasterKey,
}

impl TenantCipher {
    pub fn new(master: MasterKey) -> Self {
        Self { master }
    }

    fn aead(&self, tenant: &TenantId) -> ChaCha20Poly1305 {
        let key = self.master.tenant_key(tenant);
        ChaCha20Poly1305::new_from_slice(&*key).expect("tenant key is 32 bytes")
    }

    /// Encrypt `plaintext` for `tenant`. Output is `nonce || ciphertext+tag`.
    pub fn encrypt(&self, tenant: &TenantId, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = self.aead(tenant);
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes).map_err(|_| CryptoError::Encrypt)?;
        let nonce = Nonce::try_from(nonce_bytes.as_slice()).map_err(|_| CryptoError::Encrypt)?;
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| CryptoError::Encrypt)?;
        let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    /// Decrypt a `nonce || ciphertext+tag` blob for `tenant`.
    pub fn decrypt(&self, tenant: &TenantId, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if blob.len() < NONCE_LEN {
            return Err(CryptoError::CiphertextTooShort);
        }
        let (nonce_bytes, ciphertext) = blob.split_at(NONCE_LEN);
        let nonce = Nonce::try_from(nonce_bytes).map_err(|_| CryptoError::Decrypt)?;
        let cipher = self.aead(tenant);
        cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|_| CryptoError::Decrypt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> TenantCipher {
        TenantCipher::new(MasterKey::new(b"a-32-byte-master-key-for-testing".to_vec()).unwrap())
    }

    fn tenant(s: &str) -> TenantId {
        TenantId::new(s).unwrap()
    }

    #[test]
    fn rejects_short_master_key() {
        assert!(matches!(
            MasterKey::new(b"short".to_vec()),
            Err(CryptoError::MasterKeyTooShort { .. })
        ));
    }

    #[test]
    fn round_trips_within_a_tenant() {
        let c = cipher();
        let acme = tenant("acme");
        let blob = c.encrypt(&acme, b"hello world").unwrap();
        assert_ne!(blob, b"hello world");
        assert_eq!(c.decrypt(&acme, &blob).unwrap(), b"hello world");
    }

    #[test]
    fn a_tenants_key_cannot_read_anothers_ciphertext() {
        let c = cipher();
        let blob = c.encrypt(&tenant("acme"), b"secret").unwrap();
        assert!(matches!(
            c.decrypt(&tenant("evil"), &blob),
            Err(CryptoError::Decrypt)
        ));
    }

    #[test]
    fn identical_plaintext_yields_unrelated_ciphertext_across_tenants() {
        let c = cipher();
        let a = c.encrypt(&tenant("acme"), b"same").unwrap();
        let b = c.encrypt(&tenant("beta"), b"same").unwrap();
        // Different keys (and random nonces) -> no shared structure to dedup on.
        assert_ne!(a[NONCE_LEN..], b[NONCE_LEN..]);
    }

    #[test]
    fn tampered_ciphertext_is_rejected() {
        let c = cipher();
        let acme = tenant("acme");
        let mut blob = c.encrypt(&acme, b"trustworthy").unwrap();
        let last = blob.len() - 1;
        blob[last] ^= 0xff;
        assert!(matches!(c.decrypt(&acme, &blob), Err(CryptoError::Decrypt)));
    }
}
