use anyhow::Result;

use crate::{
    commands::{
        doc_links, identifiers as identifier_policy, macro_policy, python as python_package,
    },
    context::RepositoryContext,
    process,
};

pub(crate) fn rust(repository: &RepositoryContext, macros_only: bool) -> Result<()> {
    macro_policy::enforce(repository)?;
    if macros_only {
        return Ok(());
    }
    let root = Some(repository.root());
    process::cargo_status(["fmt", "--all", "--", "--check"], root)?;
    process::cargo_status(
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
    process::cargo_status(["test", "--workspace", "--all-features", "--locked"], root)?;
    process::cargo_status_with_env(
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

pub(crate) fn python(repository: &RepositoryContext) -> Result<()> {
    python_package::enforce(repository)
}

pub(crate) fn docs(repository: &RepositoryContext) -> Result<()> {
    doc_links::enforce(repository)
}

pub(crate) fn identifiers(repository: &RepositoryContext) -> Result<()> {
    identifier_policy::enforce(repository)
}
