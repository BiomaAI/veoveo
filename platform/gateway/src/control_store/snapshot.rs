//! Gateway-owned whole control-plane JSON at the native write boundary.
use surrealdb::types::{Error, Kind, SurrealValue, Value};
use veoveo_mcp_contract::GatewayControlPlane;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ControlPlaneSnapshot(pub GatewayControlPlane);

impl SurrealValue for ControlPlaneSnapshot {
    fn kind_of() -> Kind {
        Kind::Object
    }

    fn into_value(self) -> Value {
        veoveo_platform_store::native_json_into_value(
            serde_json::to_value(self.0).expect("typed gateway control-plane snapshot"),
        )
    }

    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(veoveo_platform_store::native_json_from_value_strict(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid gateway control-plane snapshot".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_preserves_whole_json_and_rejects_native_extension_values() {
        let mut plane: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../../configs/gateway.smoke.json")).unwrap();
        plane.extensions.insert(
            "vendor/snapshot".into(),
            serde_json::json!({"opaque": [null, u64::MAX, {"flag": true}]}),
        );
        let expected = serde_json::to_value(&plane).unwrap();
        let value = ControlPlaneSnapshot(plane.clone()).into_value();
        assert_eq!(
            veoveo_platform_store::native_json_from_value_strict(value.clone()).unwrap(),
            expected
        );
        assert_eq!(
            ControlPlaneSnapshot::from_value(value.clone()).unwrap().0,
            plane
        );
        for invalid in [
            Value::None,
            surrealdb::types::RecordId::new("gateway_control_revision", "foreign").into_value(),
            chrono::Utc::now().into_value(),
        ] {
            let Value::Object(mut object) = value.clone() else {
                panic!("snapshot is a native object");
            };
            object.insert("vendor/snapshot", invalid);
            assert!(ControlPlaneSnapshot::from_value(Value::Object(object)).is_err());
        }
    }
}
