use crate::{ComputerError, Result, api::TerminalToken};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(super) fn issue(id: Uuid) -> Result<(TerminalToken, String)> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| ComputerError::Unavailable)?;
    let secret = hex::encode(bytes);
    let token = TerminalToken::new(format!("{id}.{secret}"));
    Ok((token, hash(&secret)))
}
pub(super) fn parse(token: &TerminalToken) -> Result<(Uuid, String)> {
    let text = token.expose_secret();
    if text.len() != 101 {
        return Err(ComputerError::Forbidden);
    }
    let (id, secret) = text.split_once('.').ok_or(ComputerError::Forbidden)?;
    let id: Uuid = id.parse().map_err(|_| ComputerError::Forbidden)?;
    if id.is_nil()
        || !secret
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || secret.len() != 64
        || !text.starts_with(&id.to_string())
    {
        return Err(ComputerError::Forbidden);
    }
    Ok((id, hash(secret)))
}
fn hash(secret: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"veoveo.computer.session-ticket.v1\0");
    hash.update(secret.as_bytes());
    hex::encode(hash.finalize())
}
