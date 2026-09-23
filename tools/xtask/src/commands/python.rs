//! Repository-local Python checks; uv owns dependency resolution and packaging.
use crate::{context::RepositoryContext, process};
use anyhow::Result;

pub(crate) fn enforce(repository: &RepositoryContext) -> Result<()> {
    for project in [
        "sdk/python",
        "templates/python-mcp",
        "testing/fixtures/fork-workload",
    ] {
        process::status(
            "uv",
            [
                "run",
                "--directory",
                project,
                "--locked",
                "--all-extras",
                "pytest",
                "-q",
            ],
            Some(repository.root()),
        )?;
    }
    Ok(())
}
