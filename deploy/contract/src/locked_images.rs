//! Qualified image versions retain one target/repository owner across revisions.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};

use crate::{LockedSource, validate_digest, validate_name};

pub(crate) fn validate(sources: &[LockedSource], registry: &str) -> Result<()> {
    let mut repositories = BTreeMap::new();
    let prefix = format!("{registry}/");
    for source in sources {
        let mut versions = BTreeSet::new();
        let mut targets = BTreeMap::new();
        for image in &source.images {
            validate_name("locked image", &image.name)?;
            ensure!(
                versions.insert((&image.name, &image.source_revision)),
                "locked image target {}:{} repeats a build revision",
                source.name,
                image.name
            );
            ensure!(
                image.repository.starts_with(&prefix)
                    && image.repository.len() > prefix.len()
                    && !image.repository.contains('@')
                    && !image.repository.chars().any(char::is_whitespace)
                    && !image
                        .repository
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .contains(':'),
                "locked image repository must be untagged and inside the declared pull registry"
            );
            if let Some(previous) = targets.insert(&image.name, &image.repository) {
                ensure!(
                    previous == &image.repository,
                    "locked image target changes repository across revisions"
                );
            }
            let owner = (&source.name, &image.name);
            if let Some(previous) = repositories.insert(&image.repository, owner) {
                ensure!(
                    previous == owner,
                    "locked image repository {} is owned by both {}:{} and {}:{}",
                    image.repository,
                    previous.0,
                    previous.1,
                    owner.0,
                    owner.1
                );
            }
            validate_digest(&image.digest)?;
            validate_digest(&image.publication_digest)?;
            ensure!(
                image.digest != image.publication_digest,
                "locked image {} must distinguish its runnable manifest digest from its attested publication digest",
                image.name
            );
        }
    }
    Ok(())
}
