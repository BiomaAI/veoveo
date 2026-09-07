//! Catalog visibility and confined read access shared with the Recording service.
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};
use veoveo_platform_store::{RecordId, RecordIdKey, RecordingRecord};
use veoveo_rrd::ingest_parts::ingest_segment_parts_directory;

pub fn authorized_live_layer_path(spool_root: &Path, relative: &str) -> Result<PathBuf> {
    let path = confined_layer_path(spool_root, relative)?;
    if path.exists() {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalizing live layer {}", path.display()))?;
        ensure!(
            canonical.starts_with(spool_root) && canonical.is_file(),
            "live layer escapes the configured spool root"
        );
        return Ok(canonical);
    }
    let parts = ingest_segment_parts_directory(&path);
    if parts.exists() {
        let canonical_parts = parts
            .canonicalize()
            .with_context(|| format!("canonicalizing live layer parts {}", parts.display()))?;
        ensure!(
            canonical_parts.starts_with(spool_root) && canonical_parts.is_dir(),
            "live layer parts escape the configured spool root"
        );
        return Ok(path);
    }
    let parent = path.parent().context("live layer path has no parent")?;
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("canonicalizing live layer parent {}", parent.display()))?;
    ensure!(
        canonical_parent.starts_with(spool_root) && canonical_parent.is_dir(),
        "live layer parent escapes the configured spool root"
    );
    Ok(path)
}

pub fn confined_layer_path(spool_root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "recording layer path must be a normalized relative path"
    );
    Ok(spool_root.join(relative))
}

pub fn labels_visible<'a>(
    recording: &RecordingRecord,
    clearance: impl IntoIterator<Item = &'a str>,
) -> bool {
    let clearance: BTreeSet<&str> = clearance.into_iter().collect();
    recording
        .labels
        .iter()
        .all(|label| clearance.contains(label.as_str()))
}

pub fn record_uuid(record: &RecordId, table: &str) -> Result<uuid::Uuid> {
    ensure!(
        record.table.as_str() == table,
        "record has unexpected table"
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("record key is not UUID: {other:?}"),
    };
    let value = uuid::Uuid::parse_str(&raw)?;
    ensure!(value.get_version_num() == 7, "record key is not UUIDv7");
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn live_layer_path_authorizes_confined_parts_before_rollover() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let relative = "recordings/live.ingest-stream-r0.rrd";
        let final_path = root.join(relative);
        let parts = ingest_segment_parts_directory(&final_path);
        fs::create_dir_all(&parts).unwrap();

        assert_eq!(
            authorized_live_layer_path(&root, relative).unwrap(),
            final_path
        );
    }

    #[test]
    fn live_layer_path_rejects_traversal() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();

        assert!(authorized_live_layer_path(&root, "../outside.rrd").is_err());
        assert!(authorized_live_layer_path(&root, "/outside.rrd").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn live_layer_parts_cannot_follow_a_symlink_outside_the_spool() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("layer.rrd.parts")).unwrap();
        assert!(authorized_live_layer_path(root.path(), "layer.rrd").is_err());
    }
}
