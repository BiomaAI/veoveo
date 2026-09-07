//! Ordered, complete ingest parts; atomic staging files are never read as committed data.

use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

pub const VIDEO_STREAM_MARKER: &str = ".video-stream";

pub fn ingest_segment_parts_directory(segment_path: &Path) -> PathBuf {
    let mut value = OsString::from(segment_path.as_os_str());
    value.push(".parts");
    PathBuf::from(value)
}

pub fn ingest_part_sequence(path: &Path) -> Option<u64> {
    let name = path.file_name()?.to_str()?;
    name.strip_suffix(".rrd")?.parse().ok()
}

fn is_uncommitted_ingest_part(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some(staging_name) = name.strip_suffix(".tmp") else {
        return false;
    };
    let Some((committed_name, staging_id)) = staging_name.rsplit_once('.') else {
        return false;
    };
    let Some(sequence) = committed_name.strip_suffix(".rrd") else {
        return false;
    };
    let Ok(sequence) = sequence.parse::<u64>() else {
        return false;
    };
    if committed_name != format!("{sequence:020}.rrd") {
        return false;
    }
    uuid::Uuid::parse_str(staging_id).is_ok_and(|parsed_id| {
        parsed_id.get_version_num() == 7 && parsed_id.to_string() == staging_id
    })
}

pub fn ingest_part_paths(parts_directory: &Path) -> Result<Vec<PathBuf>> {
    if !parts_directory.exists() {
        return Ok(Vec::new());
    }
    let mut parts = std::fs::read_dir(parts_directory)?
        .map(|entry| {
            let entry = entry?;
            let path = entry.path();
            if is_uncommitted_ingest_part(&path) {
                match entry.file_type() {
                    Ok(file_type) => ensure!(
                        file_type.is_file(),
                        "ingest parts directory contains a non-file entry"
                    ),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return Ok(None);
                    }
                    Err(error) => return Err(error.into()),
                }
                return Ok(None);
            }
            ensure!(
                entry.file_type()?.is_file(),
                "ingest parts directory contains a non-file entry"
            );
            if path.file_name().and_then(|name| name.to_str()) == Some(VIDEO_STREAM_MARKER) {
                return Ok(None);
            }
            let sequence = ingest_part_sequence(&path)
                .with_context(|| format!("invalid ingest part name {}", path.display()))?;
            Ok(Some((sequence, path)))
        })
        .filter_map(|entry| match entry {
            Ok(Some(part)) => Some(Ok(part)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    parts.sort_by_key(|(sequence, _)| *sequence);
    for pair in parts.windows(2) {
        ensure!(
            pair[0].0 < pair[1].0,
            "ingest parts contain a duplicate sequence"
        );
    }
    Ok(parts.into_iter().map(|(_, path)| path).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ingest_parts_are_ordered_by_sequence() {
        let directory = tempfile::tempdir().unwrap();
        for sequence in [9, 1, 42] {
            std::fs::write(directory.path().join(format!("{sequence:020}.rrd")), []).unwrap();
        }
        let sequences = ingest_part_paths(directory.path())
            .unwrap()
            .into_iter()
            .map(|path| ingest_part_sequence(&path).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(sequences, [1, 9, 42]);
    }

    #[test]
    fn ingest_parts_exclude_atomic_staging_files() {
        let directory = tempfile::tempdir().unwrap();
        for sequence in [9, 1, 42] {
            std::fs::write(directory.path().join(format!("{sequence:020}.rrd")), []).unwrap();
        }
        std::fs::write(
            directory
                .path()
                .join(format!("{:020}.rrd.{}.tmp", 43, uuid::Uuid::now_v7())),
            [],
        )
        .unwrap();

        let sequences = ingest_part_paths(directory.path())
            .unwrap()
            .into_iter()
            .map(|path| ingest_part_sequence(&path).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(sequences, [1, 9, 42]);
    }

    #[test]
    fn ingest_parts_reject_names_outside_the_atomic_staging_convention() {
        let directory = tempfile::tempdir().unwrap();
        for name in [
            "00000000000000000001.rrd.not-a-uuid.tmp",
            "00000000000000000002.rrd.019fac19-635E-7F52-B081-714F9E364232.tmp",
        ] {
            std::fs::write(directory.path().join(name), []).unwrap();
        }

        let error = ingest_part_paths(directory.path()).unwrap_err();

        assert!(error.to_string().contains("invalid ingest part name"));
    }
}
