//! Authoring collection URIs share typed pages and current SQL visibility.
use super::*;
use crate::contract::{MapMetadataError, MapMetadataRequest};

impl MapMcp {
    pub(super) async fn read_authoring_page(
        &self,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<ReadResourceResult>, McpError> {
        let request = match MapMetadataRequest::parse(uri) {
            Ok(request) => request,
            Err(MapMetadataError::UnknownResource) => return Ok(None),
            Err(error) => return Err(invalid_params(error)),
        };
        let identity = require_scope(context, MapScope::FeatureRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let page = self
            .state
            .authoring
            .metadata_page(&identity, &scope, request)
            .await
            .map_err(internal)?;
        json_read(uri, &page).map(Some)
    }
}
