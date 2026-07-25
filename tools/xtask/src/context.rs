use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::process;

#[derive(Debug)]
pub(crate) struct RepositoryContext {
    root: PathBuf,
}

impl RepositoryContext {
    pub(crate) fn discover(start: &Path) -> Result<Self> {
        let root = process::output_text("git", ["rev-parse", "--show-toplevel"], Some(start))
            .context("locating the Veoveo Git worktree")?;
        Ok(Self {
            root: PathBuf::from(root.trim()),
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}
