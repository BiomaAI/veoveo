//! Linux loop devices are global; their device nodes are container-local.
use crate::{Result, StorageError, command};
use nix::{
    fcntl::OFlag,
    sys::stat::{Mode, SFlag, makedev, mknod},
};
use std::{
    ffi::OsStr,
    fs::{self, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    },
    path::Path,
};

// Linux UAPI include/uapi/linux/loop.h. GET_FREE returns an index and does not
// bind a backing file. losetup owns atomic LOOP_CONFIGURE and no-overlap checks.
const LOOP_CTL_GET_FREE: nix::libc::c_ulong = 0x4c82;

fn node(index: u32) -> Result<()> {
    let path = format!("/dev/loop{index}");
    let device = makedev(7, u64::from(index));
    match mknod(
        Path::new(&path),
        SFlag::S_IFBLK,
        Mode::S_IRUSR | Mode::S_IWUSR,
        device,
    ) {
        Ok(()) | Err(nix::errno::Errno::EEXIST) => {}
        Err(_) => return Err(StorageError::BackendUnavailable),
    }
    let metadata = fs::symlink_metadata(&path).map_err(|_| StorageError::BackendUnavailable)?;
    if !metadata.file_type().is_block_device() || metadata.rdev() != device {
        return Err(StorageError::IdentityMismatch);
    }
    Ok(())
}

fn discover() -> Result<()> {
    // Restore nodes for existing associations before asking losetup whether the
    // exact inode is attached. Missing nodes must not hide retained consumers.
    for entry in fs::read_dir("/sys/class/block").map_err(|_| StorageError::BackendUnavailable)? {
        let entry = entry.map_err(|_| StorageError::BackendUnavailable)?;
        let name = entry.file_name();
        if let Some(index) = name
            .to_str()
            .and_then(|name| name.strip_prefix("loop"))
            .and_then(|number| number.parse::<u32>().ok())
        {
            node(index)?;
        }
    }
    Ok(())
}

fn prepare_free_node() -> Result<()> {
    let control = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(OFlag::O_NOFOLLOW.bits())
        .open("/dev/loop-control")
        .map_err(|_| StorageError::BackendUnavailable)?;
    let metadata = control
        .metadata()
        .map_err(|_| StorageError::BackendUnavailable)?;
    if !metadata.file_type().is_char_device() || metadata.rdev() != makedev(10, 237) {
        return Err(StorageError::IdentityMismatch);
    }
    // SAFETY: control is a verified Linux loop-control descriptor. This ioctl
    // takes no pointer argument and returns an integer device index.
    let index = unsafe { nix::libc::ioctl(control.as_raw_fd(), LOOP_CTL_GET_FREE) };
    node(u32::try_from(index).map_err(|_| StorageError::BackendUnavailable)?)
}

pub(super) async fn associated(backing: &Path) -> Result<String> {
    discover()?;
    command::checked(
        "losetup",
        &[
            OsStr::new("--list"),
            OsStr::new("--noheadings"),
            OsStr::new("--output"),
            OsStr::new("NAME"),
            OsStr::new("--associated"),
            backing.as_os_str(),
        ],
    )
    .await
}

pub(super) async fn attach(backing: &Path) -> Result<String> {
    for _ in 0..4 {
        let observed = associated(backing).await?;
        if !observed.is_empty() {
            return Ok(observed);
        }
        prepare_free_node()?;
        let output = command::run(
            "losetup",
            &[
                OsStr::new("--find"),
                OsStr::new("--show"),
                OsStr::new("--nooverlap"),
                backing.as_os_str(),
            ],
        )
        .await?;
        if output.status.success() {
            let observed = associated(backing).await?;
            if observed != output.stdout.trim() || observed.is_empty() {
                return Err(StorageError::RecoveryRequired);
            }
            return Ok(observed);
        }
        // Another host can claim the free index between discovery and setup.
        // Re-read exact backing associations before another no-overlap attempt.
    }
    Err(StorageError::BackendUnavailable)
}
