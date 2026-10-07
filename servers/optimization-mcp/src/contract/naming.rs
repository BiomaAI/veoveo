//! Schema roles for the exact current Optimization document tags.
use schemars::{Schema, SchemaGenerator};
use veoveo_types::naming::{ScalarGrammar, ScalarNaming, scalar_schema};

fn tags(values: &[&str]) -> Schema {
    scalar_schema(
        Schema::try_from(serde_json::json!({"type":"string", "enum":values}))
            .expect("declared format tag schema"),
        ScalarNaming::builtin(ScalarGrammar::FormatTag),
    )
    .expect("declared scalar naming profile")
}
pub(super) fn routing_version(_: &mut SchemaGenerator) -> Schema {
    tags(&[super::ROUTING_PROBLEM_VERSION])
}
pub(super) fn convex_version(_: &mut SchemaGenerator) -> Schema {
    tags(&[super::CONVEX_PROBLEM_VERSION])
}
pub(super) fn milp_version(_: &mut SchemaGenerator) -> Schema {
    tags(&[super::MILP_PROBLEM_VERSION])
}
pub(super) fn problem_version(_: &mut SchemaGenerator) -> Schema {
    tags(&[
        super::ROUTING_PROBLEM_VERSION,
        super::CONVEX_PROBLEM_VERSION,
        super::MILP_PROBLEM_VERSION,
    ])
}
pub(crate) fn executor_version(_: &mut SchemaGenerator) -> Schema {
    tags(&[super::EXECUTOR_PROTOCOL_VERSION])
}
