use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use veoveo_mcp_contract::audit::*;

/// A typed JSON value encoded in one URL query component. The parser never accepts
/// open metadata; the owning audit contract validates every nested field.
pub(crate) struct Encoded<T>(pub T);
impl<'de, T: DeserializeOwned> Deserialize<'de> for Encoded<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() > 16_384 {
            return Err(serde::de::Error::custom("audit query exceeds 16 KiB"));
        }
        serde_json::from_str(&value)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Parameters<T> {
    #[serde(bound(deserialize = "T: DeserializeOwned"))]
    pub query: Encoded<T>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ViewParameters<T> {
    #[serde(bound(deserialize = "T: DeserializeOwned"))]
    pub query: Encoded<T>,
    pub view: AuditRecordId,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StreamParameters {
    pub partition: Encoded<AuditPartition>,
    pub view: AuditRecordId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StreamCursor {
    pub partition: AuditPartition,
    pub sequence: Option<AuditBlockSequence>,
}
