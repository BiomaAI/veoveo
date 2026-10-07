//! Validate advertised input schemas with current owner requests, including local references.
use super::*;
use rmcp::handler::server::common::schema_for_input;
use rmcp::model::Tool;
use veoveo_view_mcp::contract::CloseViewRequest;

fn request_fixture(name: &str) -> Result<Value> {
    let view_id = ViewId::parse("view-1")?;
    Ok(match name {
        "create_scene_composition" => {
            serde_json::to_value(composition_request(LOCAL_LAYER, true)?)?
        }
        "create_view" => serde_json::to_value(CreateViewRequest {
            composition_id: SceneCompositionId::parse(
                "composition-281b253a-55b2-5b69-8f2b-cc214e0be326",
            )?,
            camera: local_camera(),
        })?,
        "set_camera" => serde_json::to_value(SetCameraRequest {
            view_id,
            expected_revision: 2,
            camera: local_camera(),
        })?,
        "capture_frame" => capture_request("view-1", 2, false)?,
        "close_view" => serde_json::to_value(CloseViewRequest {
            view_id,
            expected_revision: 2,
        })?,
        _ => bail!("no owning request fixture for `{name}`"),
    })
}

pub(super) fn assert_tool_schema(tool: &Tool) -> Result<()> {
    // Admit the repository's bounded, local-reference profile before compiling it.
    veoveo_mcp_conformance::validate_tool_input_schema(tool).with_context(|| {
        format!(
            "`{}` input schema violates the supported profile",
            tool.name
        )
    })?;
    let document = Value::Object(tool.input_schema.as_ref().clone());
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&document)
        .with_context(|| format!("cannot compile `{}` input schema", tool.name))?;
    let request = request_fixture(&tool.name)?;
    validator
        .validate(&request)
        .map_err(|error| anyhow!("`{}` schema refuses its owning request: {error}", tool.name))?;
    let mut unknown = request.clone();
    unknown["retiredUnknownField"] = json!(true);
    ensure!(
        !validator.is_valid(&unknown),
        "`{}` admits unknown request fields",
        tool.name
    );
    // Required fields come from the same owner generator as the server's tool router.
    let owner = owner_input_schema(&tool.name)?;
    for field in owner["required"]
        .as_array()
        .context("owner schema omitted required fields")?
    {
        let field = field.as_str().context("owner required field is not text")?;
        let mut missing = request.clone();
        missing.as_object_mut().unwrap().remove(field);
        ensure!(
            !validator.is_valid(&missing),
            "`{}` admits missing `{field}`",
            tool.name
        );
    }
    let structured = match tool.name.as_ref() {
        "create_view" | "set_camera" => Some("camera"),
        "capture_frame" => Some("policy"),
        _ => None,
    };
    if let Some(field) = structured {
        for invalid in [
            Value::Null,
            json!(true),
            json!(1),
            json!("retired-string-form"),
            json!([]),
            json!({}),
        ] {
            let mut malformed = request.clone();
            malformed[field] = invalid;
            ensure!(
                !validator.is_valid(&malformed),
                "`{}.{field}` admits a malformed object",
                tool.name
            );
        }
        let mut unknown = request.clone();
        unknown[field]["retiredUnknownField"] = json!(true);
        ensure!(
            !validator.is_valid(&unknown),
            "`{}.{field}` admits unknown fields",
            tool.name
        );
    }
    if tool.name == "capture_frame" {
        let mut retired = request.clone();
        retired["policy"]["encoding"] = json!("retired-format");
        ensure!(
            !validator.is_valid(&retired),
            "capture_frame admits an unknown encoding"
        );
    }
    if tool.name == "create_scene_composition" {
        let mut retired = request.clone();
        retired["schemaVersion"] = json!(1);
        ensure!(
            !validator.is_valid(&retired),
            "composition admits a retired schema version"
        );
    }
    Ok(())
}

fn owner_input_schema(name: &str) -> Result<std::sync::Arc<rmcp::model::JsonObject>> {
    let schema = match name {
        "create_scene_composition" => schema_for_input::<CreateSceneCompositionRequest>(),
        "create_view" => schema_for_input::<CreateViewRequest>(),
        "set_camera" => schema_for_input::<SetCameraRequest>(),
        "capture_frame" => schema_for_input::<CaptureFrameRequest>(),
        "close_view" => schema_for_input::<CloseViewRequest>(),
        _ => bail!("no owning input schema for `{name}`"),
    };
    schema.map_err(|error| anyhow!(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_tool<T: schemars::JsonSchema + 'static>(name: &'static str) -> Tool {
        Tool::new(
            name,
            "owner schema fixture",
            schema_for_input::<T>().unwrap(),
        )
    }

    #[test]
    fn all_current_owner_requests_admit_through_full_schemas() {
        for tool in [
            owner_tool::<CreateSceneCompositionRequest>("create_scene_composition"),
            owner_tool::<CreateViewRequest>("create_view"),
            owner_tool::<SetCameraRequest>("set_camera"),
            owner_tool::<CaptureFrameRequest>("capture_frame"),
            owner_tool::<CloseViewRequest>("close_view"),
        ] {
            assert_tool_schema(&tool).unwrap();
        }
    }

    #[test]
    fn local_camera_and_policy_references_are_supported() {
        for tool in [
            owner_tool::<CreateViewRequest>("create_view"),
            owner_tool::<SetCameraRequest>("set_camera"),
            owner_tool::<CaptureFrameRequest>("capture_frame"),
        ] {
            let field = if tool.name == "capture_frame" {
                "policy"
            } else {
                "camera"
            };
            assert!(tool.input_schema["properties"][field].get("$ref").is_some());
            assert_tool_schema(&tool).unwrap();
        }
    }

    #[test]
    fn unresolved_external_and_nonobject_references_refuse() {
        for replacement in [
            json!({"$ref":"#/$defs/Missing"}),
            json!({"$ref":"https://invalid.example/camera"}),
            json!({"$ref":"#/$defs/NotCamera"}),
        ] {
            let mut tool = owner_tool::<CreateViewRequest>("create_view");
            let mut document = Value::Object(tool.input_schema.as_ref().clone());
            document["properties"]["camera"] = replacement;
            document["$defs"]["NotCamera"] = json!({"type":"string"});
            tool.input_schema = std::sync::Arc::new(document.as_object().unwrap().clone());
            assert!(assert_tool_schema(&tool).is_err());
        }
    }

    #[test]
    fn permissive_camera_or_unknown_fields_cannot_manufacture_success() {
        for replacement in [
            json!({}),
            json!({"type":"object"}),
            json!({"type":["object","string"]}),
        ] {
            let mut tool = owner_tool::<CreateViewRequest>("create_view");
            let mut document = Value::Object(tool.input_schema.as_ref().clone());
            document["properties"]["camera"] = replacement;
            tool.input_schema = std::sync::Arc::new(document.as_object().unwrap().clone());
            assert!(assert_tool_schema(&tool).is_err());
        }
    }
}
