//! Discovery of local RRD layer files used by Hub recovery and smoke evidence.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingLayerFileScope {
    Committed,
    CommittedAndWriting,
}

pub fn collect_recording_layer_files(
    root: &Path,
    scope: RecordingLayerFileScope,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_into(root, scope, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_into(
    directory: &Path,
    scope: RecordingLayerFileScope,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("reading recording layer directory {}", directory.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            let writing_parts = path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.ends_with(".rrd.parts"));
            if writing_parts && scope == RecordingLayerFileScope::Committed {
                continue;
            }
            collect_into(&path, scope, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rrd") {
            files.push(path);
        }
    }
    Ok(())
}
