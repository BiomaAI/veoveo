//! Explicit installation recovery proof over the four drained pilot exports.
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::PathBuf,
    process::{Command, Output, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(PartialEq, Eq)]
struct RestoredEntry {
    mode: u32,
    uid: u64,
    gid: u64,
    modified: u64,
    kind: u8,
    link: Option<PathBuf>,
    contents: Vec<u8>,
}

fn archive_entries(reader: impl Read) -> Result<BTreeMap<PathBuf, RestoredEntry>> {
    let mut result = BTreeMap::new();
    for entry in tar::Archive::new(reader).entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut restored = RestoredEntry {
            mode: entry.header().mode()?,
            uid: entry.header().uid()?,
            gid: entry.header().gid()?,
            modified: entry.header().mtime()?,
            kind: entry.header().entry_type().as_byte(),
            link: entry.link_name()?.map(|name| name.into_owned()),
            contents: Vec::new(),
        };
        entry.read_to_end(&mut restored.contents)?;
        assert!(
            result.insert(path, restored).is_none(),
            "duplicate archive path"
        );
    }
    Ok(result)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema: String,
    source_namespace: String,
    exported_at: String,
    volumes: Vec<Volume>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Volume {
    agent: String,
    volume: String,
    source_path: String,
    archive: String,
    sha256: String,
    bytes: u64,
}

fn successful(output: Output) -> Result<Vec<u8>> {
    if !output.status.success() {
        return Err(format!(
            "recovery command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output.stdout)
}

struct RestoreDirectory {
    node: String,
    path: String,
}
impl Drop for RestoreDirectory {
    fn drop(&mut self) {
        let _ = Command::new("timeout")
            .args([
                "30s", "docker", "exec", &self.node, "rm", "-rf", "--", &self.path,
            ])
            .output();
    }
}

#[test]
#[ignore = "requires the private drained pilot export and the installation Docker node"]
fn drained_pilot_archives_restore_with_identical_contents_and_metadata() -> Result<()> {
    let root = PathBuf::from(std::env::var("VEOVEO_PILOT_EXPORT_DIRECTORY")?).canonicalize()?;
    let node = std::env::var("VEOVEO_PILOT_EXPORT_NODE")?;
    let manifest: Manifest =
        serde_json::from_reader(File::open(root.join("archive-manifest.json"))?)?;
    assert_eq!(manifest.schema, "bioma-pilot-cutover/v1");
    assert_eq!(manifest.source_namespace, "veoveo");
    assert!(!manifest.exported_at.is_empty());
    assert_eq!(manifest.volumes.len(), 4);
    for (index, volume) in manifest.volumes.iter().enumerate() {
        assert_eq!(volume.agent, format!("uav-{}-pilot", index + 1));
        assert_eq!(volume.archive, format!("{}.tar", volume.agent));
        assert!(volume.volume.starts_with("pvc-"));
        assert!(volume.source_path.ends_with(&volume.agent));
        let archive = root.join(&volume.archive);
        assert_eq!(archive.metadata()?.len(), volume.bytes);
        let digest = successful(
            Command::new("timeout")
                .args(["30s", "sha256sum"])
                .arg(&archive)
                .output()?,
        )?;
        assert_eq!(
            String::from_utf8(digest)?.split_whitespace().next(),
            Some(volume.sha256.as_str())
        );
        let directory = successful(
            Command::new("timeout")
                .args([
                    "30s",
                    "docker",
                    "exec",
                    &node,
                    "mktemp",
                    "-d",
                    "/tmp/veoveo-pilot-restore.XXXXXXXX",
                ])
                .output()?,
        )?;
        let path = String::from_utf8(directory)?.trim().to_owned();
        assert!(path.starts_with("/tmp/veoveo-pilot-restore."));
        assert!(!path.contains(char::is_whitespace));
        let restored = RestoreDirectory {
            node: node.clone(),
            path,
        };
        let expected = archive_entries(File::open(&archive)?)?;
        successful(
            Command::new("timeout")
                .args([
                    "30s",
                    "docker",
                    "exec",
                    "-i",
                    &node,
                    "tar",
                    "-C",
                    &restored.path,
                    "-xf",
                    "-",
                ])
                .stdin(Stdio::from(File::open(&archive)?))
                .output()?,
        )?;
        // BusyBox updates directory timestamps while extracting their children.
        // Restore those timestamps only after every archive entry is in place.
        for (path, entry) in &expected {
            if entry.kind == b'5' {
                successful(
                    Command::new("timeout")
                        .args([
                            "30s",
                            "docker",
                            "exec",
                            &node,
                            "touch",
                            "-m",
                            "-d",
                            &format!("@{}", entry.modified),
                            &PathBuf::from(&restored.path).join(path).to_string_lossy(),
                        ])
                        .output()?,
                )?;
            }
        }
        let restored_archive = successful(
            Command::new("timeout")
                .args([
                    "30s",
                    "docker",
                    "exec",
                    &node,
                    "tar",
                    "-C",
                    &restored.path,
                    "-cf",
                    "-",
                    ".",
                ])
                .output()?,
        )?;
        let actual = archive_entries(restored_archive.as_slice())?;
        assert!(
            expected.keys().eq(actual.keys()),
            "restored file inventory differs"
        );
        for (path, entry) in expected {
            let actual = actual.get(&path).ok_or("restored entry missing")?;
            assert_eq!(
                (
                    entry.mode,
                    entry.uid,
                    entry.gid,
                    entry.modified,
                    entry.kind,
                    &entry.link
                ),
                (
                    actual.mode,
                    actual.uid,
                    actual.gid,
                    actual.modified,
                    actual.kind,
                    &actual.link
                ),
                "restored metadata differs: {path:?}"
            );
            assert!(
                entry.contents == actual.contents,
                "restored contents differ: {path:?}"
            );
        }
        eprintln!(
            "{}: exact archive hash, restored contents and metadata verified",
            volume.agent
        );
    }
    Ok(())
}
