//! SQL-scoped indexes for owner-visible routes, matrices, and acquisition jobs.
use super::*;
use crate::catalog::owned::Collection;

impl MapMcp {
    pub(super) async fn read_owned_page(
        &self,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<ReadResourceResult>, McpError> {
        let (root, query) = uri
            .split_once('?')
            .map_or((uri, None), |(root, query)| (root, Some(query)));
        let collection = match root {
            uris::ROUTES_URI => Collection::Routes,
            uris::MATRICES_URI => Collection::Matrices,
            uris::ACQUISITIONS_URI => Collection::Acquisitions,
            _ => return Ok(None),
        };
        let identity = require_scope(
            context,
            if collection == Collection::Acquisitions {
                MapScope::Admin
            } else {
                MapScope::DatasetRead
            },
        )?;
        let cursor = query
            .map(|query| {
                query
                    .strip_prefix("cursor=")
                    .ok_or_else(|| invalid_params("expected one Map catalog cursor"))
            })
            .transpose()?;
        let after = collection.parse_cursor(cursor).map_err(invalid_params)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let result = match collection {
            Collection::Routes => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .routes_page(&scope, after.as_deref())
                    .await
                    .map_err(internal)?,
            )?,
            Collection::Matrices => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .matrices_page(&scope, after.as_deref())
                    .await
                    .map_err(internal)?,
            )?,
            Collection::Acquisitions => {
                if after.is_none() {
                    self.state
                        .acquisitions
                        .reconcile_interrupted(&scope)
                        .await
                        .map_err(internal)?;
                }
                json_read(
                    uri,
                    &self
                        .state
                        .catalog
                        .acquisitions_page(&scope, after.as_deref())
                        .await
                        .map_err(internal)?,
                )?
            }
        };
        Ok(Some(result))
    }
}
