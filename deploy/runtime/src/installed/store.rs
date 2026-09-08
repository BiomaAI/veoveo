//! Local receipts are an optimization, never cluster ownership credentials.
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use veoveo_deploy_contract::components::{AtomicTarget, InstalledUnitReceipt};

use crate::{compile::objects::bytes_digest, process::output_checked};

pub(super) struct ReceiptStore {
    directory: PathBuf,
    pub(super) cluster_uid: String,
    // Held across preflight and execution for all worktrees sharing this Git directory.
    _lock: File,
}

impl ReceiptStore {
    pub(super) fn open(repository: &Path, context: &str) -> Result<Self> {
        let git = output_checked(
            "git",
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
            Some(repository),
        )?;
        let root = PathBuf::from(std::str::from_utf8(&git)?.trim()).join("veoveo-deployment");
        Self::at(&root, cluster_uid(context)?)
    }

    pub(super) fn at(root: &Path, cluster_uid: String) -> Result<Self> {
        ensure!(
            !cluster_uid.is_empty() && !cluster_uid.chars().any(char::is_whitespace),
            "cluster has no usable identity"
        );
        let directory = root.join(
            bytes_digest(cluster_uid.as_bytes())?
                .as_str()
                .replace(':', "-"),
        );
        fs::create_dir_all(&directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("execution.lock"))?;
        lock.try_lock()
            .context("another profile operation holds the local cluster receipt lock")?;
        Ok(Self {
            directory,
            cluster_uid,
            _lock: lock,
        })
    }

    fn path(&self, target: &AtomicTarget) -> Result<PathBuf> {
        Ok(self.directory.join(format!(
            "{}.json",
            bytes_digest(&serde_json::to_vec(target)?)?
                .as_str()
                .replace(':', "-")
        )))
    }

    pub(super) fn load(&self, target: &AtomicTarget) -> Result<Option<InstalledUnitReceipt>> {
        let bytes = match fs::read(self.path(target)?) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let receipt: InstalledUnitReceipt =
            serde_json::from_slice(&bytes).context("decoding installed-unit receipt")?;
        receipt.validate()?;
        ensure!(
            receipt.cluster_uid == self.cluster_uid && &receipt.unit.target == target,
            "installed receipt belongs to another cluster or atomic target"
        );
        Ok(Some(receipt))
    }

    pub(super) fn remove(&self, target: &AtomicTarget) -> Result<()> {
        match fs::remove_file(self.path(target)?) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub(super) fn save(&self, receipt: &InstalledUnitReceipt) -> Result<()> {
        receipt.validate()?;
        ensure!(
            receipt.cluster_uid == self.cluster_uid,
            "cannot record another cluster"
        );
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer_pretty(temporary.as_file_mut(), receipt)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.path(&receipt.unit.target)?)?;
        Ok(())
    }
}

pub(super) fn cluster_uid(context: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct Namespace {
        metadata: Identity,
    }
    #[derive(Deserialize)]
    struct Identity {
        uid: String,
    }
    let bytes = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "get",
            "namespace",
            "kube-system",
            "--output=json",
            "--request-timeout=10s",
        ],
        None,
    )?;
    let namespace: Namespace =
        serde_json::from_slice(&bytes).context("reading cluster identity")?;
    Ok(namespace.metadata.uid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_local_receipts_fail_instead_of_becoming_cache_misses() {
        let root = tempfile::tempdir().unwrap();
        let store = ReceiptStore::at(root.path(), "cluster".into()).unwrap();
        let target = AtomicTarget::ManifestSet {
            name: "public-resources".into(),
        };
        fs::write(store.path(&target).unwrap(), b"{broken receipt}").unwrap();
        assert!(store.load(&target).is_err());
    }
}
