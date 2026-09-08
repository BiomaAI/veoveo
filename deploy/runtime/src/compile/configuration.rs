//! Retained components render with their immutable installation configuration.
use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result, ensure};
use veoveo_deploy_contract::{LoadedProfile, components::*};

use crate::{
    process::{path_str, status_checked},
    snapshot::SnapshotInputs,
    sources::resolve_revision,
};

pub(super) struct ConfigurationSnapshots {
    profiles: BTreeMap<InstallationSnapshot, LoadedProfile>,
    _checkouts: Vec<tempfile::TempDir>,
}

pub(super) fn identity(profile: &LoadedProfile, revision: &str) -> Result<InstallationSnapshot> {
    Ok(InstallationSnapshot {
        source: super::installation_source(profile, revision)?,
        profile: profile
            .path
            .strip_prefix(&profile.repository)
            .context("configuration document is outside its installation repository")?
            .to_str()
            .context("configuration path is not UTF-8")?
            .to_owned(),
    })
}

impl ConfigurationSnapshots {
    pub fn prepare<'a>(
        current: &LoadedProfile,
        requested: impl Iterator<Item = &'a InstallationSnapshot>,
    ) -> Result<Self> {
        let revision = resolve_revision(&current.repository, "HEAD")?;
        let current_identity = identity(current, &revision)?;
        let mut roots =
            BTreeMap::from([(current_identity.source.clone(), current.repository.clone())]);
        let mut profiles = BTreeMap::new();
        let mut checkouts = Vec::new();
        for snapshot in requested {
            if profiles.contains_key(snapshot) {
                continue;
            }
            ensure!(
                snapshot.source.name == INSTALLATION_SOURCE_NAME
                    && snapshot.source.repository == current_identity.source.repository,
                "locked configuration differs from the installation repository"
            );
            let root = if let Some(root) = roots.get(&snapshot.source) {
                root.clone()
            } else {
                let checkout = tempfile::Builder::new()
                    .prefix("veoveo-configuration-")
                    .tempdir()?;
                status_checked(
                    "git",
                    [
                        "clone",
                        "--quiet",
                        "--no-checkout",
                        path_str(&current.repository)?,
                        path_str(checkout.path())?,
                    ],
                    &[("GIT_LFS_SKIP_SMUDGE", "1")],
                    None,
                )?;
                ensure!(
                    resolve_revision(checkout.path(), snapshot.source.revision.as_str())?
                        == snapshot.source.revision.as_str(),
                    "configuration revision did not resolve exactly"
                );
                status_checked(
                    "git",
                    [
                        "checkout",
                        "--quiet",
                        "--detach",
                        snapshot.source.revision.as_str(),
                    ],
                    &[("GIT_LFS_SKIP_SMUDGE", "1")],
                    Some(checkout.path()),
                )?;
                let root = PathBuf::from(checkout.path());
                roots.insert(snapshot.source.clone(), root.clone());
                checkouts.push(checkout);
                root
            };
            let input = SnapshotInputs::new(&root, snapshot.source.revision.as_str())?;
            input.file(&root.join(&snapshot.profile))?;
            let profile = LoadedProfile::load(&root.join(&snapshot.profile), &root)?;
            ensure!(
                profile.definition.name == current.definition.name
                    && profile.definition.namespace == current.definition.namespace
                    && profile.definition.kubernetes.context
                        == current.definition.kubernetes.context
                    && profile.definition.registry.locked() == current.definition.registry.locked(),
                "retained configuration belongs to a different installation destination"
            );
            for path in profile.installation_inputs()? {
                input.file(&path)?;
            }
            profiles.insert(snapshot.clone(), profile);
        }
        Ok(Self {
            profiles,
            _checkouts: checkouts,
        })
    }

    pub fn get(&self, snapshot: &InstallationSnapshot) -> Result<&LoadedProfile> {
        self.profiles
            .get(snapshot)
            .context("component has no immutable installation configuration")
    }
}
