//! Fixed-size pages over rows already filtered and limited by SQL.
use crate::contract::CollectionPage;
use anyhow::{Context, Result};
#[cfg(feature = "mcp")]
use rmcp::ErrorData as McpError;

pub(crate) const PAGE_SIZE: usize = 100;

pub(crate) fn page<R, T, C>(
    mut records: Vec<R>,
    position: impl Fn(&R) -> Result<C>,
    convert: impl Fn(R) -> Result<T>,
) -> Result<CollectionPage<T, C>> {
    let has_more = records.len() > PAGE_SIZE;
    records.truncate(PAGE_SIZE);
    let next_cursor = if has_more {
        Some(position(
            records.last().context("missing Time page cursor")?,
        )?)
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
        error.downcast_ref::<crate::persistence::PersistenceError>(),
        Some(crate::persistence::PersistenceError::InvalidTimeField { .. })
    ) {
        McpError::invalid_params("invalid Time collection cursor", None)
    } else {
        McpError::internal_error(error.to_string(), None)
    }
}
