use super::*;
use crate::contract::{
    CatalogEntity, CollectionCatalogEntry, KnowledgeResource, SourceCatalogEntry, SourceCatalogPage,
};
use veoveo_platform_store::knowledge::CatalogSelection;
use veoveo_types::{ResourceAddress, ResourceUri, ServerSlug};

impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    pub(super) async fn read(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let uri = ResourceUri::new(&request.uri)
            .map_err(|_| ErrorData::invalid_params("invalid Knowledge URI", None))?;
        let address = KnowledgeResource::parse(&uri)
            .map_err(|_| ErrorData::invalid_params("unknown Knowledge resource", None))?;
        let target = PolicyTarget::Resource {
            server: "knowledge".parse().unwrap(),
            uri: uri.clone(),
        };
        let (identity, admitted) = self
            .authority(
                &context,
                KnowledgeScope::Read,
                GatewayAction::ResourcesRead,
                &target,
            )
            .await?;
        if let Some(result) = SETUP.read_documents(&request, &context)? {
            return Ok(result);
        }
        let snapshot = self.catalog(&identity, &admitted, &uri, address).await?;
        let current = authorize(
            &self.store,
            &identity,
            KnowledgeScope::Read,
            GatewayAction::ResourcesRead,
            &target,
        )
        .await
        .map_err(error)?;
        if current.control_digest != admitted.control_digest
            || current.approvals != admitted.approvals
            || snapshot
                .next_expiry
                .is_some_and(|at| at <= chrono::Utc::now())
        {
            return Err(error(crate::ServiceError::AccessChanged));
        }
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(snapshot.body, uri.as_str()).with_mime_type("application/json"),
        ])
        .with_ttl_ms(0)
        .with_cache_scope(CacheScope::Private)
        .into())
    }

    pub(super) async fn catalog(
        &self,
        identity: &GatewayInternalIdentity,
        admitted: &RequestAuthority,
        uri: &ResourceUri,
        address: KnowledgeResource,
    ) -> Result<CatalogSnapshot, ErrorData> {
        let mut next_expiry = None;
        let body = match address {
            KnowledgeResource::Contract => {
                serde_json::to_string(SETUP.documents().contract_declaration())
            }
            KnowledgeResource::Sources { after } => {
                let mut sources = self
                    .store
                    .readable_knowledge_sources(
                        &identity.authority.tenant,
                        &admitted.approvals,
                        &identity.actor.scopes,
                        after.as_ref(),
                    )
                    .await
                    .map_err(crate::ServiceError::from)
                    .map_err(error)?;
                let more = sources.len() > 100;
                sources.truncate(100);
                let page = sources
                    .into_iter()
                    .map(|row| source(row.server, &row.registrations))
                    .collect::<Result<Vec<_>, _>>()?;
                let next_cursor =
                    more.then(|| page.last().expect("full source page").server.clone());
                serde_json::to_string(&SourceCatalogPage {
                    items: page,
                    next_cursor,
                })
            }
            KnowledgeResource::Source(server) => {
                let rows = self
                    .store
                    .readable_knowledge_collections(
                        &identity.authority.tenant,
                        &admitted.approvals,
                        &identity.actor.scopes,
                        CatalogSelection::Source(&server),
                    )
                    .await
                    .map_err(crate::ServiceError::from)
                    .map_err(error)?;
                serde_json::to_string(&source(server, &rows)?)
            }
            KnowledgeResource::Collection(collection) => {
                let rows = self
                    .store
                    .readable_knowledge_collections(
                        &identity.authority.tenant,
                        &admitted.approvals,
                        &identity.actor.scopes,
                        CatalogSelection::Collection(&collection),
                    )
                    .await
                    .map_err(crate::ServiceError::from)
                    .map_err(error)?;
                let registration = rows.first().ok_or_else(|| {
                    ErrorData::resource_not_found("Knowledge collection is unavailable", None)
                })?;
                let active = self
                    .store
                    .active_knowledge_generation(&identity.authority.tenant)
                    .await
                    .map_err(crate::ServiceError::from)
                    .map_err(error)?;
                let mut generation = None;
                let mut statistics = None;
                if let Some(active) = active {
                    let spec = self
                        .store
                        .knowledge_generation(&identity.authority.tenant, active)
                        .await
                        .map_err(crate::ServiceError::from)
                        .map_err(error)?;
                    if spec.is_some_and(|spec| {
                        spec.collections().get(&collection) == Some(&registration.revision())
                    }) {
                        generation = Some(active);
                        let scope = admitted
                            .collection_scope(identity, registration)
                            .map_err(error)?;
                        let snapshot = self
                            .store
                            .knowledge_collection_statistics(&scope, active)
                            .await
                            .map_err(crate::ServiceError::from)
                            .map_err(error)?;
                        statistics = Some(snapshot.statistics);
                        next_expiry = snapshot.next_expiry;
                        if self
                            .store
                            .active_knowledge_generation(&identity.authority.tenant)
                            .await
                            .map_err(crate::ServiceError::from)
                            .map_err(error)?
                            != Some(active)
                        {
                            return Err(error(crate::ServiceError::AccessChanged));
                        }
                    }
                }
                serde_json::to_string(&CollectionCatalogEntry {
                    entity: CatalogEntity::Dataset,
                    uri: uri.clone(),
                    descriptor: registration.descriptor.clone(),
                    approval: registration.approval.clone(),
                    generation,
                    statistics,
                })
            }
            _ => {
                return Err(ErrorData::resource_not_found(
                    "Knowledge document is unavailable",
                    None,
                ));
            }
        }
        .map_err(|_| ErrorData::internal_error("invalid Knowledge catalog", None))?;
        Ok(CatalogSnapshot { body, next_expiry })
    }
}
pub(super) struct CatalogSnapshot {
    pub body: String,
    pub next_expiry: Option<chrono::DateTime<chrono::Utc>>,
}
fn source(
    server: ServerSlug,
    registrations: &[crate::contract::CollectionRegistration],
) -> Result<SourceCatalogEntry, ErrorData> {
    let first = registrations
        .first()
        .ok_or_else(|| ErrorData::resource_not_found("Knowledge source is unavailable", None))?;
    if registrations
        .iter()
        .any(|r| r.source_contract_revision != first.source_contract_revision)
    {
        return Err(ErrorData::internal_error(
            "Knowledge source declarations disagree; rediscovery required",
            None,
        ));
    }
    let uri = KnowledgeResource::Source(server.clone())
        .to_uri()
        .map_err(|_| ErrorData::internal_error("invalid source address", None))?;
    Ok(SourceCatalogEntry {
        entity: CatalogEntity::DataService,
        contract_revision: first.source_contract_revision,
        server,
        uri,
        collections: registrations
            .iter()
            .map(|r| r.descriptor.collection().clone())
            .collect(),
    })
}
