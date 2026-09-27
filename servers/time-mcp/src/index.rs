//! Opaque collection cursors and fixed-size resource pages.
use crate::contract::CollectionPage;
use anyhow::{Context, Result};
#[cfg(feature = "mcp")]
use rmcp::ErrorData as McpError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(crate) const PAGE_SIZE: usize = 100;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CursorError {
    #[error("invalid Time collection cursor")]
    Invalid,
    #[cfg(any(feature = "mcp", test))]
    #[error("expected one Time collection cursor")]
    ExpectedOne,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor<T> {
    version: u8,
    collection: String,
    position: T,
}

pub(crate) fn decode<T: DeserializeOwned>(
    root: &str,
    encoded: Option<&str>,
) -> Result<Option<T>, CursorError> {
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    let invalid = || CursorError::Invalid;
    if encoded.is_empty() || encoded.len() > 2048 {
        return Err(invalid());
    }
    let bytes = hex::decode(encoded).map_err(|_| invalid())?;
    let cursor: Cursor<T> = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if cursor.version != 1 || cursor.collection != root {
        return Err(invalid());
    }
    Ok(Some(cursor.position))
}

#[cfg(any(feature = "mcp", test))]
pub(crate) fn parse<T: DeserializeOwned>(
    uri: &str,
    root: &str,
) -> Result<Option<Option<T>>, CursorError> {
    if uri == root {
        return Ok(Some(None));
    }
    let Some(query) = uri
        .strip_prefix(root)
        .and_then(|suffix| suffix.strip_prefix('?'))
    else {
        return Ok(None);
    };
    let encoded = query
        .strip_prefix("cursor=")
        .filter(|value| !value.contains(['&', '=', '?', '#']))
        .ok_or(CursorError::ExpectedOne)?;
    decode(root, Some(encoded)).map(Some)
}

pub(crate) fn page<R, T, P: Serialize>(
    mut records: Vec<R>,
    root: &str,
    position: impl Fn(&R) -> P,
    convert: impl Fn(R) -> Result<T>,
) -> Result<CollectionPage<T>> {
    let has_more = records.len() > PAGE_SIZE;
    records.truncate(PAGE_SIZE);
    let next_cursor = if has_more {
        let last = records.last().context("missing Time page cursor")?;
        Some(hex::encode(serde_json::to_vec(&Cursor {
            version: 1,
            collection: root.to_owned(),
            position: position(last),
        })?))
    } else {
        None
    };
    Ok(CollectionPage {
        items: records.into_iter().map(convert).collect::<Result<_>>()?,
        limit: PAGE_SIZE,
        next_cursor,
    })
}

#[cfg(feature = "mcp")]
pub(crate) fn query_error(error: anyhow::Error) -> McpError {
    if matches!(
        error.downcast_ref::<veoveo_platform_store::StoreError>(),
        Some(veoveo_platform_store::StoreError::InvalidTimeField { .. })
    ) {
        McpError::invalid_params("invalid Time collection cursor", None)
    } else {
        McpError::internal_error(error.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_platform_store::TimeVersionCursor;
    #[test]
    fn cursors_reject_wrong_collection_version_shape_and_repeated_parameters() {
        let root = "time://calendars";
        let page = page(
            (0..101).collect(),
            root,
            |n| TimeVersionCursor {
                key: format!("calendar-{n}"),
                version: 1,
            },
            Ok,
        )
        .unwrap();
        assert_eq!(page.items.len(), 100);
        let encoded = page.next_cursor.unwrap();
        let cursor = decode::<TimeVersionCursor>(root, Some(&encoded))
            .unwrap()
            .unwrap();
        assert_eq!(cursor.key, "calendar-99");
        assert!(decode::<TimeVersionCursor>("time://epochs", Some(&encoded)).is_err());
        for suffix in [
            "?",
            "?cursor=",
            "?cursor=broken",
            "?offset=10",
            "?cursor=00&cursor=00",
        ] {
            assert!(parse::<TimeVersionCursor>(&format!("{root}{suffix}"), root).is_err());
        }
        for json in [
            serde_json::json!({"version":2,"collection":root,"position":cursor}),
            serde_json::json!({"version":1,"collection":root,"position":{"key":"calendar-1","version":1,"extra":true}}),
        ] {
            assert!(
                decode::<TimeVersionCursor>(
                    root,
                    Some(&hex::encode(serde_json::to_vec(&json).unwrap()))
                )
                .is_err()
            );
        }
    }
}
