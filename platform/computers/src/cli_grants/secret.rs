use super::CliGrantCredential;
use crate::{ComputerError, Result};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) fn issue(id: Uuid) -> Result<(CliGrantCredential, String)> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| ComputerError::Unavailable)?;
    let secret = hex::encode(bytes);
    Ok((
        CliGrantCredential::new(format!("vcli1.{id}.{secret}")),
        hash(&secret),
    ))
}
pub(super) fn parse(token: &CliGrantCredential) -> Result<(Uuid, String)> {
    let text = token.expose_secret();
    if text.len() != 107 {
        return Err(ComputerError::Forbidden);
    }
    let text = text
        .strip_prefix("vcli1.")
        .ok_or(ComputerError::Forbidden)?;
    let (id, secret) = text.split_once('.').ok_or(ComputerError::Forbidden)?;
    let id: Uuid = id.parse().map_err(|_| ComputerError::Forbidden)?;
    if id.is_nil()
        || !text.starts_with(&id.to_string())
        || secret.len() != 64
        || !secret
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ComputerError::Forbidden);
    }
    Ok((id, hash(secret)))
}
fn hash(secret: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"veoveo.computer.cli-grant.v1\0");
    hash.update(secret.as_bytes());
    hex::encode(hash.finalize())
}
