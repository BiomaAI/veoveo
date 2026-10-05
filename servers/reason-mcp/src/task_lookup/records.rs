//! Complete admitted MCP success receipts retain their original JSON spelling.
use crate::contract::AnalyzeRecordingOutput;
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_platform_store::{native_json_from_value_strict, native_json_into_value};
use veoveo_task_runtime::TaskError;

pub(crate) struct AnalysisResultRecord {
    original: serde_json::Value,
    output: AnalyzeRecordingOutput,
}
impl AnalysisResultRecord {
    pub(crate) fn new(original: serde_json::Value) -> Result<Self, TaskError> {
        let envelope: rmcp::model::CallToolResult = serde_json::from_value(original.clone())?;
        let output = crate::task_product::validate(&envelope)
            .map_err(|e| TaskError::InvalidRecord(e.to_string()))?
            .ok_or_else(|| TaskError::InvalidRecord("Reason product is a tool error".into()))?;
        Ok(Self { original, output })
    }
    pub(crate) fn output(&self) -> &AnalyzeRecordingOutput {
        &self.output
    }
    pub(crate) fn into_output(self) -> AnalyzeRecordingOutput {
        self.output
    }
}
impl SurrealValue for AnalysisResultRecord {
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
            .map_err(|_| Error::internal("invalid Reason retained result".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_error_admission_remains_a_no_product_settlement() {
        let original = json!({"content":[], "isError":true,
            "structuredContent":{"opaque":"not an analysis"}});
        let envelope = serde_json::from_value(original.clone()).unwrap();
        assert!(crate::task_product::validate(&envelope).unwrap().is_none());
        // Contributions branch before constructing a retained product receipt.
        assert!(AnalysisResultRecord::new(original).is_err());
    }

    #[test]
    fn retained_success_preserves_extensions_nulls_and_complete_result_agreement() {
        let output: AnalyzeRecordingOutput =
            serde_json::from_str(include_str!("../../testdata/analysis-output-v1.json")).unwrap();
        let envelope = crate::task_product::analysis_tool_result(output.clone()).unwrap();
        let mut original = serde_json::to_value(envelope).unwrap();
        original["_meta"]["external/example"] = json!({"max":u64::MAX,"null":null});
        original["external_extension"] = json!({"opaque":true});
        let baseline = AnalysisResultRecord::new(original.clone())
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
                AnalysisResultRecord::from_value(native_json_into_value(value.clone())).unwrap();
            assert_eq!(
                serde_json::to_value(decoded.output()).unwrap(),
                serde_json::to_value(&output).unwrap()
            );
            assert_eq!(
                native_json_from_value_strict(decoded.into_value()).unwrap(),
                value
            );
        }
        let mut changed_meta = original.clone();
        changed_meta["_meta"]["external/example"]["max"] = 0.into();
        assert_ne!(
            AnalysisResultRecord::new(changed_meta)
                .unwrap()
                .into_value(),
            baseline
        );
        let mut changed_content = original;
        changed_content["content"][0]["text"] = "changed".into();
        assert!(AnalysisResultRecord::new(changed_content).is_err());
        for invalid in [
            Value::RecordId(surrealdb::types::RecordId::new("task", "native")),
            Value::Datetime(chrono::Utc::now().into()),
            Value::None,
        ] {
            let Value::Object(mut fields) = baseline.clone() else {
                panic!("result object")
            };
            fields.insert("external_extension", invalid);
            assert!(AnalysisResultRecord::from_value(Value::Object(fields)).is_err());
        }
    }
}
