use super::*;
use crate::{Abandonment, filesystem::FencedUnclaimedHome};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AbandonedAdmission {
    abandonment: Abandonment,
    engine_id: Uuid,
}
impl Journal {
    fn abandonment_path(&self, request: &Abandonment) -> Result<PathBuf> {
        request.validate()?;
        // Shares the handoff identity namespace: no transition can reuse an
        // instance that either kind of maintenance previously admitted.
        Ok(self
            .directory(request.target.computer_id)?
            .join("instances")
            .join(format!("{}.json", request.target.instance_id.simple())))
    }
    pub(crate) fn verify_abandonment(&self, request: &Abandonment) -> Result<()> {
        let path = self.abandonment_path(request)?;
        private_directory(path.parent().ok_or(StorageError::InvalidIdentity)?)?;
        let prior = read::<AbandonedAdmission>(&path)?.ok_or(StorageError::RecoveryRequired)?;
        if prior.abandonment != *request || prior.engine_id != self.identity.engine_id {
            return Err(StorageError::IdentityMismatch);
        }
        Ok(())
    }
    pub(crate) fn check_abandonment_target(&self, request: &Abandonment) -> Result<()> {
        let path = self.abandonment_path(request)?;
        match fs::symlink_metadata(path.parent().ok_or(StorageError::InvalidIdentity)?) {
            Ok(_) => {
                private_directory(path.parent().ok_or(StorageError::InvalidIdentity)?)?;
                if let Some(prior) = read::<AbandonedAdmission>(&path)?
                    && (prior.abandonment != *request || prior.engine_id != self.identity.engine_id)
                {
                    return Err(StorageError::IdentityMismatch);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(StorageError::Unavailable),
        }
        Ok(())
    }
    pub(crate) fn commit_abandonment(
        &mut self,
        request: &Abandonment,
        fenced: FencedUnclaimedHome,
    ) -> Result<()> {
        request.validate()?;
        let mut record = self
            .load(request.source.computer_id)?
            .ok_or(StorageError::RecoveryRequired)?;
        if record.identity() != &request.source
            || fenced.identity() != &request.source
            || fenced.engine_id() != self.identity.engine_id
            || record.writer() != &WriterState::Unclaimed
        {
            return Err(StorageError::IdentityMismatch);
        }
        let path = self.abandonment_path(request)?;
        let parent = path.parent().ok_or(StorageError::InvalidIdentity)?;
        match fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => sync_directory(parent.parent().ok_or(StorageError::InvalidIdentity)?)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                private_directory(parent)?
            }
            Err(_) => return Err(StorageError::Unavailable),
        }
        let transition = AbandonedAdmission {
            abandonment: request.clone(),
            engine_id: fenced.engine_id(),
        };
        match read::<AbandonedAdmission>(&path)? {
            Some(prior) if prior == transition => {}
            Some(_) => return Err(StorageError::IdentityMismatch),
            None => publish(&path, &transition, false)?,
        }
        record.identity = request.target.clone();
        publish(
            &self
                .directory(request.target.computer_id)?
                .join("record.json"),
            &record,
            true,
        )
    }
}
