//! One local writer owns a private, synchronized metadata root. This journal
//! records filesystem outcomes; its lock is not a physical Computer fence.
use crate::{HomeIdentity, HostIdentity, PhysicalWriter, Result, StorageError, WriterState};
use nix::{fcntl::OFlag, unistd::geteuid};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_RECORD_BYTES: u64 = 8192;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum HostSchema {
    #[serde(rename = "veoveo.io/retained-storage-host/v1")]
    V1,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
enum HomeSchema {
    #[serde(rename = "veoveo.io/retained-home/v1")]
    V1,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    schema: HostSchema,
    identity: HostIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackingIdentity {
    device: u64,
    inode: u64,
    length: u64,
}
impl BackingIdentity {
    fn capture(file: &File) -> Result<Self> {
        let metadata = private_file(file)?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
        })
    }
    pub fn matches(&self, file: &File) -> Result<bool> {
        Ok(self == &Self::capture(file)?)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case", deny_unknown_fields)]
pub enum AllocationState {
    Allocating,
    Ready { backing: BackingIdentity },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AllocationRecord {
    schema: HomeSchema,
    identity: HomeIdentity,
    capacity_bytes: u64,
    state: AllocationState,
    writer: WriterState,
}
impl AllocationRecord {
    pub fn identity(&self) -> &HomeIdentity {
        &self.identity
    }
    pub fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }
    pub fn state(&self) -> &AllocationState {
        &self.state
    }
    pub fn writer(&self) -> &WriterState {
        &self.writer
    }
    fn validate(&self, host: &HostIdentity, computer: Uuid) -> Result<()> {
        host.writer(&self.identity)?;
        self.writer.validate()?;
        if matches!(self.state, AllocationState::Allocating)
            && !matches!(self.writer, WriterState::Unclaimed)
        {
            return Err(StorageError::RecoveryRequired);
        }
        if computer != self.identity.computer_id || !valid_capacity(self.capacity_bytes) {
            return Err(StorageError::RecoveryRequired);
        }
        if let AllocationState::Ready { backing } = &self.state
            && (backing.length != self.capacity_bytes || backing.inode == 0)
        {
            return Err(StorageError::RecoveryRequired);
        }
        Ok(())
    }
}

pub struct Journal {
    root: PathBuf,
    identity: HostIdentity,
    _lock: File,
}

/// Only a newly created reservation can begin one physical allocation attempt.
/// An existing Allocating record requires recovery, even when its file is absent.
#[derive(Debug)]
pub enum Reservation {
    Created(AllocationRecord),
    Existing(AllocationRecord),
}
impl Reservation {
    pub fn record(&self) -> &AllocationRecord {
        match self {
            Self::Created(record) | Self::Existing(record) => record,
        }
    }
}
impl Journal {
    /// Bounded local metadata enumeration. Incomplete allocations are retained
    /// and validated; callers choose whether their surface can expose them.
    pub fn list(&self) -> Result<Vec<AllocationRecord>> {
        let mut records = Vec::new();
        for entry in fs::read_dir(self.root.join("homes")).map_err(|_| StorageError::Unavailable)? {
            if records.len() == 4096 {
                return Err(StorageError::CapacityExceeded);
            }
            let entry = entry.map_err(|_| StorageError::Unavailable)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(StorageError::RecoveryRequired)?;
            let id = Uuid::parse_str(name).map_err(|_| StorageError::RecoveryRequired)?;
            if name != id.simple().to_string() {
                return Err(StorageError::RecoveryRequired);
            }
            records.push(self.load(id)?.ok_or(StorageError::RecoveryRequired)?);
        }
        records.sort_by_key(|record| record.identity.computer_id);
        Ok(records)
    }
    pub fn open(root: PathBuf, identity: HostIdentity) -> Result<Self> {
        identity.validate()?;
        if !root.is_absolute() {
            return Err(StorageError::InvalidIdentity);
        }
        match fs::DirBuilder::new().mode(0o700).create(&root) {
            Ok(()) => sync_directory(root.parent().ok_or(StorageError::InvalidIdentity)?)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(StorageError::Unavailable),
        }
        private_directory(&root)?;
        if fs::canonicalize(&root).map_err(|_| StorageError::Unavailable)? != root {
            return Err(StorageError::InvalidIdentity);
        }
        let lock = private_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".lock"))
            .map_err(|_| StorageError::Unavailable)?;
        private_file(&lock)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => StorageError::Busy,
            std::fs::TryLockError::Error(_) => StorageError::Unavailable,
        })?;
        let journal = Self {
            root,
            identity,
            _lock: lock,
        };
        match read::<Header>(&journal.root.join("host.json"))? {
            Some(header) if header.identity == journal.identity => {}
            Some(_) => return Err(StorageError::IdentityMismatch),
            None => {
                // Unknown files or interrupted first publication need explicit
                // recovery. Never adopt a pre-existing home into a new host.
                let entries = fs::read_dir(&journal.root).map_err(|_| StorageError::Unavailable)?;
                for entry in entries {
                    if entry.map_err(|_| StorageError::Unavailable)?.file_name() != ".lock" {
                        return Err(StorageError::RecoveryRequired);
                    }
                }
                publish(
                    &journal.root.join("host.json"),
                    &Header {
                        schema: HostSchema::V1,
                        identity: journal.identity.clone(),
                    },
                    false,
                )?;
            }
        }
        let homes = journal.root.join("homes");
        match fs::DirBuilder::new().mode(0o700).create(&homes) {
            Ok(()) => sync_directory(&journal.root)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(StorageError::Unavailable),
        }
        private_directory(&homes)?;
        Ok(journal)
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub fn directory(&self, computer: Uuid) -> Result<PathBuf> {
        if computer.is_nil() {
            return Err(StorageError::InvalidIdentity);
        }
        Ok(self.root.join("homes").join(computer.simple().to_string()))
    }
    pub fn load(&self, computer: Uuid) -> Result<Option<AllocationRecord>> {
        let directory = self.directory(computer)?;
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(StorageError::Unavailable),
            Ok(_) => private_directory(&directory)?,
        }
        let record = read::<AllocationRecord>(&directory.join("record.json"))?
            .ok_or(StorageError::RecoveryRequired)?;
        record.validate(&self.identity, computer)?;
        Ok(Some(record))
    }
    /// Reserve metadata before allocating bytes. Returning Allocating on a retry
    /// reports incomplete work; it is not permission to format an existing file.
    pub fn reserve(&mut self, identity: HomeIdentity, capacity_bytes: u64) -> Result<Reservation> {
        self.identity.writer(&identity)?;
        if !valid_capacity(capacity_bytes) {
            return Err(StorageError::InvalidIdentity);
        }
        if let Some(existing) = self.load(identity.computer_id)? {
            if existing.identity != identity || existing.capacity_bytes != capacity_bytes {
                return Err(StorageError::IdentityMismatch);
            }
            return Ok(Reservation::Existing(existing));
        }
        if identity.instance_id != identity.computer_id {
            return Err(StorageError::IdentityMismatch);
        }
        let directory = self.directory(identity.computer_id)?;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .map_err(|_| StorageError::Unavailable)?;
        sync_directory(&self.root.join("homes"))?;
        let record = AllocationRecord {
            schema: HomeSchema::V1,
            identity,
            capacity_bytes,
            state: AllocationState::Allocating,
            writer: WriterState::Unclaimed,
        };
        publish(&directory.join("record.json"), &record, false)?;
        Ok(Reservation::Created(record))
    }
    /// Called only after the host backend has verified ext4, mount identity and
    /// home permissions. This records the exact owned backing file; it does not
    /// perform that physical verification on behalf of the backend.
    pub fn record_ready(&mut self, identity: &HomeIdentity) -> Result<AllocationRecord> {
        let mut record = self
            .load(identity.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if &record.identity != identity {
            return Err(StorageError::IdentityMismatch);
        }
        let directory = self.directory(identity.computer_id)?;
        let file = private_options()
            .read(true)
            .open(directory.join("home.ext4"))
            .map_err(|_| StorageError::Unavailable)?;
        let backing = BackingIdentity::capture(&file)?;
        if backing.length != record.capacity_bytes {
            return Err(StorageError::RecoveryRequired);
        }
        match &record.state {
            AllocationState::Ready { backing: prior } if prior == &backing => return Ok(record),
            AllocationState::Ready { .. } => return Err(StorageError::RecoveryRequired),
            AllocationState::Allocating => {}
        }
        file.sync_all().map_err(|_| StorageError::Unavailable)?;
        record.state = AllocationState::Ready { backing };
        publish(&directory.join("record.json"), &record, true)?;
        Ok(record)
    }
    pub(crate) fn claim_writer(
        &mut self,
        identity: &HomeIdentity,
        writer: PhysicalWriter,
    ) -> Result<()> {
        writer.validate()?;
        let mut record = self
            .load(identity.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if record.identity() != identity || !matches!(record.state, AllocationState::Ready { .. }) {
            return Err(StorageError::IdentityMismatch);
        }
        match &record.writer {
            WriterState::Claimed { writer: prior } if prior == &writer => return Ok(()),
            WriterState::Claimed { .. } => return Err(StorageError::WriterDenied),
            WriterState::Unclaimed => {}
        }
        record.writer = WriterState::Claimed { writer };
        publish(
            &self.directory(identity.computer_id)?.join("record.json"),
            &record,
            true,
        )
    }
}

mod handoff;

fn valid_capacity(bytes: u64) -> bool {
    (512 * 1024 * 1024..=256 * 1024 * 1024 * 1024).contains(&bytes) && bytes.is_multiple_of(4096)
}
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .mode(0o600)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK).bits());
    options
}
fn private_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| StorageError::Unavailable)?;
    if !metadata.is_dir()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
    {
        return Err(StorageError::InvalidIdentity);
    }
    Ok(())
}
fn private_file(file: &File) -> Result<fs::Metadata> {
    let metadata = file.metadata().map_err(|_| StorageError::Unavailable)?;
    if !metadata.is_file()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o600
        || metadata.nlink() != 1
    {
        return Err(StorageError::RecoveryRequired);
    }
    Ok(metadata)
}
fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let file = match private_options().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StorageError::Unavailable),
    };
    private_file(&file)?;
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StorageError::Unavailable)?;
    if bytes.len() > MAX_RECORD_BYTES as usize || bytes.first() != Some(&b'{') {
        return Err(StorageError::RecoveryRequired);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| StorageError::RecoveryRequired)
}
fn publish<T: Serialize>(path: &Path, record: &T, replace: bool) -> Result<()> {
    let directory = path.parent().ok_or(StorageError::InvalidIdentity)?;
    let bytes = serde_json::to_vec(record).map_err(|_| StorageError::RecoveryRequired)?;
    if bytes.len() > MAX_RECORD_BYTES as usize {
        return Err(StorageError::RecoveryRequired);
    }
    let temporary = directory.join(format!(".pending-{}", Uuid::now_v7().simple()));
    let mut file = private_options()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| StorageError::Unavailable)?;
    file.write_all(&bytes)
        .map_err(|_| StorageError::Unavailable)?;
    file.sync_all().map_err(|_| StorageError::Unavailable)?;
    if replace {
        fs::rename(&temporary, path).map_err(|_| StorageError::Unavailable)?;
    } else {
        fs::hard_link(&temporary, path).map_err(|_| StorageError::Unavailable)?;
        fs::remove_file(&temporary).map_err(|_| StorageError::Unavailable)?;
    }
    sync_directory(directory)
}
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| StorageError::Unavailable)
}

#[cfg(test)]
mod tests;
