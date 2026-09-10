//! Exact owner-reviewed commands admit public arguments and source dependencies.
use std::{
    ffi::OsString,
    fs,
    path::{Component, Path},
};

use anyhow::{Context, Result, ensure};

use super::{
    model::{
        CATALOG_PATH, CATALOG_SCHEMA, CheckCatalog, CheckDefinition, CommandIdentity, InputScope,
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
    let path = root.join(CATALOG_PATH);
    if !path.exists() {
        return Ok(None);
    }
    ensure!(
        fs::metadata(&path)?.len() <= 1024 * 1024,
        "check catalog exceeds its size bound"
    );
    let catalog: CheckCatalog =
        serde_json::from_slice(&fs::read(path)?).context("decoding check catalog")?;
    ensure!(
        catalog.schema_version == CATALOG_SCHEMA && catalog.checks.len() <= 512,
        "unsupported check catalog"
    );
    let mut identities = std::collections::BTreeSet::new();
    let mut selected = None;
    for definition in catalog.checks {
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
            selected = Some(definition);
        }
    }
    Ok(selected)
}

pub(super) fn check_id(arguments: &[String]) -> Result<String> {
    Ok(digest(&serde_json::to_vec(&CommandIdentity::Admitted {
        arguments: arguments.to_vec(),
    })?))
}
