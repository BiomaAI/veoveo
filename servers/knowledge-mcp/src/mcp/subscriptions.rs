//! One database observer per host; request-owned, authorized catalog snapshots.
use super::*;
use crate::contract::KnowledgeResource;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use rmcp::service::SubscriptionContext;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;
use veoveo_platform_store::{PlatformTable, ResourceChangeTable, ResourceInvalidation};
use veoveo_types::{ResourceAddress, ResourceUri, Sha256Digest};

struct Snapshot {
    resources: BTreeMap<ResourceUri, Sha256Digest>,
    discovery: Option<Sha256Digest>,
    deadline: DateTime<Utc>,
}

impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    pub(crate) fn observe_catalog(&self, cancellation: CancellationToken) {
        let store = self.store.clone();
        let changes = self.changes.clone();
        tokio::spawn(async move {
            use PlatformTable::*;
            let mut tables: Vec<ResourceChangeTable> = vec![
                KnowledgeCollection,
                KnowledgeGeneration,
                KnowledgeActive,
                KnowledgeMember,
                GatewayControlActive,
                GatewayRefreshFamily,
                GatewayJwtRevocation,
                Enterprise,
                Tenant,
                Principal,
                ManagedAgent,
                AgentDefinition,
                WorkContext,
            ]
            .into_iter()
            .map(Into::into)
            .collect();
            tables.extend([
                ResourceChangeTable::KnowledgeSync,
                ResourceChangeTable::KnowledgeCoordinator,
            ]);
            let mut stream = store.resource_changes(tables);
            loop {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    value = stream.next() => match value {
                        Some(value) => { changes.send_replace(Some(value)); }
                        None => break,
                    },
                }
            }
        });
    }

    pub(super) async fn listen_catalog(
        &self,
        context: SubscriptionContext,
    ) -> Result<(), ErrorData> {
        // Subscribe before any baseline read. Coalescing is safe because each wake
        // re-reads current SQL state; it never forwards database rows to callers.
        let mut changes = self.changes.subscribe();
        let mut resources = BTreeMap::new();
        for text in context.accepted().resource_subscriptions.iter().flatten() {
            let uri = ResourceUri::new(text).map_err(|_| invalid())?;
            let address = KnowledgeResource::parse(&uri).map_err(|_| invalid())?;
            if !matches!(
                address,
                KnowledgeResource::Sources { .. }
                    | KnowledgeResource::Source(_)
                    | KnowledgeResource::Collection(_)
            ) {
                return Err(invalid());
            }
            resources.insert(uri, address);
        }
        if resources.len() > 32 {
            return Err(ErrorData::invalid_params(
                "at most 32 Knowledge resources per listener",
                None,
            ));
        }
        let ready = async {
            while changes.borrow_and_update().is_none() {
                changes.changed().await.map_err(|_| unavailable())?;
            }
            Ok::<_, ErrorData>(())
        };
        tokio::select! {
            _ = context.cancelled() => return Ok(()),
            value = tokio::time::timeout(std::time::Duration::from_secs(10), ready) => value.map_err(|_| unavailable())??,
        }
        let mut previous = self.subscription_snapshot(&context, &resources).await?;
        self.notify_snapshot(&context, &previous, None, true)
            .await?;
        loop {
            let wait = (previous.deadline - Utc::now())
                .to_std()
                .unwrap_or_default();
            let reconcile = tokio::select! {
                _ = context.cancelled() => return Ok(()),
                _ = tokio::time::sleep(wait) => false,
                changed = changes.changed() => {
                    changed.map_err(|_| unavailable())?;
                    !matches!(*changes.borrow_and_update(), Some(ResourceInvalidation::Live))
                }
            };
            let current = tokio::select! {
                _ = context.cancelled() => return Ok(()),
                current = self.subscription_snapshot(&context, &resources) => current?,
            };
            self.notify_snapshot(&context, &current, Some(&previous), reconcile)
                .await?;
            previous = current;
        }
    }

    async fn subscription_snapshot(
        &self,
        context: &SubscriptionContext,
        resources: &BTreeMap<ResourceUri, KnowledgeResource>,
    ) -> Result<Snapshot, ErrorData> {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let request = context.request_context();
            let identity = &gateway_identity(request)?;
            let admitted =
                crate::authority::authenticate(&self.store, identity, KnowledgeScope::Read)
                    .await
                    .map_err(error)?;
            let mut snapshot = Snapshot {
                resources: BTreeMap::new(),
                discovery: None,
                deadline: identity.expires_at.min(
                    identity
                        .request_context
                        .as_ref()
                        .expect("admitted identity")
                        .access_token
                        .expires_at,
                ),
            };
            if context.accepted().resources_list_changed == Some(true) {
                if !admitted.allows(
                    identity,
                    GatewayAction::SubscriptionsListen,
                    &PolicyTarget::Server {
                        server: "knowledge".parse().unwrap(),
                    },
                ) {
                    return Err(error(crate::ServiceError::AccessChanged));
                }
                let listing =
                    DomainServer::list_resources(self, SETUP.declared_resources(), None, request)
                        .await?;
                let page = SETUP.list_resources(listing, None)?;
                snapshot.discovery = Some(digest(
                    &serde_json::to_vec(&page).map_err(|_| unavailable())?,
                ));
            }
            for (uri, address) in resources {
                let target = PolicyTarget::Resource {
                    server: "knowledge".parse().unwrap(),
                    uri: uri.clone(),
                };
                if ![
                    GatewayAction::ResourcesRead,
                    GatewayAction::SubscriptionsListen,
                ]
                .into_iter()
                .all(|action| admitted.allows(identity, action, &target))
                {
                    return Err(error(crate::ServiceError::AccessChanged));
                }
                let value = self
                    .catalog(identity, &admitted, uri, address.clone())
                    .await?;
                if let Some(deadline) = value.next_expiry {
                    snapshot.deadline = snapshot.deadline.min(deadline);
                }
                snapshot
                    .resources
                    .insert(uri.clone(), digest(value.body.as_bytes()));
            }
            let current =
                crate::authority::authenticate(&self.store, identity, KnowledgeScope::Read)
                    .await
                    .map_err(error)?;
            if current.control_digest != admitted.control_digest
                || current.approvals != admitted.approvals
                || snapshot.deadline <= Utc::now()
            {
                return Err(error(crate::ServiceError::AccessChanged));
            }
            Ok(snapshot)
        })
        .await
        .map_err(|_| unavailable())?
    }

    async fn notify_snapshot(
        &self,
        context: &SubscriptionContext,
        current: &Snapshot,
        previous: Option<&Snapshot>,
        reconcile: bool,
    ) -> Result<(), ErrorData> {
        // The sink owns bounded backpressure. Cancelling the request cancels delivery.
        let send = async {
            if current.discovery.is_some()
                && (reconcile || previous.is_none_or(|p| p.discovery != current.discovery))
            {
                context
                    .sink()
                    .notify_resource_list_changed()
                    .await
                    .map_err(|_| unavailable())?;
            }
            for (uri, digest) in &current.resources {
                if reconcile || previous.is_none_or(|p| p.resources.get(uri) != Some(digest)) {
                    context
                        .sink()
                        .notify_resource_updated(uri.to_string())
                        .await
                        .map_err(|_| unavailable())?;
                }
            }
            Ok(())
        };
        tokio::select! {
            _ = context.cancelled() => Ok(()),
            result = tokio::time::timeout(std::time::Duration::from_secs(10), send) => result.map_err(|_| unavailable())?,
        }
    }
}
fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn invalid() -> ErrorData {
    ErrorData::invalid_params(
        "resource is immutable or not a Knowledge catalog address",
        None,
    )
}
fn unavailable() -> ErrorData {
    ErrorData::internal_error(
        "Knowledge catalog observation is unavailable; reconnect and read current state",
        None,
    )
}
