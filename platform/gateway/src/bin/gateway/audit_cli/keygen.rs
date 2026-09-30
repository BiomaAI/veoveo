//! Installation key bootstrap never sends secret material to stdout or logs.
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::Path};
use veoveo_audit::integrity::AuditSigningKey;

pub(super) fn generate(output: &Path) -> anyhow::Result<()> {
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    getrandom::fill(seed.as_mut_slice()).context("cannot generate audit signing entropy")?;
    let key = AuditSigningKey::from_seed(&seed);
    let encoded = zeroize::Zeroizing::new(STANDARD.encode(seed.as_slice()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output)
        .context("audit seed output must be a new file in an existing private directory")?;
    file.write_all(encoded.as_bytes())
        .context("cannot write audit signing seed")?;
    file.sync_all().context("cannot sync audit signing seed")?;
    #[derive(serde::Serialize)]
    struct PublicKey {
        key_id: veoveo_types::Sha256Digest,
        public_key: String,
    }
    serde_json::to_writer(
        std::io::stdout().lock(),
        &PublicKey {
            key_id: key.key_id(),
            public_key: STANDARD.encode(key.public_key()),
        },
    )?;
    println!();
    Ok(())
}
