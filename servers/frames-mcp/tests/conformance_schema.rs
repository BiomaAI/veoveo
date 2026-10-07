//! Actual owner DTOs qualify the generic tool schema profile.
#[test]
fn actual_owner_schemas_pass_the_shared_profile() {
    for schema in [
        serde_json::to_value(schemars::schema_for!(
            veoveo_frames_mcp::contract::BatchTransformRequest
        ))
        .unwrap(),
        serde_json::to_value(schemars::schema_for!(
            veoveo_frames_mcp::contract::PublishWorldRequest
        ))
        .unwrap(),
    ] {
        let tool = rmcp::model::Tool::new(
            "owner_input",
            "Actual owner argument schema",
            std::sync::Arc::new(schema.as_object().unwrap().clone()),
        );
        veoveo_mcp_conformance::validate_tool_input_schema(&tool).unwrap();
    }
}
