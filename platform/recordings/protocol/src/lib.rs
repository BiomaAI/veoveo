//! Canonical external and installation-local Recording Hub ingest protocol.
//!
//! Control messages and errors use one versioned protobuf contract. Rerun
//! messages remain in their upstream RRD encoding inside a typed envelope so
//! the boundary records the exact decoder release without re-modeling Arrow.

use sha2::{Digest, Sha256};

pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/veoveo.recording.ingest.v1.rs"));
}

pub const PROTOCOL_VERSION: &str = "2026-07-16";
pub const REQUIRED_SCOPE: &str = "recording:ingest";
pub const DEFAULT_MAXIMUM_BATCH_BYTES: u64 = 8 * 1024 * 1024;
pub const MEDIA_TYPE: &str = "application/vnd.veoveo.recording-ingest.v1+protobuf";
pub const DISCOVERY_PATH: &str = "/.well-known/veoveo-recording-ingest";
pub const STREAMS_PATH: &str = "/ingest/recordings/v1/streams";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BatchValidationError {
    #[error("batch sequence must start at one")]
    ZeroSequence,
    #[error("batch payload format is unsupported")]
    UnsupportedPayloadFormat,
    #[error("batch payload is empty")]
    EmptyPayload,
    #[error("batch payload exceeds the {maximum_bytes}-byte limit")]
    PayloadTooLarge { maximum_bytes: u64 },
    #[error("batch message_count must be positive")]
    EmptyMessageCount,
    #[error("batch sha256 must contain exactly 32 bytes")]
    InvalidDigestLength,
    #[error("batch sha256 does not match encoded_rrd")]
    DigestMismatch,
}

impl v1::RecordingBatch {
    pub fn validate(&self, maximum_bytes: u64) -> Result<(), BatchValidationError> {
        if self.sequence == 0 {
            return Err(BatchValidationError::ZeroSequence);
        }
        if v1::RerunPayloadFormat::try_from(self.payload_format)
            != Ok(v1::RerunPayloadFormat::Rrd0341)
        {
            return Err(BatchValidationError::UnsupportedPayloadFormat);
        }
        if self.encoded_rrd.is_empty() {
            return Err(BatchValidationError::EmptyPayload);
        }
        if self.encoded_rrd.len() as u64 > maximum_bytes {
            return Err(BatchValidationError::PayloadTooLarge { maximum_bytes });
        }
        if self.message_count == 0 {
            return Err(BatchValidationError::EmptyMessageCount);
        }
        if self.sha256.len() != 32 {
            return Err(BatchValidationError::InvalidDigestLength);
        }
        if Sha256::digest(&self.encoded_rrd).as_slice() != self.sha256.as_slice() {
            return Err(BatchValidationError::DigestMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(payload: &[u8]) -> v1::RecordingBatch {
        v1::RecordingBatch {
            sequence: 1,
            payload_format: v1::RerunPayloadFormat::Rrd0341.into(),
            encoded_rrd: payload.to_vec(),
            sha256: Sha256::digest(payload).to_vec(),
            message_count: 1,
        }
    }

    #[test]
    fn validates_the_canonical_batch_shape() {
        batch(b"rrd").validate(DEFAULT_MAXIMUM_BATCH_BYTES).unwrap();
    }

    #[test]
    fn rejects_digest_mismatch_and_limits() {
        let mut invalid = batch(b"rrd");
        invalid.sha256[0] ^= 1;
        assert_eq!(
            invalid.validate(DEFAULT_MAXIMUM_BATCH_BYTES),
            Err(BatchValidationError::DigestMismatch)
        );
        assert_eq!(
            batch(b"large").validate(4),
            Err(BatchValidationError::PayloadTooLarge { maximum_bytes: 4 })
        );
    }
}
