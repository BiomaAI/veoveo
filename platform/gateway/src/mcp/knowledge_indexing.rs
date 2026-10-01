//! Registered indexing clients read only explicitly approved collections. The
//! source observation confirms membership before any content reaches the client.
use super::GatewayMcp;
use crate::{
    AuthenticatedSubject, GatewayCatalog,
    mcp_support::{mcp_internal, mcp_invalid_request},
};
use rmcp::model::{ErrorData, RequestMetaObject};
use veoveo_knowledge_contract::{
    CollectionApproval, CollectionRegistration, KnowledgeCollectionApproval,
};
use veoveo_mcp_contract::{GatewayAction, GatewayResourceProjection, PolicyTarget, PrincipalKind};
use veoveo_mcp_knowledge_extension::{
    INDEXING_READ_KEY, IndexingReadIntent, IndexingReadKind, Observation, is_enumeration_uri,
};
use veoveo_types::InvocationMode;

#[derive(PartialEq, Eq)]
pub(super) struct IndexingReadPermit {
    intent: IndexingReadIntent,
    admission: IndexingAdmission,
}
#[derive(PartialEq, Eq)]
enum IndexingAdmission {
    SourceContract(KnowledgeCollectionApproval),
    Collection(Box<CollectionRegistration>),
}
fn denied() -> ErrorData {
    mcp_invalid_request(
        "The indexing client may read only its currently approved Knowledge collections.",
    )
}

pub(super) fn allows_action(
    catalog: &GatewayCatalog,
    subject: &AuthenticatedSubject,
    action: GatewayAction,
    target: &PolicyTarget,
) -> bool {
    let Some(indexing) = catalog
        .oauth_client(&subject.access_token.oauth_client_id)
        .and_then(|client| client.knowledge_indexing.as_ref())
    else {
        return true;
    };
    if subject.principal.kind != PrincipalKind::Service
        || subject.access_token.invocation_mode != InvocationMode::Automated
    {
        return false;
    }
    if !matches!(
        action,
        GatewayAction::ResourcesList
            | GatewayAction::ResourcesTemplatesList
            | GatewayAction::ResourcesRead
            | GatewayAction::ArtifactRead
            | GatewayAction::SubscriptionsListen
    ) {
        return false;
    }
    let server = match target {
        PolicyTarget::Gateway => {
            return matches!(
                action,
                GatewayAction::ResourcesList
                    | GatewayAction::ResourcesTemplatesList
                    | GatewayAction::SubscriptionsListen
            );
        }
        PolicyTarget::Server { server }
        | PolicyTarget::Resource { server, .. }
        | PolicyTarget::ResourceTemplate { server, .. }
        | PolicyTarget::Artifact { server, .. } => server,
        _ => return false,
    };
    indexing
        .collections
        .iter()
        .any(|collection| collection.server() == server)
}

impl GatewayMcp {
    pub(super) async fn admit_indexing_subscription_filter(
        &self,
        subject: &AuthenticatedSubject,
        filter: &rmcp::model::SubscriptionFilter,
    ) -> Result<(), ErrorData> {
        let indexing = self
            .catalog
            .current()
            .oauth_client(&subject.access_token.oauth_client_id)
            .is_some_and(|client| client.knowledge_indexing.is_some());
        if !indexing
            || !(filter.tools_list_changed == Some(true)
                || filter.prompts_list_changed == Some(true)
                || filter.task_ids.as_ref().is_some_and(|ids| !ids.is_empty()))
        {
            return Ok(());
        }
        use veoveo_audit_contract::{
            AuditDetail, AuditOutcome, AuditReadMethod, AuditReason, AuditTarget,
        };
        let draft = subject
            .audit_draft(
                &self.profile_id,
                AuditTarget::Installation,
                AuditDetail::Read {
                    method: AuditReadMethod::Subscription,
                },
                AuditOutcome::Denied,
                AuditReason::PolicyDenied,
            )
            .map_err(|_| mcp_internal("invalid indexing subscription audit"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required indexing subscription audit unavailable"))?;
        Err(denied())
    }

    pub(super) async fn admit_indexing_read(
        &self,
        subject: &AuthenticatedSubject,
        projection: &GatewayResourceProjection,
        meta: &RequestMetaObject,
    ) -> Result<Option<IndexingReadPermit>, ErrorData> {
        let catalog = self.catalog.current();
        let indexing = catalog
            .oauth_client(&subject.access_token.oauth_client_id)
            .and_then(|client| client.knowledge_indexing.as_ref());
        let Some(indexing) = indexing else {
            if meta.contains_key(INDEXING_READ_KEY) {
                return Err(denied());
            }
            return Ok(None);
        };
        let intent: IndexingReadIntent = meta
            .get(INDEXING_READ_KEY)
            .cloned()
            .ok_or_else(denied)
            .and_then(|value| serde_json::from_value(value).map_err(|_| denied()))?;
        if !indexing.collections.contains(&intent.collection)
            || intent.collection.server() != &projection.server
            || subject.principal.kind != PrincipalKind::Service
            || subject.access_token.invocation_mode != InvocationMode::Automated
        {
            return Err(denied());
        }
        let manifest = catalog.server(&projection.server).ok_or_else(denied)?;
        let approval = manifest
            .knowledge
            .iter()
            .find(|entry| {
                entry.collection == intent.collection
                    && (entry.mode == CollectionApproval::Index
                        || intent.kind == IndexingReadKind::SourceContract)
            })
            .ok_or_else(denied)?;
        if intent.kind == IndexingReadKind::SourceContract {
            let expected =
                veoveo_mcp_contract::ServerResourceUris::new(manifest.uri_scheme.clone())
                    .contract_uri();
            if projection.upstream_uri != expected {
                return Err(denied());
            }
            return Ok(Some(IndexingReadPermit {
                intent,
                admission: IndexingAdmission::SourceContract(approval.clone()),
            }));
        }
        let registration = self
            .state
            .platform_store()
            .knowledge_collection(&subject.authority.tenant, &intent.collection)
            .await
            .map_err(|_| mcp_internal("knowledge collection registration unavailable"))?
            .ok_or_else(denied)?;
        if &registration.approval != approval {
            return Err(denied());
        }
        if intent.kind == IndexingReadKind::Enumeration
            && !is_enumeration_uri(&registration.descriptor, &projection.upstream_uri)
        {
            return Err(denied());
        }
        Ok(Some(IndexingReadPermit {
            intent,
            admission: IndexingAdmission::Collection(Box::new(registration)),
        }))
    }

    pub(super) async fn admit_indexing_subscription(
        &self,
        subject: &AuthenticatedSubject,
        projection: &GatewayResourceProjection,
        meta: &RequestMetaObject,
    ) -> Result<(), ErrorData> {
        let inferred;
        let meta = if !meta.contains_key(INDEXING_READ_KEY) {
            inferred = self.indexing_root_intent(subject, projection, meta).await?;
            &inferred
        } else {
            meta
        };
        let Some(permit) = self.admit_indexing_read(subject, projection, meta).await? else {
            return Ok(());
        };
        let IndexingAdmission::Collection(registration) = &permit.admission else {
            return Err(denied());
        };
        if permit.intent.kind == IndexingReadKind::Member
            && !self
                .state
                .platform_store()
                .knowledge_member_observed(registration, &projection.upstream_uri)
                .await
                .map_err(|_| mcp_internal("knowledge member subscription admission unavailable"))?
        {
            return Err(denied());
        }
        Ok(())
    }

    async fn indexing_root_intent(
        &self,
        subject: &AuthenticatedSubject,
        projection: &GatewayResourceProjection,
        meta: &RequestMetaObject,
    ) -> Result<RequestMetaObject, ErrorData> {
        let catalog = self.catalog.current();
        let Some(indexing) = catalog
            .oauth_client(&subject.access_token.oauth_client_id)
            .and_then(|client| client.knowledge_indexing.as_ref())
        else {
            return Ok(meta.clone());
        };
        let manifest = catalog.server(&projection.server).ok_or_else(denied)?;
        let approvals = manifest
            .knowledge
            .iter()
            .filter(|approval| {
                approval.mode == CollectionApproval::Index
                    && indexing.collections.contains(&approval.collection)
            })
            .map(|approval| (approval.collection.clone(), approval.clone()))
            .collect();
        let registration = self
            .state
            .platform_store()
            .knowledge_collection_at_root(
                &subject.authority.tenant,
                &projection.upstream_uri,
                &approvals,
                &subject.actor.scopes,
            )
            .await
            .map_err(|_| mcp_internal("knowledge root subscription admission unavailable"))?
            .ok_or_else(denied)?;
        let mut meta = meta.clone();
        meta.insert(
            INDEXING_READ_KEY.into(),
            serde_json::to_value(IndexingReadIntent {
                collection: registration.descriptor.collection().clone(),
                kind: IndexingReadKind::Enumeration,
            })
            .expect("typed indexing intent"),
        );
        Ok(meta)
    }

    pub(super) async fn validate_indexing_delivery(
        &self,
        subject: &AuthenticatedSubject,
        projection: &GatewayResourceProjection,
        permit: &IndexingReadPermit,
        observation: Option<&Observation>,
    ) -> Result<(), ErrorData> {
        // Approval may have changed while the source was executing. Re-admit
        // before delivery and reject a declaration/approval change during the read.
        let mut meta = RequestMetaObject::default();
        meta.insert(
            INDEXING_READ_KEY.into(),
            serde_json::to_value(&permit.intent).expect("typed indexing intent"),
        );
        let current = self
            .admit_indexing_read(subject, projection, &meta)
            .await?
            .ok_or_else(denied)?;
        if &current != permit {
            return Err(denied());
        }
        match &permit.admission {
            IndexingAdmission::SourceContract(_) => {
                if observation.is_none() {
                    Ok(())
                } else {
                    Err(denied())
                }
            }
            IndexingAdmission::Collection(registration) => {
                match (permit.intent.kind, observation) {
                    (IndexingReadKind::Member, None) => Err(denied()),
                    (_, Some(observation)) => registration
                        .admit_observation(observation)
                        .map_err(|_| denied()),
                    (IndexingReadKind::Enumeration, None) => Ok(()),
                    (IndexingReadKind::SourceContract, None) => Err(denied()),
                }
            }
        }
    }
}
