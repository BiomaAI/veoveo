//! Private regular-file transfer profile. Paths and bytes carry no authority.
use std::{
    io::{Read, Write},
    path::{Component, Path},
};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 4096;

/// No Debug or Display: the requested path is private user data.
pub struct FileRequest(pub(crate) Payload);

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Payload {
    version: u8,
    pub path: String,
    pub operation: FileOperation,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum FileOperation {
    Import { bytes: u64, sha256: [u8; 32] },
    Export { maximum_bytes: u64 },
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.path.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFailure {
    InvalidRequest,
    WrongIdentity,
    ConfinementUnavailable,
    PathUnavailable,
    UnsupportedFile,
    TooLarge,
    DestinationExists,
    InputIncomplete,
    Integrity,
    FileChanged,
    Storage,
    OutputUnavailable,
    CommitUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileReceipt {
    pub bytes: u64,
    pub sha256: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileTransferBounds {
    Import { bytes: u64, sha256: [u8; 32] },
    Export { maximum_bytes: u64 },
}

impl FileRequest {
    pub fn bounds(&self) -> FileTransferBounds {
        match self.0.operation {
            FileOperation::Import { bytes, sha256 } => FileTransferBounds::Import { bytes, sha256 },
            FileOperation::Export { maximum_bytes } => FileTransferBounds::Export { maximum_bytes },
        }
    }
    pub fn import(path: String, bytes: u64, sha256: [u8; 32]) -> Result<Self, FileFailure> {
        Self::new(path, FileOperation::Import { bytes, sha256 })
    }
    pub fn export(path: String, maximum_bytes: u64) -> Result<Self, FileFailure> {
        Self::new(path, FileOperation::Export { maximum_bytes })
    }
    fn new(path: String, operation: FileOperation) -> Result<Self, FileFailure> {
        let request = Self(Payload {
            version: 1,
            path,
            operation,
        });
        request.validate()?;
        Ok(request)
    }
    fn validate(&self) -> Result<(), FileFailure> {
        let path = &self.0.path;
        // Component iteration normalizes repeated separators and trailing dots;
        // reject those spellings as well as traversal before any filesystem call.
        if self.0.version != 1
            || path.is_empty()
            || path.len() > 1024
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || !Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err(FileFailure::InvalidRequest);
        }
        let limit = match self.0.operation {
            FileOperation::Import { bytes, .. } => bytes,
            FileOperation::Export { maximum_bytes } => maximum_bytes,
        };
        if limit > MAX_FILE_BYTES {
            return Err(FileFailure::TooLarge);
        }
        Ok(())
    }
    /// Header only. An import's exact binary body follows on the private stream.
    pub fn encode(&self) -> Result<Zeroizing<Vec<u8>>, FileFailure> {
        self.validate()?;
        let body =
            Zeroizing::new(serde_json::to_vec(&self.0).map_err(|_| FileFailure::InvalidRequest)?);
        if body.len() > MAX_HEADER_BYTES {
            return Err(FileFailure::InvalidRequest);
        }
        let mut bytes = Zeroizing::new((body.len() as u32).to_be_bytes().to_vec());
        bytes.extend_from_slice(&body);
        Ok(bytes)
    }
}

pub fn read_file_request(input: &mut impl Read) -> Result<FileRequest, FileFailure> {
    let mut length = [0; 4];
    input
        .read_exact(&mut length)
        .map_err(|_| FileFailure::InvalidRequest)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_HEADER_BYTES {
        return Err(FileFailure::InvalidRequest);
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    input
        .read_exact(&mut bytes)
        .map_err(|_| FileFailure::InvalidRequest)?;
    let request =
        FileRequest(serde_json::from_slice(&bytes).map_err(|_| FileFailure::InvalidRequest)?);
    request.validate()?;
    Ok(request)
}

/// The only public diagnostic is a bounded structured outcome, never file paths.
pub fn write_file_result(
    output: &mut impl Write,
    result: &Result<FileReceipt, FileFailure>,
) -> std::io::Result<()> {
    serde_json::to_writer(&mut *output, result)?;
    output.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfer_header_is_bounded_and_leaves_binary_body_unread() {
        let request = FileRequest::import("project/data.bin".into(), 4, [3; 32]).unwrap();
        let mut frame = request.encode().unwrap();
        frame.extend_from_slice(&[0, 255, 10, 13]);
        let mut input = frame.as_slice();
        let decoded = read_file_request(&mut input).unwrap();
        assert_eq!(decoded.0.path, "project/data.bin");
        assert_eq!(input, [0, 255, 10, 13]);
        for path in [
            "",
            "/etc/passwd",
            "../outside",
            "a/../b",
            "a//b",
            "a/./b",
            "a/",
            "a\0b",
        ] {
            assert!(FileRequest::export(path.into(), 1).is_err(), "{path:?}");
        }
        assert!(FileRequest::export("a".into(), MAX_FILE_BYTES + 1).is_err());
        let duplicate = br#"{"version":1,"path":"a","path":"b","operation":{"kind":"export","maximum_bytes":1}}"#;
        let mut frame = (duplicate.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(duplicate);
        assert!(read_file_request(&mut frame.as_slice()).is_err());
    }
}
