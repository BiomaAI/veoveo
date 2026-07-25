use anyhow::Result;

use crate::{context::RepositoryContext, process};

pub(crate) fn rust(repository: &RepositoryContext) -> Result<()> {
    let root = Some(repository.root());
    process::status("cargo", ["fmt", "--all", "--", "--check"], root)?;
    process::status(
        "cargo",
        [
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        root,
    )?;
    process::status(
        "cargo",
        ["test", "--workspace", "--all-features", "--locked"],
        root,
    )?;
    process::status_with_env(
        "cargo",
        [
            "doc",
            "--workspace",
            "--all-features",
            "--no-deps",
            "--locked",
        ],
        &[("RUSTDOCFLAGS", "-D warnings")],
        root,
    )
}
