use crate::{PoseError, PoseLimits, PoseSnapshot, decode_snapshot};

pub fn encode_stream_frame(encoded_snapshot: &[u8]) -> Result<Vec<u8>, PoseError> {
    let length = u32::try_from(encoded_snapshot.len()).map_err(|_| PoseError::MessageBytes {
        actual: encoded_snapshot.len(),
        maximum: u32::MAX as usize,
    })?;
    let mut frame = Vec::with_capacity(4 + encoded_snapshot.len());
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(encoded_snapshot);
    Ok(frame)
}

pub struct PoseStreamDecoder {
    buffered: Vec<u8>,
    limits: PoseLimits,
}

impl PoseStreamDecoder {
    pub fn new(limits: PoseLimits) -> Self {
        Self {
            buffered: Vec::new(),
            limits,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<PoseSnapshot>, PoseError> {
        self.buffered.extend_from_slice(bytes);
        let mut snapshots = Vec::new();
        loop {
            if self.buffered.len() < 4 {
                break;
            }
            let length =
                u32::from_be_bytes(self.buffered[..4].try_into().expect("length")) as usize;
            if length > self.limits.max_message_bytes {
                return Err(PoseError::MessageBytes {
                    actual: length,
                    maximum: self.limits.max_message_bytes,
                });
            }
            if self.buffered.len() < 4 + length {
                break;
            }
            let encoded = self.buffered[4..4 + length].to_vec();
            self.buffered.drain(..4 + length);
            snapshots.push(decode_snapshot(&encoded, &self.limits)?);
        }
        if self.buffered.len() > self.limits.max_message_bytes.saturating_add(4) {
            return Err(PoseError::MessageBytes {
                actual: self.buffered.len(),
                maximum: self.limits.max_message_bytes + 4,
            });
        }
        Ok(snapshots)
    }
}
