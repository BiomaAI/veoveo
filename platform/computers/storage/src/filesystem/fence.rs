//! Normal unmount plus verified loop detachment excludes lingering namespace
//! writers. Docker's removed-container result alone cannot establish this fact.
use super::*;
use crate::{Handoff, PhysicalWriter, WriterState, docker::RemovedWriter};

pub(crate) struct FencedHome {
    identity: HomeIdentity,
    removed: RemovedWriter,
}
impl FencedHome {
    pub(crate) fn identity(&self) -> &HomeIdentity {
        &self.identity
    }
    pub(crate) fn writer(&self) -> &PhysicalWriter {
        self.removed.writer()
    }
    pub(crate) fn engine_id(&self) -> Uuid {
        self.removed.engine_id()
    }
}
impl Filesystem {
    pub(crate) fn claim_writer(
        &mut self,
        identity: &HomeIdentity,
        writer: PhysicalWriter,
    ) -> Result<()> {
        self.journal.claim_writer(identity, writer)
    }
    pub(crate) fn commit_handoff(&mut self, request: &Handoff, fenced: FencedHome) -> Result<()> {
        self.journal.commit_handoff(request, fenced)
    }
    pub(crate) async fn fence(
        &mut self,
        identity: &HomeIdentity,
        removed: RemovedWriter,
    ) -> Result<FencedHome> {
        let record = self
            .journal
            .load(identity.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if record.identity() != identity
            || removed.engine_id() != self.journal.identity().engine_id
            || !matches!(record.writer(), WriterState::Claimed { writer } if writer == removed.writer())
        {
            return Err(StorageError::IdentityMismatch);
        }
        let directory = self.journal.directory(identity.computer_id)?;
        let backing = directory.join("home.ext4");
        verify_backing(&record, &backing)?;
        let target = directory.join("mount");
        let devices = associated(&backing).await?;
        if let Some(mount) = mounted(&target).await? {
            verify_device(&devices)?;
            mount.verify(&target, &devices)?;
            {
                let file = File::open(&target).map_err(|_| StorageError::BackendUnavailable)?;
                nix::unistd::syncfs(file).map_err(|_| StorageError::BackendUnavailable)?;
            }
            nix::mount::umount(&target).map_err(|_| StorageError::RecoveryRequired)?;
        }
        if !devices.is_empty() {
            verify_device(&devices)?;
            command::checked("losetup", &[OsStr::new("--detach"), OsStr::new(&devices)]).await?;
        }
        // losetup --detach can report success while autoclear is waiting for
        // another mount's last reference. Never treat that reply as exclusion.
        if !associated(&backing).await?.is_empty() || mounted(&target).await?.is_some() {
            return Err(StorageError::RecoveryRequired);
        }
        Ok(FencedHome {
            identity: identity.clone(),
            removed,
        })
    }
}
async fn associated(backing: &Path) -> Result<String> {
    loop_devices::associated(backing).await
}
fn verify_device(device: &str) -> Result<()> {
    if !device
        .strip_prefix("/dev/loop")
        .is_some_and(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(StorageError::RecoveryRequired);
    }
    Ok(())
}
