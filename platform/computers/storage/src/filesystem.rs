//! Fixed-size ext4 allocations. Formatting is confined to a newly reserved file;
//! ordinary restoration never creates or substitutes retained bytes.
use crate::{
    AllocationRecord, AllocationState, HomeIdentity, Journal, Reservation, Result, StorageError,
    command,
};
use nix::{
    fcntl::OFlag,
    mount::{MsFlags, mount},
    sys::statvfs::statvfs,
    unistd::{Gid, Uid, chown},
};
use serde::Deserialize;
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const HOME_UID: u32 = 10001;
mod fence;
mod loop_devices;
mod recovery;
pub(crate) use fence::{FencedHome, FencedUnclaimedHome};
pub struct Filesystem {
    journal: Journal,
    reserve_bytes: u64,
}
impl Filesystem {
    pub fn new(journal: Journal, reserve_bytes: u64) -> Result<Self> {
        if !nix::unistd::geteuid().is_root() {
            return Err(StorageError::BackendUnavailable);
        }
        // The first retained profile uses a persistent ext-family host volume.
        // Volatile, overlay and remote roots need separate qualification.
        if nix::sys::statfs::statfs(journal.root())
            .map_err(|_| StorageError::BackendUnavailable)?
            .filesystem_type()
            != nix::sys::statfs::EXT4_SUPER_MAGIC
        {
            return Err(StorageError::BackendUnavailable);
        }
        if reserve_bytes < 512 * 1024 * 1024 {
            return Err(StorageError::InvalidIdentity);
        }
        Ok(Self {
            journal,
            reserve_bytes,
        })
    }
    pub fn journal(&self) -> &Journal {
        &self.journal
    }
    pub async fn prepare(
        &mut self,
        identity: HomeIdentity,
        capacity_bytes: u64,
    ) -> Result<PathBuf> {
        if self.journal.load(identity.computer_id)?.is_none() {
            let space =
                statvfs(self.journal.root()).map_err(|_| StorageError::BackendUnavailable)?;
            let free = space
                .blocks_available()
                .checked_mul(space.fragment_size())
                .ok_or(StorageError::CapacityExceeded)?;
            if free
                < capacity_bytes
                    .checked_add(self.reserve_bytes)
                    .ok_or(StorageError::CapacityExceeded)?
            {
                return Err(StorageError::CapacityExceeded);
            }
        }
        match self.journal.reserve(identity.clone(), capacity_bytes)? {
            Reservation::Existing(record) => match record.state() {
                AllocationState::Allocating => {
                    self.recover_allocation(&identity, capacity_bytes).await
                }
                AllocationState::Ready { .. } => self.restore(&identity).await,
            },
            Reservation::Created(_) => {
                let directory = self.journal.directory(identity.computer_id)?;
                let backing = directory.join("home.ext4");
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(OFlag::O_NOFOLLOW.bits())
                    .open(&backing)
                    .map_err(|_| StorageError::RecoveryRequired)?;
                // A lost command result leaves Allocating. No repeat formats it.
                command::checked(
                    "fallocate",
                    &[
                        OsStr::new("--length"),
                        OsStr::new(&capacity_bytes.to_string()),
                        backing.as_os_str(),
                    ],
                )
                .await?;
                command::checked(
                    "mkfs.ext4",
                    &[
                        OsStr::new("-F"),
                        OsStr::new("-q"),
                        OsStr::new("-m"),
                        OsStr::new("0"),
                        OsStr::new("-U"),
                        OsStr::new(&identity.computer_id.to_string()),
                        backing.as_os_str(),
                    ],
                )
                .await?;
                file.sync_all()
                    .map_err(|_| StorageError::BackendUnavailable)?;
                let mounted = attach(&directory, identity.computer_id).await?;
                let home = mounted.join("home");
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&home)
                    .map_err(|_| StorageError::RecoveryRequired)?;
                chown(
                    &home,
                    Some(Uid::from_raw(HOME_UID)),
                    Some(Gid::from_raw(HOME_UID)),
                )
                .map_err(|_| StorageError::BackendUnavailable)?;
                fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
                    .map_err(|_| StorageError::BackendUnavailable)?;
                verify_home(&home)?;
                File::open(&home)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| StorageError::BackendUnavailable)?;
                File::open(&mounted)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| StorageError::BackendUnavailable)?;
                self.journal.record_ready(&identity)?;
                Ok(mounted)
            }
        }
    }
    pub async fn restore(&mut self, identity: &HomeIdentity) -> Result<PathBuf> {
        let record = self
            .journal
            .load(identity.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if record.identity() != identity {
            return Err(StorageError::IdentityMismatch);
        }
        let directory = self.journal.directory(identity.computer_id)?;
        verify_backing(&record, &directory.join("home.ext4"))?;
        let mounted = attach(&directory, identity.computer_id).await?;
        verify_home(&mounted.join("home"))?;
        Ok(mounted)
    }
}

fn verify_backing(record: &AllocationRecord, path: &Path) -> Result<()> {
    let AllocationState::Ready { backing } = record.state() else {
        return Err(StorageError::RecoveryRequired);
    };
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK).bits())
        .open(path)
        .map_err(|_| StorageError::RecoveryRequired)?;
    if !backing.matches(&file)? {
        return Err(StorageError::RecoveryRequired);
    }
    Ok(())
}
fn verify_home(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::RecoveryRequired)?;
    if !metadata.is_dir() || metadata.uid() != HOME_UID || metadata.gid() != HOME_UID {
        return Err(StorageError::RecoveryRequired);
    }
    Ok(())
}

async fn attach(directory: &Path, computer_id: Uuid) -> Result<PathBuf> {
    let backing = directory.join("home.ext4");
    verify_filesystem(&backing, computer_id).await?;
    let device = loop_devices::attach(&backing).await?;
    if !device.strip_prefix("/dev/loop").is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return Err(StorageError::RecoveryRequired);
    }
    let target = directory.join("mount");
    match fs::DirBuilder::new().mode(0o700).create(&target) {
        Ok(()) => File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| StorageError::BackendUnavailable)?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(StorageError::BackendUnavailable),
    }
    if !fs::symlink_metadata(&target)
        .map_err(|_| StorageError::RecoveryRequired)?
        .is_dir()
    {
        return Err(StorageError::RecoveryRequired);
    }
    if let Some(observed) = mounted(&target).await? {
        observed.verify(&target, &device)?;
    } else {
        mount(
            Some(device.as_str()),
            &target,
            Some("ext4"),
            MsFlags::MS_NODEV | MsFlags::MS_NOSUID,
            None::<&str>,
        )
        .map_err(|_| StorageError::BackendUnavailable)?;
        mounted(&target)
            .await?
            .ok_or(StorageError::RecoveryRequired)?
            .verify(&target, &device)?;
    }
    Ok(target)
}

async fn verify_filesystem(backing: &Path, computer_id: Uuid) -> Result<()> {
    let kind = command::checked(
        "blkid",
        &[
            OsStr::new("-p"),
            OsStr::new("-s"),
            OsStr::new("TYPE"),
            OsStr::new("-o"),
            OsStr::new("value"),
            backing.as_os_str(),
        ],
    )
    .await?;
    let uuid = command::checked(
        "blkid",
        &[
            OsStr::new("-p"),
            OsStr::new("-s"),
            OsStr::new("UUID"),
            OsStr::new("-o"),
            OsStr::new("value"),
            backing.as_os_str(),
        ],
    )
    .await?;
    if kind != "ext4" || uuid != computer_id.to_string() {
        return Err(StorageError::RecoveryRequired);
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MountTable {
    filesystems: Vec<Mounted>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mounted {
    source: String,
    target: PathBuf,
    fstype: String,
    options: String,
}
impl Mounted {
    fn verify(&self, target: &Path, device: &str) -> Result<()> {
        if self.target != target
            || self.source != device
            || self.fstype != "ext4"
            || !["rw", "nodev", "nosuid"]
                .iter()
                .all(|required| self.options.split(',').any(|option| option == *required))
        {
            return Err(StorageError::RecoveryRequired);
        }
        Ok(())
    }
}
async fn mounted(target: &Path) -> Result<Option<Mounted>> {
    let result = command::run(
        "findmnt",
        &[
            OsStr::new("--json"),
            OsStr::new("--mountpoint"),
            target.as_os_str(),
            OsStr::new("--output"),
            OsStr::new("SOURCE,TARGET,FSTYPE,OPTIONS"),
        ],
    )
    .await?;
    if result.status.code() == Some(1) && result.stdout.is_empty() && result.stderr_empty {
        return Ok(None);
    }
    if !result.status.success() {
        return Err(StorageError::BackendUnavailable);
    }
    let mut table: MountTable =
        serde_json::from_str(&result.stdout).map_err(|_| StorageError::BackendUnavailable)?;
    if table.filesystems.len() != 1 {
        return Err(StorageError::RecoveryRequired);
    }
    Ok(table.filesystems.pop())
}
