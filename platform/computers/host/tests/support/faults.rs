//! Faults run only in the owned native fixture's private host namespace.
use anyhow::{Result, ensure};
use std::{
    fs::{self, File},
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::PathBuf,
    process::Command,
};

fn command(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).output()?;
    ensure!(
        output.status.success(),
        "fixture fault command {program} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}
fn hide_nodes() -> Result<()> {
    ensure!(
        command("findmnt", &["-n", "-o", "FSTYPE", "--mountpoint", "/dev"])? == "tmpfs",
        "fixture /dev must be private tmpfs"
    );
    let mut removed = 0;
    for entry in fs::read_dir("/dev")? {
        let entry = entry?;
        let name = entry.file_name();
        if name
            .to_str()
            .and_then(|name| name.strip_prefix("loop"))
            .is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            ensure!(
                entry.file_type()?.is_block_device(),
                "unexpected loop node type"
            );
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    ensure!(removed > 0, "fixture had no device nodes to remove");
    Ok(())
}
pub fn run(mode: &str) -> Result<()> {
    let computer: uuid::Uuid = std::env::var("VEOVEO_HOST_PROBE_COMPUTER")?.parse()?;
    let directory = PathBuf::from("/var/lib/veoveo-computers/state/retained/homes")
        .join(computer.simple().to_string());
    match mode {
        "hide-loop-nodes" => hide_nodes(),
        "interrupt-allocation" => {
            let record = directory.join("record.json");
            let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&record)?)?;
            ensure!(
                value["writer"]["status"] == "unclaimed" && value["state"]["stage"] == "ready",
                "fault requires newly prepared unclaimed fixture"
            );
            let inode = fs::metadata(directory.join("home.ext4"))?.ino();
            fs::write(directory.join("fault-inode"), inode.to_string())?;
            let marker = directory.join("mount/home/allocation-marker");
            fs::write(&marker, b"retained before Ready acknowledgement")?;
            File::open(&marker)?.sync_all()?;
            value["state"] = serde_json::json!({"stage": "allocating"});
            fs::write(&record, serde_json::to_vec(&value)?)?;
            File::open(record)?.sync_all()?;
            let mount = directory.join("mount");
            let mount = mount.to_str().unwrap();
            let device = command("findmnt", &["-n", "-o", "SOURCE", "--mountpoint", mount])?;
            ensure!(
                device
                    .strip_prefix("/dev/loop")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
                "exact fixture loop required"
            );
            command("umount", &[mount])?;
            command("losetup", &["--detach", &device])?;
            hide_nodes()
        }
        "assert-recovery" => {
            let record: serde_json::Value =
                serde_json::from_slice(&fs::read(directory.join("record.json"))?)?;
            ensure!(
                record["state"]["stage"] == "ready" && record["writer"]["status"] == "unclaimed",
                "recovery changed writer admission"
            );
            ensure!(
                fs::metadata(directory.join("home.ext4"))?.ino().to_string()
                    == fs::read_to_string(directory.join("fault-inode"))?,
                "recovery replaced backing inode"
            );
            ensure!(
                fs::read(directory.join("mount/home/allocation-marker"))?
                    == b"retained before Ready acknowledgement",
                "recovery lost retained bytes"
            );
            Ok(())
        }
        _ => anyhow::bail!("unknown owned fixture fault"),
    }
}
