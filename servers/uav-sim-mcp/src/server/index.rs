//! Fixed-size live collection pages. Cursors carry positions, never authority.
use anyhow::{Context, Result};
use rmcp::ErrorData as McpError;

use crate::contract::CollectionPage;

pub(super) const PAGE_SIZE: usize = 100;

pub(super) fn page<R, T>(
    mut rows: Vec<R>,
    cursor: impl Fn(&R) -> Result<String>,
    convert: impl Fn(R) -> Result<T>,
) -> Result<CollectionPage<T>> {
    let more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let next_cursor = if more {
        Some(cursor(rows.last().context("missing UAV page cursor")?)?)
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
