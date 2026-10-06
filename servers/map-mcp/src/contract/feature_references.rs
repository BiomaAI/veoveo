//! Generic references preserve escaped wire text; concrete route meaning belongs to readers.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{Check, Checked, ResourceUri};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
struct ReferenceValue(ResourceUri);
impl Check for ReferenceValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        let text = self.0.as_str();
        let scheme = text.split_once(':').map(|(scheme, _)| scheme);
        if text.len() > 1024
            || !matches!(
                scheme,
                Some(
                    "map"
                        | "artifact"
                        | "recording"
                        | "stream"
                        | "reason"
                        | "time"
                        | "frames"
                        | "view"
                )
            )
        {
            return Err(super::MapRelationshipError);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureResourceReference(Checked<ReferenceValue>);
impl FeatureResourceReference {
    pub fn new(uri: ResourceUri) -> Result<Self, super::MapRelationshipError> {
        Checked::new(ReferenceValue(uri)).map(Self)
    }
    pub fn uri(&self) -> &ResourceUri {
        &self.0.get().0
    }
}
impl JsonSchema for FeatureResourceReference {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FeatureResourceReference".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ResourceUri::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_reference_admits_escaped_network_components_without_normalization() {
        for text in [
            "map://host:8443/path%2Fpart?q=a%26b#fragment",
            "view://scene/a%20b",
            "artifact://host/path?x=%E2%82%AC",
        ] {
            let value = FeatureResourceReference::new(ResourceUri::new(text).unwrap()).unwrap();
            assert_eq!(value.uri().as_str(), text);
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                serde_json::to_string(text).unwrap()
            );
            assert_eq!(
                serde_json::from_value::<FeatureResourceReference>(serde_json::json!(text))
                    .unwrap(),
                value
            );
        }
        for text in [
            "MAP://host/path",
            "map:opaque",
            "map://host/raw space",
            "map://host/é",
            "map://host/%xy",
            "map://host/{id}",
            "https://host/path",
        ] {
            assert!(
                serde_json::from_value::<FeatureResourceReference>(serde_json::json!(text))
                    .is_err(),
                "{text}"
            );
            assert!(
                ResourceUri::new(text)
                    .ok()
                    .and_then(|uri| FeatureResourceReference::new(uri).ok())
                    .is_none()
            );
        }
        let long = format!("map://host/{}", "a".repeat(1024));
        assert!(FeatureResourceReference::new(ResourceUri::new(long).unwrap()).is_err());
    }
}
