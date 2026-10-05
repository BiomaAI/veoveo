//! JSON-profile driver adapters for known Frames envelopes.
use crate::contract::{
    CoordinateOperationKind, CoordinateOperationProvenance, CoordinateOperationRef, FrameWorldTree,
    ValidatedWorldTree,
};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_map_mcp::contract::CrsId;
use veoveo_platform_store::{native_json_from_value, native_json_into_value};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct WorldTree(pub(super) FrameWorldTree);

impl SurrealValue for WorldTree {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json_into_value(serde_json::to_value(self.0).expect("Frames tree serializes"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        let tree: FrameWorldTree = serde_json::from_value(native_json_from_value(value)?)
            .map_err(|e| Error::internal(format!("invalid stored Frames tree: {e}")))?;
        ValidatedWorldTree::new(tree.clone())
            .map_err(|e| Error::internal(format!("invalid stored Frames tree: {e}")))?;
        Ok(Self(tree))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "CoordinateOperationProvenance", deny_unknown_fields)]
struct ProvenanceWire {
    #[serde(deserialize_with = "stored_operation")]
    operation: CoordinateOperationRef,
    kind: CoordinateOperationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    engine: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    grid_packages: Vec<String>,
    #[serde(default)]
    approximation_used: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accuracy_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

fn stored_operation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<CoordinateOperationRef, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Operation {
        operation_id: crate::contract::CoordinateOperationId,
        operation_uri: crate::contract::FrameOperationUri,
        #[serde(default)]
        source_frame: Option<crate::contract::CoordinateSpace>,
        #[serde(default)]
        target_frame: Option<crate::contract::CoordinateSpace>,
        created_at: chrono::DateTime<chrono::Utc>,
    }
    let wire = Operation::deserialize(deserializer)?;
    if wire.operation_uri.operation_id() != &wire.operation_id {
        return Err(serde::de::Error::custom(
            "stored Frames operation address and identity disagree",
        ));
    }
    Ok(
        CoordinateOperationRef::new(wire.operation_id, wire.created_at)
            .with_frames(wire.source_frame, wire.target_frame),
    )
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct Provenance(
    #[serde(with = "ProvenanceWire")] pub(super) CoordinateOperationProvenance,
);

impl SurrealValue for Provenance {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn is_value(value: &Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> Value {
        native_json_into_value(serde_json::to_value(self).expect("Frames provenance serializes"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(native_json_from_value(value)?)
            .map_err(|e| Error::internal(format!("invalid stored Frames provenance: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_driver_preserves_admitted_tree_wire_and_frame_order() {
        let tree = super::super::read_tests::tree();
        let native = WorldTree(tree.clone()).into_value();
        let decoded = WorldTree::from_value(native.clone()).unwrap();
        assert_eq!(decoded.0, tree);
        assert_eq!(decoded.into_value(), native);
    }

    #[test]
    fn tree_driver_rejects_unknown_fields_invalid_graphs_and_native_values() {
        for value in [
            serde_json::json!({"frames": [], "extra": true}),
            serde_json::json!({"frames": []}),
        ] {
            assert!(WorldTree::from_value(native_json_into_value(value)).is_err());
        }
        assert!(
            WorldTree::from_value(Value::RecordId(surrealdb::types::RecordId::new(
                "frame", "root"
            )))
            .is_err()
        );
    }
    #[test]
    fn provenance_driver_preserves_wire_and_rejects_unknown_fields() {
        let value = serde_json::json!({"operation": {"operation_id":"op-test", "operation_uri":"frames://operation/op-test", "created_at":"2026-01-01T00:00:00Z"}, "kind":"frame_conversion", "approximation_used":false});
        let native = native_json_into_value(value.clone());
        let admitted = Provenance::from_value(native.clone()).unwrap();
        assert_eq!(admitted.into_value(), native);
        let mut nested = value.clone();
        nested["operation"]["extra"] = true.into();
        assert!(Provenance::from_value(native_json_into_value(nested)).is_err());
        let mut extra = value;
        extra["extra"] = true.into();
        assert!(Provenance::from_value(native_json_into_value(extra)).is_err());
    }
}
