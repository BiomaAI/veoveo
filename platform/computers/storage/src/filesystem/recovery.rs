//! Resume a private, unclaimed allocation only from verified completed bytes.
use super::*;
use crate::{BackingIdentity, WriterState};

impl Filesystem {
    pub(super) async fn recover_allocation(
        &mut self,
        identity: &HomeIdentity,
        capacity: u64,
    ) -> Result<PathBuf> {
        let record = self
            .journal
            .load(identity.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if record.identity() != identity
            || record.capacity_bytes() != capacity
            || !matches!(record.state(), AllocationState::Allocating)
            || !matches!(record.writer(), WriterState::Unclaimed)
        {
            return Err(StorageError::IdentityMismatch);
        }
        let directory = self.journal.directory(identity.computer_id)?;
        let backing = directory.join("home.ext4");
        let file = OpenOptions::new()
            .read(true)
            .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK).bits())
            .open(&backing)
            .map_err(|_| StorageError::RecoveryRequired)?;
        // The allocation directory and file remain private to this locked
        // helper. A guest is never admitted before Ready and writer claim.
        let captured = BackingIdentity::capture(&file)?;
        if file
            .metadata()
            .map_err(|_| StorageError::RecoveryRequired)?
            .len()
            != capacity
        {
            return Err(StorageError::RecoveryRequired);
        }
        verify_filesystem(&backing, identity.computer_id).await?;
        if mounted(&directory.join("mount")).await?.is_none() {
            // Read-only full validation. Incomplete formatting or filesystem
            // damage stays in recovery; this path never repairs or formats.
            command::checked(
                "e2fsck",
                &[OsStr::new("-f"), OsStr::new("-n"), backing.as_os_str()],
            )
            .await?;
        }
        let mounted = attach(&directory, identity.computer_id).await?;
        if !captured.matches(&file)?
            || !captured
                .matches(&File::open(&backing).map_err(|_| StorageError::RecoveryRequired)?)?
        {
            return Err(StorageError::RecoveryRequired);
        }
        for entry in fs::read_dir(&mounted).map_err(|_| StorageError::RecoveryRequired)? {
            let entry = entry.map_err(|_| StorageError::RecoveryRequired)?;
            if ![OsStr::new("home"), OsStr::new("lost+found")]
                .contains(&entry.file_name().as_os_str())
            {
                return Err(StorageError::RecoveryRequired);
            }
        }
        let home = mounted.join("home");
        match fs::DirBuilder::new().mode(0o700).create(&home) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(StorageError::RecoveryRequired),
        }
        let metadata = fs::symlink_metadata(&home).map_err(|_| StorageError::RecoveryRequired)?;
        if !metadata.is_dir() {
            return Err(StorageError::RecoveryRequired);
        }
        if metadata.uid() == 0 && metadata.gid() == 0 {
            if fs::read_dir(&home)
                .map_err(|_| StorageError::RecoveryRequired)?
                .next()
                .is_some()
            {
                return Err(StorageError::RecoveryRequired);
            }
            chown(
                &home,
                Some(Uid::from_raw(HOME_UID)),
                Some(Gid::from_raw(HOME_UID)),
            )
            .map_err(|_| StorageError::BackendUnavailable)?;
        }
        verify_home(&home)?;
        let mount = File::open(&mounted).map_err(|_| StorageError::BackendUnavailable)?;
        nix::unistd::syncfs(mount).map_err(|_| StorageError::BackendUnavailable)?;
        self.journal.record_ready(identity)?;
        eprintln!(
            "retained-storage: recovered completed allocation {}",
            identity.computer_id
        );
        Ok(mounted)
    }
}
