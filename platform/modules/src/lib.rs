//! Dependency-free checked module declarations; database execution is opt-in.
//!
//! Names cannot be constructed without admission:
//! ```compile_fail
//! let name = veoveo_modules::ModuleName("unadmitted".to_owned());
//! ```
//! Object categories cannot be replaced with unrelated strings:
//! ```compile_fail
//! let claim = veoveo_modules::OwnershipClaim::Table("task".to_owned());
//! ```
//! Prepared execution cannot bypass SQL admission:
//! ```compile_fail
//! use veoveo_modules::runner::PreparedInstallation;
//! fn bypass(selection: veoveo_modules::ModuleSelection<'_>) {
//!     let _ = PreparedInstallation { selection };
//! }
//! ```
mod declaration;
mod names;
mod registry;
pub use declaration::*;
pub use names::*;
pub use registry::*;

#[cfg(feature = "runner")]
pub mod runner;

/// History bookkeeping is infrastructure owned by the Store base.
pub const LANE_TABLE: &str = "platform_module_lane";
pub const PREPARATION_TABLE: &str = "platform_module_installation";
pub const MIGRATION_TABLE: &str = "platform_module_migration";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationError(String);
impl DeclarationError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}
impl std::fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DeclarationError {}

#[cfg(feature = "serialization")]
mod plan;
#[cfg(feature = "serialization")]
pub use plan::*;
