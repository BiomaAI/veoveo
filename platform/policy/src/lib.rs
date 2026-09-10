//! Shared pure authorization rules. Authentication and durable freshness are callers' responsibilities.
mod catalog;
mod evaluation;
pub use catalog::{PolicyCatalog, PolicyCatalogView};
pub use evaluation::{
    PolicyRequest, RecordingIngestPolicyDecision, RecordingIngestPolicyRequest, decide,
    decide_recording_ingest, exposure_contains, mcp_method_name, resource_scheme_from_uri,
};
