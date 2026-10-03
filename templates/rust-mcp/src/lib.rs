//! The Glossary server's public library.
//!
//! `contract` holds the identifiers, resource addresses and tool shapes that
//! clients, tests and tools reuse. It builds with default features disabled and
//! pulls in no MCP or runtime dependency. `runtime` adds the domain logic, and the
//! `mcp` feature builds the hosted server binary on top of both.

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod glossary;
