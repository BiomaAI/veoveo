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

/// Preserve the inner JSON text until installation target codecs can admit it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuditParameters {
    pub query: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuditViewParameters {
    pub query: String,
    pub view: AuditRecordId,
}
pub(crate) fn decode_query(
    input: &str,
    registry: &AuditTargetRegistry,
) -> Result<AuditQuery, String> {
    if input.len() > 16_384 {
        return Err("audit query exceeds 16 KiB".into());
    }
    registry
        .decoder()
        .from_str(input)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Query, http::Uri};
    use veoveo_computers_contract::{ComputerAuditTarget, ComputerId};

    fn request_query(input: &str) -> String {
        let mut url =
            url::Url::parse("http://localhost/admin/fixture/console/audit/records").unwrap();
        url.query_pairs_mut().append_pair("query", input);
        let uri: Uri = url.as_str().parse().unwrap();
        Query::<AuditParameters>::try_from_uri(&uri)
            .unwrap()
            .0
            .query
    }

    #[test]
    fn audit_request_queries_admit_registered_owner_targets_and_reject_invalid_payloads() {
        let registry = veoveo_gateway_catalog::audit_target_registry().unwrap();
        let registration = registry.registration::<ComputerAuditTarget>().unwrap();
        let computer = ComputerId::new();
        let target = registry.target(ComputerAuditTarget { computer }).unwrap();
        let mut query = AuditQuery::new(AuditPartition::Installation);
        query.target = Some(target.clone());
        let input = serde_json::to_string(&query).unwrap();
        let extracted = request_query(&input);
        assert_eq!(extracted, input);
        let admitted = decode_query(&extracted, &registry).unwrap();
        assert_eq!(admitted.partition, AuditPartition::Installation);
        assert_eq!(admitted.target.as_ref(), Some(&target));
        assert_eq!(
            registration
                .get(&registry, admitted.target.as_ref().unwrap())
                .unwrap()
                .computer,
            computer
        );
        assert!(decode_query(&extracted, &AuditTargetRegistry::empty()).is_err());

        let target_json = serde_json::to_string(&target).unwrap();
        for invalid in [
            String::from(r#"{"kind":"computer","computer":"invalid"}"#),
            String::from(r#"{"kind":"unregistered_owner"}"#),
            format!(r#"{{"kind":"computer","computer":"{computer}","computer":"{computer}"}}"#),
            format!(r#"{{"kind":"computer","computer":"{computer}","extra":true}}"#),
        ] {
            let input = input.replace(&target_json, &invalid);
            assert!(
                decode_query(&request_query(&input), &registry).is_err(),
                "request accepted {invalid}"
            );
        }
        assert!(decode_query(&request_query(r#"{"target": "#), &registry).is_err());
        let at_limit = format!("{}{}", " ".repeat(16_384 - input.len()), input);
        assert_eq!(at_limit.len(), 16_384);
        let admitted = decode_query(&request_query(&at_limit), &registry).unwrap();
        assert_eq!(admitted.target.as_ref(), Some(&target));
        let over_limit = format!(" {at_limit}");
        assert_eq!(over_limit.len(), 16_385);
        assert_eq!(
            decode_query(&request_query(&over_limit), &registry).unwrap_err(),
            "audit query exceeds 16 KiB"
        );
    }
}
