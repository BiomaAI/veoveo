use anyhow::{Result, ensure};

use crate::{commands::builder, context::RepositoryContext, process};

pub(crate) const UV_VERSION: &str = "0.11.32";

pub(crate) fn run(repository: &RepositoryContext) -> Result<()> {
    let cargo = process::output_text("cargo", ["--version"], Some(repository.root()))?;
    let git = process::output_text("git", ["--version"], Some(repository.root()))?;
    let docker = process::output_text("docker", ["--version"], Some(repository.root()))?;
    let curl = process::output_text("curl", ["--version"], Some(repository.root()))?;
    let uv = process::output_text("uv", ["--version"], Some(repository.root()))?;
    let buildx = builder::installed_buildx_version(repository)?;
    ensure!(
        buildx == builder::BUILDX_VERSION,
        "Docker Buildx {buildx} is installed; Veoveo requires {}",
        builder::BUILDX_VERSION
    );
    ensure!(
        uv.trim() == format!("uv {UV_VERSION}"),
        "{} is installed; Veoveo requires uv {UV_VERSION}",
        uv.trim()
    );

    println!("{}", first_line(&cargo));
    println!("{}", first_line(&git));
    println!("{}", first_line(&docker));
    println!("{}", first_line(&curl));
    println!("{}", first_line(&uv));
    println!("Docker Buildx {buildx}");
    println!("Repository tool prerequisites are present");
    Ok(())
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value).trim()
}
