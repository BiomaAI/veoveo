//! Create-only attempts and a recoverable, atomically replaced worktree index.
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::model::{
    INDEX_PATH, INDEX_SCHEMA, Outcome, RECEIPT_DIRECTORY, RECEIPT_SCHEMA, Receipt, ReceiptIndex,
    ReceiptRef,
};

const MAX_RECEIPT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RECEIPTS: usize = 100_000;

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn receipt_path(root: &Path, id: Uuid) -> PathBuf {
    root.join(RECEIPT_DIRECTORY).join(format!("{id}.json"))
}

fn read_bounded(path: &Path) -> Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("reading {}", path.display()))?;
    ensure!(
        metadata.is_file() && !metadata.is_symlink(),
        "evidence must be a regular file: {}",
        path.display()
    );
    ensure!(
        metadata.len() <= MAX_RECEIPT_BYTES,
        "evidence exceeds the size bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_RECEIPT_BYTES,
        "evidence grew beyond the size bound"
    );
    Ok(bytes)
}

fn evidence_directory(root: &Path) -> Result<PathBuf> {
    let mut directory = root.to_path_buf();
    for part in ["testing", "test-receipts"] {
        directory.push(part);
        match fs::symlink_metadata(&directory) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.is_symlink(),
                "evidence directory must not redirect: {}",
                directory.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&directory) {
                    Ok(()) => (),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
                    Err(error) => return Err(error.into()),
                }
                let metadata = fs::symlink_metadata(&directory)?;
                ensure!(
                    metadata.is_dir() && !metadata.is_symlink(),
                    "evidence directory was redirected"
                );
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(directory)
}

pub(super) fn validate_receipt(receipt: &Receipt) -> Result<()> {
    ensure!(
        receipt.schema_version == RECEIPT_SCHEMA,
        "unsupported test receipt schema"
    );
    ensure!(!receipt.run_id.is_nil(), "test receipt has no run identity");
    ensure!(
        receipt.finished_at >= receipt.started_at,
        "test receipt clock moved backwards"
    );
    ensure!(
        !receipt.check_id.is_empty() && receipt.check_id.len() <= 128,
        "invalid check identity"
    );
    ensure!(
        !receipt.name.is_empty()
            && receipt.name.len() <= 64
            && receipt
                .name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "invalid check display name"
    );
    ensure!(
        receipt.inputs.files.len() <= 100_000,
        "test input manifest exceeds its bound"
    );
    ensure!(
        receipt.environment.runtime.bindings.len() <= 128,
        "too many runtime bindings"
    );
    ensure!(
        receipt.inputs.planner_version > 0
            && receipt.inputs.planner_version <= super::model::PLANNER_VERSION,
        "unsupported receipt planner"
    );
    ensure!(
        receipt.inputs.digest
            == digest(&serde_json::to_vec(&(
                receipt.inputs.planner_version,
                &receipt.inputs.scope,
                &receipt.inputs.files
            ))?),
        "receipt manifest and digest differ"
    );
    let mut previous = None;
    for input in &receipt.inputs.files {
        super::catalog::relative(&input.path)?;
        ensure!(
            previous.is_none_or(|path: &str| path < input.path.as_str()),
            "receipt inputs are not unique and sorted"
        );
        previous = Some(input.path.as_str());
    }
    if let super::model::CommandIdentity::Admitted { arguments } = &receipt.command {
        ensure!(
            receipt.check_id == super::catalog::check_id(arguments)?,
            "command and check identity differ"
        );
    } else {
        ensure!(
            !receipt.environment.reusable,
            "opaque command cannot be reusable"
        );
    }
    ensure!(
        receipt.outcome != Outcome::Passed || receipt.exit_code == Some(0),
        "passing attempt has no successful exit status"
    );
    Ok(())
}

pub(super) fn ensure_index_complete(root: &Path, index: &ReceiptIndex) -> Result<()> {
    let ids: std::collections::BTreeSet<_> = index
        .receipts
        .iter()
        .map(|reference| reference.run_id)
        .collect();
    for entry in fs::read_dir(root.join(RECEIPT_DIRECTORY))? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let id: Uuid = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .context("invalid receipt filename")?
                .parse()?;
            ensure!(
                ids.contains(&id),
                "complete receipt is not indexed; run a check to recover publication before using this evidence"
            );
        }
    }
    Ok(())
}

pub(super) fn read_receipt(root: &Path, reference: &ReceiptRef) -> Result<Receipt> {
    ensure!(!reference.run_id.is_nil(), "invalid receipt reference");
    let bytes = read_bounded(&receipt_path(root, reference.run_id))?;
    ensure!(
        digest(&bytes) == reference.sha256,
        "receipt content differs from its index reference: {}",
        reference.run_id
    );
    let receipt: Receipt = serde_json::from_slice(&bytes).context("decoding test receipt")?;
    validate_receipt(&receipt)?;
    ensure!(
        receipt.run_id == reference.run_id,
        "receipt filename and run identity differ"
    );
    Ok(receipt)
}

pub(super) fn read_index(root: &Path) -> Result<ReceiptIndex> {
    let bytes = read_bounded(&root.join(INDEX_PATH))?;
    let index: ReceiptIndex = serde_json::from_slice(&bytes)
        .context("reading v3 evidence index; run a check with the current recorder to replace a historical v2 report")?;
    ensure!(
        index.schema_version == INDEX_SCHEMA,
        "unsupported evidence index schema"
    );
    ensure!(
        index.receipts.len() <= MAX_RECEIPTS,
        "evidence index exceeds its receipt bound"
    );
    let mut ids = std::collections::BTreeSet::new();
    for reference in &index.receipts {
        ensure!(ids.insert(reference.run_id), "duplicate receipt reference");
    }
    ensure!(
        index.latest.values().all(|id| ids.contains(id)),
        "index selects a missing receipt"
    );
    let mut latest = BTreeMap::<String, (chrono::DateTime<Utc>, Uuid)>::new();
    for reference in &index.receipts {
        let receipt = read_receipt(root, reference)?;
        if receipt.outcome != Outcome::InputsChanged {
            let candidate = (receipt.finished_at, receipt.run_id);
            let selected = latest.entry(receipt.check_id).or_insert(candidate);
            *selected = (*selected).max(candidate);
        }
    }
    ensure!(
        index.latest == latest.into_iter().map(|(key, (_, id))| (key, id)).collect(),
        "index does not select the latest completed attempts"
    );
    Ok(index)
}

/// Rebuilding from complete immutable files also recovers a crash between receipt
/// publication and index replacement. Retired v2 entries are never imported.
fn rebuild(root: &Path) -> Result<ReceiptIndex> {
    let mut receipts = Vec::new();
    let mut latest = BTreeMap::<String, (chrono::DateTime<Utc>, Uuid)>::new();
    for entry in fs::read_dir(evidence_directory(root)?)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        ensure!(
            receipts.len() < MAX_RECEIPTS,
            "evidence receipt retention bound reached"
        );
        let id: Uuid = path
            .file_stem()
            .and_then(|name| name.to_str())
            .context("receipt filename is not UTF-8")?
            .parse()
            .context("invalid receipt filename")?;
        let bytes = read_bounded(&path)?;
        let receipt: Receipt =
            serde_json::from_slice(&bytes).context("decoding complete test receipt")?;
        validate_receipt(&receipt)?;
        ensure!(
            receipt.run_id == id,
            "receipt filename and run identity differ"
        );
        receipts.push(ReceiptRef {
            run_id: id,
            sha256: digest(&bytes),
        });
        if receipt.outcome != Outcome::InputsChanged {
            let candidate = (receipt.finished_at, id);
            let selected = latest.entry(receipt.check_id).or_insert(candidate);
            if candidate > *selected {
                *selected = candidate;
            }
        }
    }
    receipts.sort_by_key(|reference| reference.run_id);
    Ok(ReceiptIndex {
        schema_version: INDEX_SCHEMA.to_owned(),
        updated_at: Utc::now(),
        receipts,
        latest: latest.into_iter().map(|(key, (_, id))| (key, id)).collect(),
    })
}

pub(super) fn publish(root: &Path, receipt: &Receipt) -> Result<ReceiptRef> {
    validate_receipt(receipt)?;
    let directory = evidence_directory(root)?;
    let mut bytes = serde_json::to_vec_pretty(receipt)?;
    bytes.push(b'\n');
    ensure!(
        bytes.len() as u64 <= MAX_RECEIPT_BYTES,
        "test receipt exceeds its publication bound"
    );
    let reference = ReceiptRef {
        run_id: receipt.run_id,
        sha256: digest(&bytes),
    };
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(receipt_path(root, receipt.run_id))
        .map_err(|error| error.error)
        .context("publishing immutable test receipt")?;
    File::open(&directory)?.sync_all()?;

    let git_path = crate::process::output_text(
        "git",
        ["rev-parse", "--git-path", "veoveo-test-report.lock"],
        Some(root),
    )?;
    let lock = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(git_path.trim()))?;
    lock.lock().context("locking evidence index publication")?;
    let old_path = root.join(INDEX_PATH);
    if old_path.exists() {
        let bytes = read_bounded(&old_path)?;
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct HistoricalVersion {
            schema_version: u32,
        }
        if !serde_json::from_slice::<HistoricalVersion>(&bytes)
            .is_ok_and(|header| header.schema_version == 2)
        {
            // Do not silently bless modified existing receipts when rebuilding.
            read_index(root)?;
        }
    }
    let index = rebuild(root)?;
    let parent = root.join("testing");
    let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
    serde_json::to_writer_pretty(&mut temporary, &index)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(root.join(INDEX_PATH))
        .map_err(|error| error.error)
        .context("publishing evidence index")?;
    File::open(&parent)?.sync_all()?;
    Ok(reference)
}
