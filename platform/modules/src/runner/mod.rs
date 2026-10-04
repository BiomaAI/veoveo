//! Private pinned SQL adapter and prepared-installation execution.
mod executor;
mod preparation;
pub use preparation::*;
mod policy;
use crate::*;
pub use executor::*;

#[derive(Debug)]
pub struct RunnerError(String);
impl RunnerError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for RunnerError {}

/// Only complete admission can construct this execution capability.
#[derive(Debug)]
pub struct PreparedInstallation<'a> {
    selection: ModuleSelection<'a>,
}
impl<'a> PreparedInstallation<'a> {
    pub(crate) fn selection(&self) -> &ModuleSelection<'a> {
        &self.selection
    }
}
pub fn prepare(selection: ModuleSelection<'_>) -> Result<PreparedInstallation<'_>, RunnerError> {
    policy::admit(&selection)?;
    Ok(PreparedInstallation { selection })
}
