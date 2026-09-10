//! Exact owner-reviewed commands admit public arguments and source dependencies.
use std::{
    ffi::OsString,
    fs,
    path::{Component, Path},
};

use anyhow::{Context, Result, ensure};

use super::{
    model::{
        CATALOG_DIRECTORY, CATALOG_SCHEMA, CheckCatalog, CheckDefinition, CommandIdentity,
        InputScope,
    },
    storage::digest,
};

pub(super) fn relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && !path.contains(['\n', '\r', '\0', '\\']),
        "invalid input path"
    );
    ensure!(
        Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_))),
        "input must be a confined relative path"
    );
    Ok(())
}

pub(super) fn lookup(root: &Path, arguments: &[OsString]) -> Result<Option<CheckDefinition>> {
    let path = root.join(CATALOG_DIRECTORY);
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(&path)?;
    ensure!(
        metadata.is_dir() && !metadata.is_symlink(),
        "check catalog must be a local directory"
    );
    let mut catalogs = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    catalogs.sort();
    ensure!(catalogs.len() <= 512, "too many check catalogs");
    let mut identities = std::collections::BTreeSet::new();
    let mut selected = None;
    for path in catalogs {
        ensure!(
            path.extension()
                .is_some_and(|extension| extension == "json"),
            "check catalog directory contains an unsupported file"
        );
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file() && !metadata.is_symlink() && metadata.len() <= 1024 * 1024,
            "invalid check catalog file"
        );
        let catalog: CheckCatalog =
            serde_json::from_slice(&fs::read(&path)?).context("decoding check catalog")?;
        ensure!(
            catalog.schema_version == CATALOG_SCHEMA && catalog.checks.len() <= 512,
            "unsupported check catalog"
        );
        let declaration = path
            .strip_prefix(root)?
            .to_str()
            .context("catalog path is not UTF-8")?
            .to_owned();
        for mut definition in catalog.checks {
            ensure!(
                !definition.arguments.is_empty() && definition.arguments.len() <= 128,
                "invalid admitted command"
            );
            ensure!(
                definition.arguments.iter().all(|arg| !arg.is_empty()
                    && arg.len() <= 512
                    && !arg.contains(['\n', '\r', '\0'])),
                "invalid admitted command argument"
            );
            ensure!(
                identities.insert(check_id(&definition.arguments)?),
                "duplicate admitted command"
            );
            match &definition.inputs {
                InputScope::Repository => (),
                InputScope::Cargo { packages, roots } => {
                    ensure!(
                        !packages.is_empty(),
                        "Cargo input declaration requires packages"
                    );
                    for path in roots {
                        relative(path)?;
                    }
                }
                InputScope::Console { roots } => {
                    ensure!(
                        !roots.is_empty(),
                        "Console input declaration requires roots"
                    );
                    for path in roots {
                        relative(path)?;
                    }
                }
            }
            if definition
                .arguments
                .iter()
                .map(OsString::from)
                .eq(arguments.iter().cloned())
            {
                match &mut definition.inputs {
                    InputScope::Repository => (),
                    InputScope::Cargo { roots, .. } | InputScope::Console { roots } => {
                        roots.push(declaration.clone())
                    }
                }
                selected = Some(definition);
            }
        }
    }
    Ok(selected)
}

pub(super) fn check_id(arguments: &[String]) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&CommandIdentity::Admitted {
        arguments: arguments.to_vec(),
    })?))
}
