use super::{CommandBinding, CommandPayload, payload::MAX_PLAINTEXT};
use crate::{ComputerError, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use uuid::Uuid;
use zeroize::Zeroizing;

type Authentication = Hmac<Sha256>;
const NONCE_BYTES: usize = 24;
const TAG_BYTES: usize = 16;
#[derive(Clone, Copy)]
pub(super) enum SecretKind {
    Command,
    OutputAccess,
    Maintenance,
    FileTransfer,
    FileAccess,
}
impl SecretKind {
    fn maximum_bytes(self) -> usize {
        match self {
            Self::Command => MAX_PLAINTEXT,
            Self::OutputAccess => super::output_access::MAX_OUTPUT_ACCESS_BYTES,
            Self::Maintenance => super::maintenance::MAX_CHECKPOINT_BYTES,
            Self::FileTransfer => super::files::MAX_FILE_PAYLOAD_BYTES,
            Self::FileAccess => super::file_access::MAX_FILE_ACCESS_BYTES,
        }
    }
    fn domain(self) -> &'static [u8] {
        match self {
            Self::Command => b"command",
            Self::OutputAccess => b"output-access",
            Self::Maintenance => b"maintenance-policy",
            Self::FileTransfer => b"file-transfer",
            Self::FileAccess => b"file-artifact-access",
        }
    }
}

/// Secret material comes from installation-owned storage, never the Computer.
pub struct ComputerSealingKey {
    id: Uuid,
    secret: Zeroizing<[u8; 32]>,
}
impl ComputerSealingKey {
    pub fn new(id: Uuid, secret: Zeroizing<[u8; 32]>) -> Result<Self> {
        if id.is_nil() {
            return Err(ComputerError::InvalidInput);
        }
        Ok(Self { id, secret })
    }
}
struct DerivedKey {
    cipher: XChaCha20Poly1305,
    authentication: Zeroizing<[u8; 32]>,
}
impl DerivedKey {
    fn from_secret(secret: &[u8; 32]) -> Result<Self> {
        let derive = |purpose: &[u8]| -> Result<Zeroizing<[u8; 32]>> {
            let mut mac = <Authentication as hmac::KeyInit>::new_from_slice(secret)
                .map_err(|_| ComputerError::Unavailable)?;
            mac.update(purpose);
            Ok(Zeroizing::new(mac.finalize().into_bytes().into()))
        };
        let encryption = derive(b"veoveo.computer.command-encryption-key.v1")?;
        Ok(Self {
            cipher: XChaCha20Poly1305::new_from_slice(encryption.as_ref())
                .map_err(|_| ComputerError::Unavailable)?,
            authentication: derive(b"veoveo.computer.command-fingerprint-key.v1")?,
        })
    }
    fn fingerprint(&self, aad: &[u8], bytes: &[u8]) -> Result<Authentication> {
        let mut mac =
            <Authentication as hmac::KeyInit>::new_from_slice(self.authentication.as_ref())
                .map_err(|_| ComputerError::Unavailable)?;
        mac.update(&(aad.len() as u32).to_be_bytes());
        mac.update(aad);
        mac.update(bytes);
        Ok(mac)
    }
}

/// Private persistence envelope, with no plaintext or unkeyed command digest.
/// Serialization is restricted to the private ledger and trusted backups.
///
/// ```compile_fail
/// use veoveo_computers::secrets::SealedCommand;
/// fn cannot_log(command: SealedCommand) { let _ = format!("{command:?}"); }
/// ```
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SealedCommand {
    version: u8,
    key_id: Uuid,
    nonce: String,
    ciphertext: String,
    fingerprint: String,
}

/// Reads retained keys but writes only with the active key. Removing a required key
/// fails closed; operators must drain or re-encrypt its pending work first.
pub struct ComputerKeyRing {
    active: Uuid,
    keys: BTreeMap<Uuid, DerivedKey>,
}
impl ComputerKeyRing {
    pub fn new(active: Uuid, keys: Vec<ComputerSealingKey>) -> Result<Self> {
        if keys.is_empty() || keys.len() > 4 {
            return Err(ComputerError::InvalidInput);
        }
        let mut selected = BTreeMap::new();
        for key in keys {
            if selected
                .insert(key.id, DerivedKey::from_secret(&key.secret)?)
                .is_some()
            {
                return Err(ComputerError::InvalidInput);
            }
        }
        if !selected.contains_key(&active) {
            return Err(ComputerError::InvalidInput);
        }
        Ok(Self {
            active,
            keys: selected,
        })
    }
    pub fn seal(
        &self,
        binding: &CommandBinding,
        command: &CommandPayload,
    ) -> Result<SealedCommand> {
        let bytes = command.encode()?;
        self.seal_bytes(binding, &bytes, SecretKind::Command)
    }
    pub(super) fn seal_bytes(
        &self,
        binding: &CommandBinding,
        bytes: &[u8],
        kind: SecretKind,
    ) -> Result<SealedCommand> {
        self.seal_bound_bytes(&binding.aad()?, bytes, kind)
    }
    pub(super) fn seal_bound_bytes(
        &self,
        binding: &[u8],
        bytes: &[u8],
        kind: SecretKind,
    ) -> Result<SealedCommand> {
        if bytes.len() < 12 || bytes.len() > kind.maximum_bytes() {
            return Err(ComputerError::InvalidInput);
        }
        let aad = Self::aad(binding, self.active, kind);
        let key = self
            .keys
            .get(&self.active)
            .ok_or(ComputerError::Unavailable)?;
        let mut nonce = [0; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|_| ComputerError::Unavailable)?;
        let ciphertext = key
            .cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: bytes,
                    aad: &aad,
                },
            )
            .map_err(|_| ComputerError::Unavailable)?;
        Ok(SealedCommand {
            version: 1,
            key_id: self.active,
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
            fingerprint: STANDARD.encode(key.fingerprint(&aad, bytes)?.finalize().into_bytes()),
        })
    }
    fn aad(binding: &[u8], key_id: Uuid, kind: SecretKind) -> Vec<u8> {
        let mut aad = key_id.as_bytes().to_vec();
        aad.extend_from_slice(kind.domain());
        aad.extend_from_slice(binding);
        aad
    }
    fn key(&self, sealed: &SealedCommand, kind: SecretKind) -> Result<&DerivedKey> {
        if sealed.version != 1
            || sealed.ciphertext.len() > (kind.maximum_bytes() + TAG_BYTES).div_ceil(3) * 4
            || sealed.ciphertext.len() < (12 + TAG_BYTES).div_ceil(3) * 4
            || sealed.nonce.len() != 32
            || sealed.fingerprint.len() != 44
        {
            return Err(ComputerError::Unavailable);
        }
        self.keys
            .get(&sealed.key_id)
            .ok_or(ComputerError::Unavailable)
    }
    pub fn open(&self, binding: &CommandBinding, sealed: &SealedCommand) -> Result<CommandPayload> {
        CommandPayload::decode(&self.open_bytes(binding, sealed, SecretKind::Command)?)
    }
    pub(super) fn open_bytes(
        &self,
        binding: &CommandBinding,
        sealed: &SealedCommand,
        kind: SecretKind,
    ) -> Result<Zeroizing<Vec<u8>>> {
        self.open_bound_bytes(&binding.aad()?, sealed, kind)
    }
    pub(super) fn open_bound_bytes(
        &self,
        binding: &[u8],
        sealed: &SealedCommand,
        kind: SecretKind,
    ) -> Result<Zeroizing<Vec<u8>>> {
        let key = self.key(sealed, kind)?;
        let aad = Self::aad(binding, sealed.key_id, kind);
        let nonce: [u8; NONCE_BYTES] = STANDARD
            .decode(&sealed.nonce)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(ComputerError::Unavailable)?;
        let ciphertext = STANDARD
            .decode(&sealed.ciphertext)
            .map_err(|_| ComputerError::Unavailable)?;
        let plaintext = Zeroizing::new(
            key.cipher
                .decrypt(
                    &XNonce::from(nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: &aad,
                    },
                )
                .map_err(|_| ComputerError::Unavailable)?,
        );
        let fingerprint = STANDARD
            .decode(&sealed.fingerprint)
            .map_err(|_| ComputerError::Unavailable)?;
        key.fingerprint(&aad, &plaintext)?
            .verify_slice(&fingerprint)
            .map_err(|_| ComputerError::Unavailable)?;
        if plaintext.len() > kind.maximum_bytes() {
            return Err(ComputerError::Unavailable);
        }
        Ok(plaintext)
    }
    /// Uses the original envelope's key even after rotation. Callers must resolve
    /// the original request identity and authority before comparing private input.
    pub fn matches(
        &self,
        binding: &CommandBinding,
        sealed: &SealedCommand,
        command: &CommandPayload,
    ) -> Result<bool> {
        // A damaged ledger cannot resolve an exact retry as if its recoverable
        // request were intact. Authentication failure is not changed input.
        self.open(binding, sealed)?;
        self.matches_payload_fingerprint(
            &binding.aad()?,
            sealed,
            &command.encode()?,
            SecretKind::Command,
        )
    }
    // Callers open and validate the protected payload before comparing a retry.
    pub(super) fn matches_payload_fingerprint(
        &self,
        binding: &[u8],
        sealed: &SealedCommand,
        bytes: &[u8],
        kind: SecretKind,
    ) -> Result<bool> {
        if bytes.len() > kind.maximum_bytes() {
            return Err(ComputerError::InvalidInput);
        }
        let key = self.key(sealed, kind)?;
        let aad = Self::aad(binding, sealed.key_id, kind);
        let fingerprint = STANDARD
            .decode(&sealed.fingerprint)
            .map_err(|_| ComputerError::Unavailable)?;
        Ok(key
            .fingerprint(&aad, bytes)?
            .verify_slice(&fingerprint)
            .is_ok())
    }
}
