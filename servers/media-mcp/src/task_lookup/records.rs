//! Media request normalization and lossless admitted completion receipts.
use crate::contract::{MediaGenerationResult, RunArgs};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_platform_store::{native_json_from_value_strict, native_json_into_value};
use veoveo_task_runtime::TaskError;

pub(super) struct RunRequestRecord(RunArgs);
impl RunRequestRecord {
    pub(super) fn new(value: serde_json::Value) -> Result<Self, TaskError> {
        serde_json::from_value(value).map(Self).map_err(Into::into)
    }
    pub(super) fn request(&self) -> &RunArgs {
        &self.0
    }
}
impl SurrealValue for RunRequestRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json_into_value(serde_json::to_value(self.0).expect("admitted Media request"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        Self::new(native_json_from_value_strict(value)?)
            .map_err(|_| Error::internal("invalid Media retained request".into()))
    }
}

pub(super) struct GenerationResultRecord {
    original: serde_json::Value,
    generation: MediaGenerationResult,
}
impl GenerationResultRecord {
    pub(super) fn new(original: serde_json::Value) -> Result<Self, TaskError> {
        let envelope: rmcp::model::CallToolResult = serde_json::from_value(original.clone())?;
        veoveo_mcp_contract::task_completion::result_uri(&envelope).map_err(|_| {
            TaskError::InvalidRecord("Media completion address disagrees with its link".into())
        })?;
        let generation = serde_json::from_value(envelope.structured_content.ok_or_else(|| {
            TaskError::InvalidRecord("Media completion lacks its generation product".into())
        })?)?;
        Ok(Self {
            original,
            generation,
        })
    }
    pub(super) fn generation(&self) -> &MediaGenerationResult {
        &self.generation
    }
}
impl SurrealValue for GenerationResultRecord {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json_into_value(self.original)
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        Self::new(native_json_from_value_strict(value)?)
            .map_err(|_| Error::internal("invalid Media retained result: requires veoveo.ai/media-generation/v2; drain writers and upgrade Media and consumers together".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{CallToolResult, ContentBlock, Resource};
    use serde_json::json;

    fn result() -> serde_json::Value {
        let generation = MediaGenerationResult::new(
            veoveo_types::TaskId::new(),
            serde_json::from_value(json!({
                "id":"codec-prediction", "modelId":"owner/model", "status":"completed",
                "outputCount":0, "timings":{"opaque":null}, "error":null
            }))
            .unwrap(),
            vec![],
        )
        .unwrap();
        let mut envelope = CallToolResult::success(vec![
            ContentBlock::text("completed"),
            ContentBlock::ResourceLink(Resource::new(generation.result_uri().as_str(), "result")),
        ]);
        envelope.structured_content = Some(serde_json::to_value(generation).unwrap());
        let mut value = serde_json::to_value(envelope).unwrap();
        value["structuredContent"]["prediction"]["error"] = serde_json::Value::Null;
        value["_meta"] = json!({"external/example":{"integer":u64::MAX,"null":null}});
        value["external_extension"] = json!({"opaque":true});
        value
    }

    #[test]
    fn request_normalizes_run_args_without_changing_open_input() {
        let original = json!({"model":"owner/model","input":{"null":null,"max":u64::MAX}});
        let record = RunRequestRecord::new(original.clone()).unwrap();
        let normalized = serde_json::to_value(record.request()).unwrap();
        assert_eq!(
            native_json_from_value_strict(record.into_value()).unwrap(),
            normalized
        );
        assert_eq!(normalized["input"], original["input"]);
        for invalid in [
            Value::RecordId(surrealdb::types::RecordId::new("task", "native")),
            Value::Datetime(chrono::Utc::now().into()),
            Value::None,
        ] {
            let Value::Object(mut fields) = native_json_into_value(original.clone()) else {
                panic!("request object")
            };
            let Value::Object(input) = fields.get_mut("input").unwrap() else {
                panic!("input object")
            };
            input.insert("provider_extension", invalid);
            assert!(RunRequestRecord::from_value(Value::Object(fields)).is_err());
        }
    }

    #[test]
    fn complete_result_is_lossless_and_strict_at_the_native_boundary() {
        let original = result();
        let baseline = GenerationResultRecord::new(original.clone())
            .unwrap()
            .into_value();
        for meta in [
            Some(original["_meta"].clone()),
            None,
            Some(serde_json::Value::Null),
        ] {
            let mut value = original.clone();
            match meta {
                Some(meta) => value["_meta"] = meta,
                None => {
                    value.as_object_mut().unwrap().remove("_meta");
                }
            }
            let decoded =
                GenerationResultRecord::from_value(native_json_into_value(value.clone())).unwrap();
            assert_eq!(
                native_json_from_value_strict(decoded.into_value()).unwrap(),
                value
            );
        }
        for field in ["_meta", "content"] {
            let mut value = original.clone();
            if field == "content" {
                value[field][0]["text"] = "changed".into();
            } else {
                value[field]["external/example"]["integer"] = 0.into();
            }
            assert_ne!(
                GenerationResultRecord::new(value).unwrap().into_value(),
                baseline
            );
        }
        for invalid in [
            Value::RecordId(surrealdb::types::RecordId::new("task", "native")),
            Value::Datetime(chrono::Utc::now().into()),
            Value::None,
        ] {
            let Value::Object(mut fields) = baseline.clone() else {
                panic!("result object")
            };
            fields.insert("external_extension", invalid);
            assert!(GenerationResultRecord::from_value(Value::Object(fields)).is_err());
        }
        let mut wrong = original;
        wrong["content"][1]["uri"] = "media://generation/other".into();
        assert!(GenerationResultRecord::new(wrong).is_err());
    }

    #[test]
    fn native_retained_result_refuses_old_and_mixed_owner_fields() {
        let current = result();
        let _ =
            GenerationResultRecord::from_value(native_json_into_value(current.clone())).unwrap();
        for (path, field, retired) in [
            ("/structuredContent", "taskId", "task_id"),
            ("/structuredContent", "resultUri", "result_uri"),
            ("/structuredContent/prediction", "modelId", "model_id"),
            (
                "/structuredContent/prediction",
                "outputCount",
                "output_count",
            ),
            (
                "/structuredContent/prediction",
                "executionMs",
                "execution_ms",
            ),
            ("/structuredContent/prediction", "createdAt", "created_at"),
        ] {
            for mode in ["replacement", "mixed", "conflicting"] {
                let mut bad = current.clone();
                let object = bad.pointer_mut(path).unwrap().as_object_mut().unwrap();
                let value = object
                    .get(field)
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                if mode == "replacement" {
                    object.remove(field);
                }
                object.insert(
                    retired.into(),
                    if mode == "conflicting" {
                        json!("retired-conflict")
                    } else {
                        value
                    },
                );
                assert!(
                    GenerationResultRecord::from_value(native_json_into_value(bad)).is_err(),
                    "accepted {path}/{retired}:{mode}"
                );
            }
        }
        let mut old = current;
        old["structuredContent"]["schema"] = "veoveo.ai/media-generation/v1".into();
        assert!(GenerationResultRecord::new(old).is_err());
    }

    #[test]
    fn generation_admission_preserves_existing_tool_error_policy() {
        let mut value = result();
        value["isError"] = true.into();
        // Media's existing settlement policy checks the product and matching link,
        // without treating this MCP flag as an alternative provider settlement.
        assert!(GenerationResultRecord::new(value).is_ok());
        assert!(GenerationResultRecord::new(json!({"content":[],"isError":true})).is_err());
    }
}
