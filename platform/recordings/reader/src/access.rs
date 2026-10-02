//! Confined read access and recording identity shared with the Recording service.
use anyhow::{Context, Result, ensure};
use std::path::{Component, Path, PathBuf};
use veoveo_platform_store::{RecordId, RecordIdKey};
use veoveo_rrd::ingest_parts::ingest_segment_parts_directory;

pub fn authorized_live_layer_path(spool_root: &Path, relative: &str) -> Result<PathBuf> {
    let path = confined_layer_path(spool_root, relative)?;
    let path = if path.exists() {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalizing live layer {}", path.display()))?;
        ensure!(
            canonical.starts_with(spool_root) && canonical.is_file(),
            "live layer escapes the configured spool root"
        );
        canonical
    } else {
        path
    };
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

pub fn record_uuid(record: &RecordId, table: &str) -> Result<uuid::Uuid> {
    ensure!(
        record.table.as_str() == table,
        "record has unexpected table"
    );
    let RecordIdKey::Uuid(value) = &record.key else {
        anyhow::bail!("record key must be a native UUID");
    };
    let value = value.into_inner();
    ensure!(
        value.get_version_num() == 7 && value.get_variant() == uuid::Variant::RFC4122,
        "record key must be an RFC UUIDv7"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn record_admission_requires_the_native_key_and_declared_table() {
        let id = veoveo_platform_store::RecordingId::new();
        assert_eq!(
            record_uuid(&id.record_id(), "recording").unwrap(),
            id.as_uuid()
        );
        assert!(record_uuid(&id.record_id(), "recording_layer").is_err());
        let string_key = RecordId::new("recording", id.to_string());
        assert!(record_uuid(&string_key, "recording").is_err());
        for raw in [
            "01983da0-0000-4000-8000-000000000001",
            "01983da0-0000-7000-c000-000000000001",
        ] {
            let raw = uuid::Uuid::parse_str(raw).unwrap();
            let record = veoveo_platform_store::RecordingId::from_uuid(raw).record_id();
            assert!(record_uuid(&record, "recording").is_err());
        }
    }
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
        // A materialized publication file does not exempt its live parts from
        // confinement while the catalog still says Writing or Staged.
        fs::write(root.path().join("layer.rrd"), b"materialized layer").unwrap();
        assert!(authorized_live_layer_path(root.path(), "layer.rrd").is_err());
    }
}
