use super::*;
use crate::{Handoff, filesystem::FencedHome};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Transition {
    request: Handoff,
    removed_writer: PhysicalWriter,
}
impl Journal {
    fn transition_path(&self, request: &Handoff) -> Result<PathBuf> {
        request.validate()?;
        Ok(self
            .directory(request.target.computer_id)?
            .join("instances")
            .join(format!("{}.json", request.target.instance_id.simple())))
    }
    pub(crate) fn verify_handoff(&self, request: &Handoff) -> Result<()> {
        let path = self.transition_path(request)?;
        private_directory(path.parent().ok_or(StorageError::InvalidIdentity)?)?;
        let transition = read::<Transition>(&path)?.ok_or(StorageError::RecoveryRequired)?;
        transition.removed_writer.validate()?;
        if transition.request != *request
            || transition.removed_writer.resource_id() != request.source_resource_id
        {
            return Err(StorageError::IdentityMismatch);
        }
        Ok(())
    }
    pub(crate) fn check_handoff_target(
        &self,
        request: &Handoff,
        writer: &PhysicalWriter,
    ) -> Result<()> {
        let path = self.transition_path(request)?;
        let parent = path.parent().ok_or(StorageError::InvalidIdentity)?;
        match fs::symlink_metadata(parent) {
            Ok(_) => private_directory(parent)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(StorageError::Unavailable),
        }
        if let Some(prior) = read::<Transition>(&path)?
            && (prior.request != *request || prior.removed_writer != *writer)
        {
            return Err(StorageError::IdentityMismatch);
        }
        Ok(())
    }
    pub(crate) fn commit_handoff(&mut self, request: &Handoff, fenced: FencedHome) -> Result<()> {
        request.validate()?;
        let mut record = self
            .load(request.source.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if record.identity() != &request.source
            || fenced.identity() != &request.source
            || fenced.engine_id() != self.identity.engine_id
            || !matches!(record.writer(), WriterState::Claimed { writer } if writer == fenced.writer())
            || fenced.writer().resource_id() != request.source_resource_id
        {
            return Err(StorageError::IdentityMismatch);
        }
        let path = self.transition_path(request)?;
        let parent = path.parent().ok_or(StorageError::InvalidIdentity)?;
        match fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => sync_directory(parent.parent().ok_or(StorageError::InvalidIdentity)?)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                private_directory(parent)?
            }
            Err(_) => return Err(StorageError::Unavailable),
        }
        let transition = Transition {
            request: request.clone(),
            removed_writer: fenced.writer().clone(),
        };
        match read::<Transition>(&path)? {
            Some(prior) if prior == transition => {}
            Some(_) => return Err(StorageError::IdentityMismatch),
            None => publish(&path, &transition, false)?,
        }
        // The immutable instance record precedes admission, preventing identity
        // reuse. Lost publication replies resolve from this record and the head.
        record.identity = request.target.clone();
        record.writer = WriterState::Unclaimed;
        publish(
            &self
                .directory(request.target.computer_id)?
                .join("record.json"),
            &record,
            true,
        )
    }
}
