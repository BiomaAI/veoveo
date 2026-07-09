//! Replay: rebuild domain truth from the decision log.
//!
//! The RRD plane is the log of record for domain state — every
//! `memory_write` is mirrored to `/domain/{table}` as its serialized typed
//! mutation. Replay applies those mutations, in log order, to a fresh
//! database with the manifest's migrations, producing
//! `memory.replayed.duckdb` next to the live file. Drift between the two
//! planes is therefore recoverable, never fatal.

use std::path::Path;

use anyhow::{Context, Result};

use crate::{
    ledger::{KernelLedger, MemoryWrite},
    manifest::AgentManifest,
    timeline::{TimelineQuery, query_segments},
};

pub struct ReplayReport {
    pub applied: usize,
    pub skipped: usize,
    pub output_path: std::path::PathBuf,
}

pub fn replay_domain(manifest: &AgentManifest, data_dir: &Path) -> Result<ReplayReport> {
    let rrd_dir = data_dir.join(&manifest.memory.rrd_dir);
    let output_path = data_dir.join("memory.replayed.duckdb");
    if output_path.exists() {
        std::fs::remove_file(&output_path)?;
    }
    let ledger = KernelLedger::open(&output_path)?;
    if let Some(dir) = &manifest.migrations_dir {
        ledger.run_migrations(dir)?;
    }

    let rows = query_segments(
        &rrd_dir,
        &TimelineQuery {
            entities: "/domain/**".to_string(),
            timeline: "log_time".to_string(),
            max_rows: u64::MAX,
        },
    )
    .context("reading the decision log")?;

    let mut applied = 0usize;
    let mut skipped = 0usize;
    for row in rows {
        let Some(object) = row.as_object() else {
            continue;
        };
        // A /domain/** TextLog row has one `...:TextLog:text` column holding
        // the serialized mutation.
        let Some(text) = object.iter().find_map(|(name, value)| {
            (name.contains(":TextLog:") && name.ends_with(":text"))
                .then(|| value.as_str())
                .flatten()
        }) else {
            continue;
        };
        // A component cell renders as a list; a single TextLog row is a
        // one-element list of the serialized mutation.
        let writes: Vec<MemoryWrite> = match serde_json::from_str::<Vec<MemoryWrite>>(text)
            .or_else(|_| serde_json::from_str::<MemoryWrite>(text).map(|write| vec![write]))
        {
            Ok(writes) => writes,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        for write in writes {
            let table = write.table().to_string();
            match ledger.write(&write, std::slice::from_ref(&table)) {
                Ok(_) => applied += 1,
                Err(err) => {
                    tracing::warn!(%err, table, "replayed write failed");
                    skipped += 1;
                }
            }
        }
    }
    Ok(ReplayReport {
        applied,
        skipped,
        output_path,
    })
}
