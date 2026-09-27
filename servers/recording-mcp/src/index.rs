//! Versioned cursors for the Recording catalog. Each request rechecks visibility.
use rmcp::ErrorData as McpError;
use serde::{Deserialize, Serialize};
use veoveo_platform_store::RecordingCursor;

use crate::uris::CATALOG_URI;

pub const PAGE_SIZE: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    collection: String,
    position: RecordingCursor,
}

pub fn parse_catalog_uri(uri: &str) -> Result<Option<Option<RecordingCursor>>, McpError> {
    if uri == CATALOG_URI {
        return Ok(Some(None));
    }
    let Some(query) = uri
        .strip_prefix(CATALOG_URI)
        .and_then(|suffix| suffix.strip_prefix('?'))
    else {
        return Ok(None);
    };
    let invalid = || McpError::invalid_params("invalid Recording catalog cursor", None);
    let encoded = query.strip_prefix("cursor=").ok_or_else(invalid)?;
    if encoded.is_empty() || encoded.len() > 2048 {
        return Err(invalid());
    }
    let bytes = hex::decode(encoded).map_err(|_| invalid())?;
    let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.version != 1 || cursor.collection != CATALOG_URI {
        return Err(invalid());
    }
    cursor.position.validate().map_err(|_| invalid())?;
    Ok(Some(Some(cursor.position)))
}

pub(crate) fn encode(position: RecordingCursor) -> anyhow::Result<String> {
    Ok(hex::encode(serde_json::to_vec(&Cursor {
        version: 1,
        collection: CATALOG_URI.to_owned(),
        position,
    })?))
}

pub fn query_error(error: anyhow::Error) -> McpError {
    if let Some(veoveo_platform_store::StoreError::InvalidRecordingField { field, reason }) =
        error.downcast_ref::<veoveo_platform_store::StoreError>()
    {
        McpError::invalid_params(format!("invalid Recording {field}: {reason}"), None)
    } else {
        McpError::internal_error(error.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_platform_store::RecordingId;

    #[test]
    fn cursor_rejects_wrong_version_collection_shape_and_query_parameters() {
        let position = RecordingCursor {
            started_at: chrono::Utc::now(),
            recording_id: RecordingId::new(),
        };
        let encoded = encode(position.clone()).unwrap();
        let decoded = parse_catalog_uri(&format!("{CATALOG_URI}?cursor={encoded}"))
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(decoded.started_at, position.started_at);
        assert_eq!(decoded.recording_id, position.recording_id);
        for suffix in [
            "?",
            "?cursor=",
            "?cursor=broken",
            "?offset=1",
            "?cursor=00&cursor=00",
        ] {
            assert!(parse_catalog_uri(&format!("{CATALOG_URI}{suffix}")).is_err());
        }
        for json in [
            serde_json::json!({"version":2,"collection":CATALOG_URI,"position":position}),
            serde_json::json!({"version":1,"collection":"time://events","position":position}),
            serde_json::json!({"version":1,"collection":CATALOG_URI,"position":{"started_at":position.started_at,"recording_id":position.recording_id,"extra":true}}),
            serde_json::json!({"version":1,"collection":CATALOG_URI,"position":{"started_at":position.started_at,"recording_id":"00000000-0000-4000-8000-000000000000"}}),
        ] {
            let encoded = hex::encode(serde_json::to_vec(&json).unwrap());
            assert!(parse_catalog_uri(&format!("{CATALOG_URI}?cursor={encoded}")).is_err());
        }
    }
}
