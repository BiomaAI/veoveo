//! Actual owner DTOs qualify the generic tool schema profile.
#[test]
fn actual_owner_schemas_pass_the_shared_profile() {
    for schema in [
        serde_json::to_value(schemars::schema_for!(veoveo_media_mcp::contract::RunArgs)).unwrap(),
    ] {
        let tool = rmcp::model::Tool::new(
            "owner_input",
            "Actual owner argument schema",
            std::sync::Arc::new(schema.as_object().unwrap().clone()),
        );
        veoveo_mcp_conformance::validate_tool_input_schema(&tool).unwrap();
    }
}

#[test]
fn actual_generation_format_tag_passes_documented_naming_receiver() {
    let root = schemars::schema_for!(veoveo_media_mcp::contract::MediaGenerationResult);
    let mut node = &root.as_value()["properties"]["schema"];
    if let Some(reference) = node["$ref"].as_str() {
        node = root
            .as_value()
            .pointer(reference.strip_prefix('#').unwrap())
            .unwrap();
    }
    let tag = schemars::Schema::try_from(node.clone()).unwrap();
    assert_eq!(
        veoveo_types::naming_profile(&tag, veoveo_types::NamingSchemaContext::new(&tag))
            .unwrap()
            .unwrap()
            .role(),
        &veoveo_types::NamingRole::Scalar {
            profile: veoveo_types::ScalarNaming::builtin(veoveo_types::ScalarGrammar::FormatTag)
        }
    );
    let mut inspection = veoveo_mcp_conformance::naming::NamingInspection::default();
    inspection
        .schema(
            "Media generation format tag",
            &tag,
            None,
            veoveo_mcp_conformance::SchemaEvidenceOrigin::SourceOnly,
        )
        .unwrap();
    let mut retired = tag.clone();
    retired
        .as_object_mut()
        .unwrap()
        .remove(veoveo_types::naming::NAMING_PROFILE_KEY);
    assert!(
        veoveo_mcp_conformance::naming::NamingInspection::default()
            .schema(
                "unannotated format tag",
                &retired,
                None,
                veoveo_mcp_conformance::SchemaEvidenceOrigin::SourceOnly
            )
            .is_err()
    );
}
