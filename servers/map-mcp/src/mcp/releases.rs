//! Release indexes are pages selected in SQL, including dataset URI parents.
use super::*;
use crate::catalog::{MapAccessContext, releases::parse_cursor};

impl MapMcp {
    pub(super) async fn read_release_page(
        &self,
        uri: &str,
        scope: &MapAccessContext,
    ) -> Result<Option<ReadResourceResult>, McpError> {
        let (root, query) = uri
            .split_once('?')
            .map_or((uri, None), |(root, query)| (root, Some(query)));
        let dataset = if root == uris::DATASETS_URI {
            None
        } else if let Some(key) = uris::parse_dataset(root) {
            Some(key)
        } else {
            return Ok(None);
        };
        let cursor = query
            .map(|value| {
                value
                    .strip_prefix("cursor=")
                    .ok_or_else(|| invalid_params("expected one release cursor"))
            })
            .transpose()?;
        let after = parse_cursor(dataset.as_ref(), cursor).map_err(invalid_params)?;
        let page = self
            .state
            .catalog
            .releases_page(scope, dataset.as_ref(), after.as_ref())
            .await
            .map_err(internal)?;
        if dataset.is_some() && after.is_none() && page.items.is_empty() {
            return Err(not_found("dataset"));
        }
        json_read(uri, &page).map(Some)
    }
}
