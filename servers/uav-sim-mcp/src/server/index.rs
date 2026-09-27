//! Fixed-size live collection pages. Cursors carry positions, never authority.
use anyhow::{Context, Result};
use rmcp::ErrorData as McpError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::contract::CollectionPage;

pub(super) const PAGE_SIZE: usize = 100;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor<T> {
    version: u8,
    collection: String,
    position: T,
}

pub(super) fn decode<T: DeserializeOwned>(
    root: &str,
    encoded: Option<&str>,
) -> Result<Option<T>, McpError> {
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    let invalid = || McpError::invalid_params("invalid UAV collection cursor", None);
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

pub(super) fn parse<T: DeserializeOwned>(
    uri: &str,
    root: &str,
) -> Result<Option<Option<T>>, McpError> {
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
        .ok_or_else(|| McpError::invalid_params("expected one UAV collection cursor", None))?;
    decode(root, Some(encoded)).map(Some)
}

pub(super) fn encode<P: Serialize>(root: &str, position: P) -> Result<String> {
    Ok(hex::encode(serde_json::to_vec(&Cursor {
        version: 1,
        collection: root.into(),
        position,
    })?))
}

pub(super) fn page<R, T, P: Serialize>(
    mut rows: Vec<R>,
    root: &str,
    position: impl Fn(&R) -> P,
    convert: impl Fn(R) -> Result<T>,
) -> Result<CollectionPage<T>> {
    let more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let next_cursor = if more {
        Some(hex::encode(serde_json::to_vec(&Cursor {
            version: 1,
            collection: root.into(),
            position: position(rows.last().context("missing UAV page cursor")?),
        })?))
    } else {
        None
    };
    Ok(CollectionPage {
        items: rows.into_iter().map(convert).collect::<Result<_>>()?,
        limit: PAGE_SIZE,
        next_cursor,
    })
}

pub(super) fn completion(mut values: Vec<String>) -> Result<rmcp::model::CompleteResult, McpError> {
    let more = values.len() > PAGE_SIZE;
    values.truncate(PAGE_SIZE);
    let total = (!more).then_some(values.len() as u32);
    Ok(rmcp::model::CompleteResult::new(
        rmcp::model::CompletionInfo::with_pagination(values, total, more)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?,
    ))
}

pub(super) fn validate_needle(needle: &str) -> Result<(), McpError> {
    if needle.len() > 512 || needle.chars().any(char::is_control) {
        return Err(McpError::invalid_params(
            "completion text must be at most 512 bytes without control characters",
            None,
        ));
    }
    Ok(())
}
