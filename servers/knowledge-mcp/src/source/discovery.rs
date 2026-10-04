//! Collection discovery over the same authenticated gateway peer as source reads.
use super::{GatewaySource, gateway::text};
use crate::ServiceError;
use rmcp::model::{
    ClientRequest, ListResourceTemplatesRequest, PaginatedRequestParams, ServerNotification,
    ServerResult, SubscriptionFilter,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_gateway_contract::GatewayDiscoveryFailureCode;
use veoveo_knowledge_contract::{
    CollectionRegistration, KnowledgeCollectionApproval, KnowledgeError,
};
use veoveo_mcp_contract::{
    GatewayDiscoveryDegradation, ServerResourceUris,
    docs::{CONTRACT_REVISION, ContractDeclaration},
};
use veoveo_mcp_knowledge_extension::{
    CollectionDescriptor, CollectionId, IndexingReadKind, client,
};
use veoveo_types::{ResourceScheme, Sha256Digest, TenantId};

pub struct ApprovedCollection {
    pub scheme: ResourceScheme,
    pub approval: KnowledgeCollectionApproval,
}
pub struct DiscoveryScope {
    pub tenant: TenantId,
    pub control_revision: Sha256Digest,
    pub collections: BTreeMap<CollectionId, ApprovedCollection>,
}
pub struct DiscoveredCollections {
    pub registrations: Vec<CollectionRegistration>,
    pub listener: GatewayCatalogListener,
}
pub struct GatewayCatalogListener(rmcp::service::Subscription);
impl GatewayCatalogListener {
    pub async fn changed(&mut self) -> Result<(), ServiceError> {
        match self.0.next().await {
            Ok(Some(ServerNotification::ResourceListChangedNotification(_))) => Ok(()),
            _ => Err(ServiceError::SourceUnavailable),
        }
    }
}
impl GatewaySource {
    /// Install the catalog listener before traversing any page. Partial gateway
    /// catalogs wait for their native completion event and never become an index.
    pub async fn discover(
        &self,
        scope: &DiscoveryScope,
    ) -> Result<DiscoveredCollections, ServiceError> {
        tokio::time::timeout(Duration::from_secs(60), self.discover_inner(scope))
            .await
            .map_err(|_| ServiceError::Deadline)?
    }
    async fn discover_inner(
        &self,
        scope: &DiscoveryScope,
    ) -> Result<DiscoveredCollections, ServiceError> {
        if scope.collections.is_empty() || scope.collections.len() > 1024 {
            return Err(
                KnowledgeError("source discovery requires 1..1024 approved collections").into(),
            );
        }
        for (id, source) in &scope.collections {
            source.approval.validate()?;
            if &source.approval.collection != id {
                return Err(KnowledgeError("approval collection mismatch").into());
            }
        }
        let filter = SubscriptionFilter::builder()
            .resources_list_changed()
            .build();
        let subscription = self
            .peer
            .listen(filter.clone())
            .await
            .map_err(|_| ServiceError::SourceUnavailable)?;
        if subscription.acknowledged() != &filter {
            return Err(ServiceError::SourceUnavailable);
        }
        let mut listener = GatewayCatalogListener(subscription);
        // The SDK acknowledgement accepts the filter before the gateway installs
        // its discovery-change receiver. Its first invalidation closes that gap.
        listener.changed().await?;
        // A changing catalog must settle within the complete operation deadline.
        for _ in 0..1024 {
            let selected = tokio::select! {
                biased;
                changed = listener.changed() => { changed?; continue; }
                selected = self.discover_pages(scope) => selected?,
            };
            let Some(selected) = selected else {
                listener.changed().await?;
                continue;
            };
            return Ok(DiscoveredCollections {
                registrations: selected,
                listener,
            });
        }
        Err(ServiceError::Traversal)
    }

    async fn discover_pages(
        &self,
        scope: &DiscoveryScope,
    ) -> Result<Option<Vec<CollectionRegistration>>, ServiceError> {
        let mut cursor = None;
        let mut cursors = BTreeSet::new();
        let mut descriptors = BTreeMap::<CollectionId, CollectionDescriptor>::new();
        let mut count = 0usize;
        loop {
            let mut params = PaginatedRequestParams::default();
            params.cursor = cursor;
            // Send through SDK dispatch without its optional URI/catalog cache;
            // each retry must observe the source discovery completion it just saw.
            let response = self
                .peer
                .send_request(ClientRequest::ListResourceTemplatesRequest(
                    ListResourceTemplatesRequest {
                        method: Default::default(),
                        params: Some(params),
                        extensions: Default::default(),
                    },
                ))
                .await
                .map_err(|_| ServiceError::SourceUnavailable)?;
            let ServerResult::ListResourceTemplatesResult(page) = response else {
                return Err(ServiceError::SourceUnavailable);
            };
            let degradation = GatewayDiscoveryDegradation::from_meta(page.meta.as_ref())
                .map_err(|_| KnowledgeError("invalid gateway discovery status"))?;
            if !degradation.is_empty() {
                if degradation
                    .failures
                    .iter()
                    .any(|failure| failure.code != GatewayDiscoveryFailureCode::DiscoveryPending)
                {
                    return Err(ServiceError::SourceUnavailable);
                }
                return Ok(None);
            }
            count += page.resource_templates.len();
            if count > 10_000 {
                return Err(ServiceError::Traversal);
            }
            for template in page.resource_templates {
                let Some(descriptor) = client::collection(&template)? else {
                    continue;
                };
                let Some(approved) = scope.collections.get(descriptor.collection()) else {
                    continue;
                };
                let root = super::enumeration_uri(&descriptor, None)?;
                if root
                    .components()
                    .map_err(|_| KnowledgeError("invalid collection root"))?
                    .scheme()
                    != approved.scheme.as_str()
                    || descriptors
                        .insert(descriptor.collection().clone(), descriptor)
                        .is_some()
                {
                    return Err(
                        KnowledgeError("duplicate collection or incorrect source scheme").into(),
                    );
                }
            }
            match page.next_cursor {
                Some(next)
                    if next.is_empty()
                        || next.len() > 4096
                        || !cursors.insert(next.clone())
                        || cursors.len() > 1000 =>
                {
                    return Err(ServiceError::Traversal);
                }
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        if descriptors.len() != scope.collections.len() {
            return Err(KnowledgeError(
                "approved source collection is absent from the gateway catalog",
            )
            .into());
        }
        let mut declarations = BTreeMap::new();
        let mut registrations = Vec::new();
        for (id, descriptor) in descriptors {
            let approved = &scope.collections[&id];
            let server = id.server();
            let revision = match declarations.get(server) {
                Some(revision) => *revision,
                None => {
                    let uri = ServerResourceUris::new(approved.scheme.clone()).contract_uri();
                    let result = self
                        .read_result(&descriptor, IndexingReadKind::SourceContract, &uri, None)
                        .await?;
                    let declaration: ContractDeclaration =
                        serde_json::from_str(text(&result, &uri, 256 * 1024)?)
                            .map_err(|_| KnowledgeError("invalid source contract declaration"))?;
                    if declaration.server != server.as_str()
                        || declaration.contract_revision != CONTRACT_REVISION
                    {
                        return Err(KnowledgeError(
                            "source must implement the current hosted contract revision",
                        )
                        .into());
                    }
                    declarations.insert(server.clone(), declaration.contract_revision);
                    declaration.contract_revision
                }
            };
            let registration = CollectionRegistration {
                tenant: scope.tenant.clone(),
                descriptor,
                source_contract_revision: revision,
                approval: approved.approval.clone(),
                control_revision: scope.control_revision.clone(),
            };
            registration.validate()?;
            registrations.push(registration);
        }
        Ok(Some(registrations))
    }
}
