//! Shared pure authorization rules. Authentication and durable freshness are callers' responsibilities.
mod catalog;
mod evaluation;
mod resource_policy;
mod resource_reads;
pub use resource_reads::admit_resource_reads;
pub mod session;
pub use catalog::{PolicyCatalog, PolicyCatalogView};
pub use evaluation::{
    PolicyRequest, RuleMatchDetail, RuleOutcome, assemble_rule_outcome, decide, exposure_contains,
    intersects, mcp_method_name, principal_rule_conditions, remember_strongest_missing_requirement,
    resource_scheme_from_uri, strongest_missing_rule_detail,
};
