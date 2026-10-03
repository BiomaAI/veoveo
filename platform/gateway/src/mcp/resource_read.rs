//! Resource delivery commits a validated source observation before returning content.
use super::http_response::RequestError;
use super::{GatewayMcp, discovery::DiscoveryCacheKey};
use crate::{
    AuthenticatedSubject,
    mcp_support::{
        mcp_internal, mcp_invalid_params, project_gateway_resource_uri_for_upstream,
        project_read_resource_result,
    },
};
use rmcp::{
    ServiceError,
    model::{
        CacheScope, ClientCapabilities, ErrorData as McpError, ReadResourceRequestParams,
        ReadResourceResult, ServerResult,
    },
    service::{Peer, PeerRequestOptions, RequestContext, RoleClient, RoleServer},
};
use veoveo_audit_contract::{
    AuditDetail, AuditOutcome, AuditReadMethod, AuditReason, AuditTarget, KnowledgeReadStatus,
};
use veoveo_mcp_contract::{
    GatewayDiscoverySurface, GatewayResourceProjection, parse_gateway_task_resource_uri,
};
use veoveo_mcp_knowledge_extension::{self as knowledge, Observation, Revision};

impl GatewayMcp {
    async fn finish_resource_read(
        &self,
        subject: &AuthenticatedSubject,
        projection: &GatewayResourceProjection,
        indexing: Option<&super::knowledge_indexing::IndexingReadPermit>,
        result: Result<(ReadResourceResult, Option<Observation>), RequestError>,
    ) -> Result<ReadResourceResult, RequestError> {
        let ordinary = || AuditDetail::Read {
            method: AuditReadMethod::ResourceRead,
        };
        let (detail, outcome, reason) = match &result {
            Ok((_, Some(observation))) => (
                AuditDetail::KnowledgeRead {
                    member: projection.gateway_uri.clone(),
                    observation: Some(Box::new(observation.into())),
                    status: if observation.not_modified() {
                        KnowledgeReadStatus::NotModified
                    } else {
                        KnowledgeReadStatus::Read
                    },
                },
                AuditOutcome::Succeeded,
                AuditReason::Accepted,
            ),
            Ok((_, None)) => (ordinary(), AuditOutcome::Succeeded, AuditReason::Accepted),
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::INVALID_REQUEST => {
                (ordinary(), AuditOutcome::Denied, AuditReason::PolicyDenied)
            }
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::INVALID_PARAMS => (
                ordinary(),
                AuditOutcome::Failed,
                AuditReason::InvalidRequest,
            ),
            Err(error) if error.protocol().code == rmcp::model::ErrorCode::RESOURCE_NOT_FOUND => {
                (ordinary(), AuditOutcome::Failed, AuditReason::NotFound)
            }
            Err(_) => (
                ordinary(),
                AuditOutcome::Failed,
                AuditReason::UpstreamFailure,
            ),
        };
        let draft = subject
            .audit_draft(
                &self.profile_id,
                AuditTarget::Resource {
                    server: projection.server.clone(),
                    uri: projection.gateway_uri.clone(),
                },
                detail,
                outcome,
                reason,
            )
            .map_err(|_| mcp_internal("invalid resource read audit attribution"))?;
        if let Some(permit) = indexing.filter(|_| outcome != AuditOutcome::Denied) {
            let read = veoveo_audit_contract::IndexingRead::new(draft, permit.collection().clone())
                .map_err(|_| mcp_internal("invalid indexing audit attribution"))?;
            self.state
                .record_indexing_audit(read)
                .await
                .map_err(|_| mcp_internal("required indexing audit unavailable"))?;
        } else {
            self.state
                .record_audit(draft)
                .await
                .map_err(|_| mcp_internal("required resource read audit unavailable"))?;
        }
        result.map(|(result, _)| result)
    }
    pub(super) async fn handle_read_resource(
        &self,
        mut request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, RequestError> {
        if let Some(task_id) = parse_gateway_task_resource_uri(&request.uri) {
            return self
                .read_task_status_resource(task_id, &request.uri, &context)
                .await;
        }
        let server = self.server_for_resource(&request.uri)?;
        let projection = self.project_resource_for_upstream(&request.uri)?;
        let subject = self.admit_resource_read(&context, &projection).await?;
        let mut indexing = None;
        let result = async {
            indexing = self
                .admit_indexing_read(&subject, &projection, &context.meta)
                .await?;
            let declaring_caller = knowledge::server::requested(Some(&context.meta))
                .map_err(|_| mcp_invalid_params("invalid knowledge capability"))?;
            let condition = knowledge::server::condition(Some(&context.meta))
                .map_err(|_| mcp_invalid_params("invalid knowledge read condition"))?;
            // The canonical request context owns metadata. Caller-supplied provenance
            // is never forwarded to a source or accepted as an observation.
            request.meta = Some(context.meta.clone());
            request
                .meta
                .as_mut()
                .expect("assigned metadata")
                .remove(knowledge::OBSERVATION_KEY);
            let snapshot = self.catalog.snapshot();
            let catalog = snapshot.catalog();
            let manifest = catalog
                .server(&server)
                .ok_or_else(|| mcp_internal(format!("unknown resource server `{server}`")))?;
            let key = DiscoveryCacheKey {
                catalog_generation: snapshot.generation(),
                principal: subject.actor.id.clone(),
                authorization_fingerprint: super::discovery_authorization_fingerprint(&subject)?,
                server: server.clone(),
            };
            self.ensure_discovery_watch(&key, context.peer.clone(), &subject)
                .await?;
            let routes = match self.discovery.resource_routes(&key).await {
                Some(routes) => routes,
                None => {
                    if let Some(fetch) = self
                        .discovery
                        .begin(GatewayDiscoverySurface::Resources, key.clone())
                        .await
                    {
                        let profile_servers = self.profile_servers().into_iter().collect();
                        match self
                            .discover_resources_for_server(
                                catalog,
                                &profile_servers,
                                &server,
                                &context,
                                &subject,
                            )
                            .await
                        {
                            Ok(routes) => {
                                self.discovery
                                    .finish_resource_routes(
                                        fetch.with_denied(routes.denied),
                                        routes.items,
                                    )
                                    .await
                            }
                            Err(error) => {
                                self.discovery
                                    .finish_failure(GatewayDiscoverySurface::Resources, fetch)
                                    .await;
                                return Err(error.into());
                            }
                        }
                    } else {
                        self.discovery
                            .settle(
                                GatewayDiscoverySurface::Resources,
                                std::slice::from_ref(&key),
                            )
                            .await;
                    }
                    // Read back through the cache's generation and notification fences.
                    self.discovery.resource_routes(&key).await.ok_or_else(|| {
                        mcp_internal(format!(
                            "resource discovery for `{server}` is unavailable; retry the read"
                        ))
                    })?
                }
            };
            let upstream_uri = routes
                .iter()
                .find(|route| route.resource.uri == request.uri)
                .map(|route| route.upstream_uri.clone());
            let Some(upstream_uri) = upstream_uri.or(project_gateway_resource_uri_for_upstream(
                manifest,
                &request.uri,
                &[],
            )?) else {
                return Err(mcp_invalid_params(format!(
                    "resource URI is not exposed: {}",
                    request.uri
                ))
                .into());
            };
            let projection = GatewayResourceProjection {
                server,
                gateway_uri: projection.gateway_uri.clone(),
                upstream_uri,
            };
            request.uri = projection.upstream_uri.to_string();
            let (mut result, observation) = self
                .idempotent_upstream_request(
                    &projection.server,
                    context.peer.clone(),
                    &subject,
                    |upstream| {
                        let request = request.clone();
                        let revision = condition.as_ref().map(|c| c.if_none_match.clone());
                        let projection = projection.clone();
                        async move {
                            read_upstream(upstream, request, &projection, revision.as_ref()).await
                        }
                    },
                )
                .await?;
            if let Some(permit) = &indexing {
                self.validate_indexing_delivery(
                    &subject,
                    &projection,
                    permit,
                    observation.as_ref(),
                )
                .await?;
            }
            prepare_delivery(&mut result, &projection, declaring_caller)?;
            Ok((result, observation))
        }
        .await;
        self.finish_resource_read(&subject, &projection, indexing.as_ref(), result)
            .await
    }
}

fn prepare_delivery(
    result: &mut ReadResourceResult,
    projection: &GatewayResourceProjection,
    declaring_caller: bool,
) -> Result<(), McpError> {
    project_read_resource_result(result, projection)?;
    // SDK read caches key by URI only. Every delivered read must recheck
    // authority and commit its own observation before it reaches a caller.
    result.ttl_ms = Some(0);
    result.cache_scope = Some(CacheScope::Private);
    if !declaring_caller && let Some(meta) = result.meta.as_mut() {
        meta.remove(knowledge::OBSERVATION_KEY);
    }
    Ok(())
}

async fn read_upstream(
    upstream: Peer<RoleClient>,
    request: ReadResourceRequestParams,
    projection: &GatewayResourceProjection,
    revision: Option<&Revision>,
) -> Result<(ReadResourceResult, Option<Observation>), ServiceError> {
    let declared = upstream
        .peer_info()
        .is_some_and(|info| knowledge::client::supports(&info.capabilities));
    let result = if declared {
        let (request, options) = knowledge::client::read_request(
            request,
            ClientCapabilities::default(),
            revision,
            PeerRequestOptions::default(),
        );
        match upstream
            .send_request_with_option(request, options)
            .await?
            .await_response()
            .await?
        {
            ServerResult::ReadResourceResult(result) => result,
            _ => return Err(ServiceError::UnexpectedResponse),
        }
    } else {
        upstream.read_resource(request).await?
    };
    let observation = validate_source_read(&result, projection, revision, declared)
        .map_err(ServiceError::McpError)?;
    Ok((result, observation))
}

fn validate_source_read(
    result: &ReadResourceResult,
    projection: &GatewayResourceProjection,
    revision: Option<&Revision>,
    declared: bool,
) -> Result<Option<Observation>, McpError> {
    let observation = knowledge::client::validate_read(result, &projection.upstream_uri, revision)
        .map_err(|_| mcp_internal("invalid upstream knowledge observation"))?;
    if observation
        .as_ref()
        .is_some_and(|o| !declared || o.collection().server() != &projection.server)
    {
        return Err(mcp_internal("upstream knowledge collection owner mismatch"));
    }
    Ok(observation)
}

#[cfg(test)]
mod tests;
