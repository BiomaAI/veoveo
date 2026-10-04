//! Installation and caller facts drawn from admitted gateway state.
use anyhow::Result;
use veoveo_mcp_gateway::GatewayCatalog;

use crate::contract::authoring::InstallationFacts;

pub use crate::catalog::caller_facts;

/// Read all facts from the same admitted catalog snapshot.
pub fn installation_facts(catalog: &GatewayCatalog) -> Result<InstallationFacts> {
    crate::catalog::installation_facts(catalog.control_plane(), catalog.registry())
}
