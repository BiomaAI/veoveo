use std::process::Command;

use anyhow::Result;
use veoveo_deploy_contract::RegistryTransport;
pub(crate) use veoveo_image_build_control::{BUILDER_NAME, BUILDX_VERSION, BuilderLease};

use crate::context::RepositoryContext;

pub(crate) fn status(repository: &RepositoryContext) -> Result<()> {
    veoveo_image_build_control::status(repository.root())
}

pub(crate) fn ensure(repository: &RepositoryContext) -> Result<BuilderLease> {
    veoveo_image_build_control::ensure(repository.root())
}

pub(crate) fn ensure_for_registry(
    repository: &RepositoryContext,
    registry: &str,
    transport: RegistryTransport,
) -> Result<BuilderLease> {
    veoveo_image_build_control::ensure_for_registry(repository.root(), registry, transport)
}

pub(crate) fn reconfigure(repository: &RepositoryContext, confirmation: &str) -> Result<()> {
    veoveo_image_build_control::reconfigure(repository.root(), confirmation)
}

pub(crate) fn recreate(repository: &RepositoryContext, confirmation: &str) -> Result<()> {
    veoveo_image_build_control::recreate(repository.root(), confirmation)
}

pub(crate) fn installed_buildx_version(repository: &RepositoryContext) -> Result<String> {
    veoveo_image_build_control::installed_buildx_version(repository.root())
}

pub(crate) fn buildx_command(repository: &RepositoryContext) -> Result<Command> {
    veoveo_image_build_control::buildx_command(repository.root())
}
