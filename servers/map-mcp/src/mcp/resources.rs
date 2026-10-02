//! Resource read dispatch, separate from tool and protocol wiring.
use super::*;

impl MapMcp {
    pub(super) async fn read_map_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let uri = request.uri.as_str();
            // Well-known surface (contract C18, C19): readable by any identity
            // that can list resources.
            if uri == uris::DOCS_URI {
                require_any_scope(&context, WELL_KNOWN_SCOPES)?;
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc(uri) {
                require_any_scope(&context, WELL_KNOWN_SCOPES)?;
                let doc = SERVER_DOCS
                    .doc(doc_id.as_str())
                    .ok_or_else(|| not_found("server document"))?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT_URI {
                require_any_scope(&context, WELL_KNOWN_SCOPES)?;
                return json_resource(uri, SERVER_DOCS.contract_declaration());
            }
            if uri == uris::WORKSPACE_APP_URI {
                require_any_scope(
                    &context,
                    &[
                        MapScope::Admin,
                        MapScope::DatasetRead,
                        MapScope::FeatureRead,
                    ],
                )?;
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, self.workspace_app.as_str()),
                ]));
            }
            if uri == uris::WORKSPACE_URI {
                let identity = require_any_scope(
                    &context,
                    &[
                        MapScope::Admin,
                        MapScope::DatasetRead,
                        MapScope::FeatureRead,
                    ],
                )?;
                return json_resource(
                    uri,
                    &crate::contract::MapWorkspaceAccess {
                        administration: identity_has_scope(&identity, MapScope::Admin),
                        dataset_read: identity_has_scope(&identity, MapScope::DatasetRead),
                        feature_read: identity_has_scope(&identity, MapScope::FeatureRead),
                        feature_write: identity_has_scope(&identity, MapScope::FeatureWrite),
                        feature_publish: identity_has_scope(&identity, MapScope::FeaturePublish),
                        basemap: self.state.workspace_basemap.clone(),
                    },
                );
            }
            if let Some(result) = self.read_authoring_page(uri, &context).await? {
                return Ok(result);
            }
            if let Some(result) = self.read_owned_page(uri, &context).await? {
                return Ok(result);
            }
            if uri == uris::ACTIVE_RELEASES_URI {
                let identity =
                    require_any_scope(&context, &[MapScope::Admin, MapScope::DatasetRead])?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                let pointers = self
                    .state
                    .catalog
                    .list_active_releases(&scope)
                    .await
                    .map_err(internal)?;
                return json_resource(uri, &pointers);
            }
            if let Some(value) = uris::parse_acquisition(uri) {
                let identity = require_scope(&context, MapScope::Admin)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                let id = value;
                let job = self
                    .state
                    .catalog
                    .acquisition(&scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("acquisition"))?;
                return json_resource(uri, &job);
            }
            if uri.starts_with("map://feature-layer/")
                || uri.starts_with("map://feature-style/")
                || uri.starts_with("map://composition/")
            {
                let identity = require_scope(&context, MapScope::FeatureRead)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                if let Some((composition, revision)) = uris::parse_composition_revision(uri) {
                    let composition_id = composition;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .composition_revision(&identity, &scope, &composition_id, revision)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("map composition revision"))?,
                    );
                }
                if let Some(composition) = uris::parse_composition(uri) {
                    let composition_id = composition;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .composition(&identity, &scope, &composition_id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("map composition"))?,
                    );
                }
                if let Some(request) = uris::parse_features_request(uri).map_err(invalid_params)? {
                    let output = self
                        .state
                        .authoring
                        .query_features(&identity, &scope, request)
                        .await
                        .map_err(invalid_params)?;
                    return json_resource(uri, &output);
                }
                if let Some((layer, feature, revision)) = uris::parse_feature_revision(uri) {
                    let layer_id = layer;
                    let feature_id = feature;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .feature_revision(&identity, &scope, &layer_id, &feature_id, revision)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature revision"))?,
                    );
                }
                if let Some((layer, version)) = uris::parse_feature_schema(uri) {
                    let layer_id = layer;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .schema_revision(&identity, &scope, &layer_id, version)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature schema revision"))?,
                    );
                }
                if let Some((layer, version)) = uris::parse_feature_style(uri) {
                    let layer_id = layer;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .style_revision(&identity, &scope, &layer_id, version)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature style revision"))?,
                    );
                }
                if let Some(style_revision) = uris::parse_feature_style_revision(uri) {
                    let style_revision_id = style_revision;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .style_revision_by_id(&identity, &scope, &style_revision_id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature style revision"))?,
                    );
                }
                if let Some((layer, feature)) = uris::parse_feature(uri) {
                    let layer_id = layer;
                    let feature_id = feature;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .feature(&identity, &scope, &layer_id, &feature_id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature"))?,
                    );
                }
                if let Some((layer, changeset)) = uris::parse_changeset(uri) {
                    let layer_id = layer;
                    let changeset_id = changeset;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .changeset(&identity, &scope, &layer_id, &changeset_id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature changeset"))?,
                    );
                }
                if let Some((layer, publication)) = uris::parse_publication(uri) {
                    let layer_id = layer;
                    let publication_id = publication;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .publication(&identity, &scope, &layer_id, &publication_id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("layer publication"))?,
                    );
                }
                if let Some((layer, publication, product)) = uris::parse_layer_product(uri) {
                    let layer_id = layer;
                    let publication_id = publication;
                    let product_id = product;
                    let product = self
                        .state
                        .authoring
                        .layer_product(&identity, &scope, &layer_id, &publication_id, &product_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("map layer product"))?;
                    return json_resource(uri, &product);
                }
                if let Some(layer) = uris::parse_feature_layer(uri) {
                    let layer_id = layer;
                    return json_resource(
                        uri,
                        &self
                            .state
                            .authoring
                            .layer(&identity, &scope, &layer_id)
                            .await
                            .map_err(internal)?
                            .ok_or_else(|| not_found("feature layer"))?,
                    );
                }
            }
            if let Some(artifact_id) = uris::parse_artifact(uri) {
                require_any_scope(&context, &[MapScope::DatasetRead, MapScope::FeatureRead])?;
                let artifact = self
                    .state
                    .artifacts
                    .get(&internal_caller(&context)?, &artifact_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("artifact"))?;
                let content = ResourceContents::blob(BASE64_STANDARD.encode(&artifact.bytes), uri)
                    .with_mime_type(
                        artifact
                            .metadata
                            .mime_type
                            .unwrap_or_else(|| "application/octet-stream".to_owned()),
                    );
                return Ok(ReadResourceResult::new(vec![content]));
            }
            let identity = require_scope(&context, MapScope::DatasetRead)?;
            let scope = self.state.scope(&identity).await.map_err(internal)?;
            if let Some(result) = self.read_release_page(uri, &scope).await? {
                return Ok(result);
            }
            if let Some(result) = self
                .read_derivation_resource(uri, &identity, &scope)
                .await?
            {
                return Ok(result);
            }
            match uri {
                uris::LOCATIONS_URI => {
                    return json_resource(
                        uri,
                        &self
                            .state
                            .analytics
                            .list_locations(&scope.tenant_key(), 10_000)
                            .map_err(internal)?,
                    );
                }
                uris::FACILITIES_URI => {
                    return json_resource(
                        uri,
                        &self
                            .state
                            .analytics
                            .list_facilities(&scope.tenant_key(), 10_000)
                            .map_err(internal)?,
                    );
                }
                uris::RASTERS_URI => {
                    return json_resource(
                        uri,
                        &self
                            .state
                            .analytics
                            .list_raster_products(&scope.tenant_key(), None, 10_000)
                            .map_err(internal)?,
                    );
                }
                _ => {}
            }
            if let Ok(address) = crate::contract::MapMobilityProfilesUri::parse(uri) {
                let page = self
                    .state
                    .catalog
                    .mobility_profiles_page(&scope, &address)
                    .await
                    .map_err(internal)?;
                return json_resource(uri, &page);
            }
            if let Ok(address) = crate::contract::MapSourcesUri::parse(uri) {
                let page = self
                    .state
                    .catalog
                    .sources_page(&scope, &address)
                    .await
                    .map_err(internal)?;
                return json_resource(uri, &page);
            }
            if let Ok(address) = crate::contract::MapRestrictionsUri::parse(uri) {
                let page = self
                    .state
                    .catalog
                    .restrictions_page(&scope, &address)
                    .await
                    .map_err(internal)?;
                return json_resource(uri, &page);
            }
            if let Ok(address) = crate::contract::MapTravelModelsUri::parse(uri) {
                let page = crate::travel_models::TravelModelReads::new(self.state.catalog.store())
                    .page(&crate::server::tasks::runtime_owner(&identity), &address)
                    .await
                    .map_err(internal)?;
                return json_resource(uri, &page);
            }
            if let Ok(address) = crate::contract::MapSourceUri::parse(uri) {
                let source = self
                    .state
                    .catalog
                    .source(&scope, address.id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("source"))?;
                return json_resource(
                    uri,
                    &crate::contract::SourceSummary::new(&source).map_err(internal)?,
                );
            }
            if let Ok(address) = crate::contract::MapReleaseUri::parse(uri) {
                let release = self
                    .state
                    .catalog
                    .release_in_dataset(&scope, address.dataset_id(), address.release_id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("release"))?;
                return json_resource(uri, &release);
            }
            if let Ok(address) = crate::contract::MapSourceFeatureUri::parse(uri) {
                self.state
                    .catalog
                    .release(&scope, address.release_id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("dataset release"))?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .analytics
                        .source_feature(
                            &scope.tenant_key(),
                            address.release_id(),
                            address.feature_id(),
                        )
                        .map_err(internal)?
                        .ok_or_else(|| not_found("source feature"))?,
                );
            }
            if let Some(value) = uris::parse_location(uri) {
                let id = value;
                return json_resource(
                    uri,
                    &self
                        .state
                        .analytics
                        .location(&scope.tenant_key(), &id)
                        .map_err(internal)?
                        .ok_or_else(|| not_found("location"))?,
                );
            }
            if let Some(value) = uris::parse_facility(uri) {
                let id = value;
                return json_resource(
                    uri,
                    &self
                        .state
                        .analytics
                        .facility(&scope.tenant_key(), &id)
                        .map_err(internal)?
                        .ok_or_else(|| not_found("facility"))?,
                );
            }
            if let Ok(address) = crate::contract::MapRasterUri::parse(uri) {
                return json_resource(
                    uri,
                    &self
                        .state
                        .analytics
                        .raster_product(&scope.tenant_key(), address.id())
                        .map_err(internal)?
                        .ok_or_else(|| not_found("raster product"))?,
                );
            }
            if let Ok(address) = crate::contract::MapMobilityProfileUri::parse(uri) {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .mobility_profile(&scope, address.id(), address.version())
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("mobility profile"))?,
                );
            }
            if let Ok(address) = crate::contract::MapRestrictionUri::parse(uri) {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .restriction(&scope, address.id())
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("restriction"))?,
                );
            }
            if let Ok(address) = crate::contract::MapRouteUri::parse(uri) {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .route(&scope, address.id())
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("route"))?,
                );
            }
            if let Some(value) = uris::parse_matrix(uri) {
                let id = value;
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .matrix(&scope, &id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("matrix"))?,
                );
            }
            if let Ok(address) = crate::contract::MapTravelModelUri::parse(uri) {
                let model = crate::travel_models::TravelModelReads::new(self.state.catalog.store())
                    .get(
                        &crate::server::tasks::runtime_owner(&identity),
                        address.id(),
                    )
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("travel model"))?;
                return json_resource(uri, &model);
            }
            Err(McpError::resource_not_found(
                format!("unknown Map resource `{uri}`"),
                None,
            ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }
}
