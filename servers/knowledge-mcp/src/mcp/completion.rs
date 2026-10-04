use super::*;
use veoveo_platform_store::knowledge::CatalogCompletion;
use veoveo_types::{ResourceAddress, ResourceTemplateUri};

impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    pub(super) async fn completion(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, ErrorData> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Err(ErrorData::invalid_params(
                "Knowledge completes resource templates only",
                None,
            ));
        };
        let template = ResourceTemplateUri::new(&reference.uri)
            .map_err(|_| ErrorData::invalid_params("invalid Knowledge template", None))?;
        if !SETUP
            .resource_templates()
            .iter()
            .any(|t| t.template() == &template)
        {
            return Err(ErrorData::invalid_params(
                "unknown Knowledge template",
                None,
            ));
        }
        let prefix = &request.argument.value;
        if prefix.len() > 512 || prefix.chars().any(char::is_control) {
            return Err(ErrorData::invalid_params("invalid completion prefix", None));
        }
        let target = PolicyTarget::ResourceTemplate {
            server: "knowledge".parse().unwrap(),
            uri: template,
        };
        let (identity, admitted) = self
            .authority(
                &context,
                KnowledgeScope::Read,
                GatewayAction::CompletionComplete,
                &target,
            )
            .await?;
        let domain = match (reference.uri.as_str(), request.argument.name.as_str()) {
            ("knowledge://source/{server}", "server")
            | ("knowledge://sources{?cursor}", "cursor") => Some(CatalogCompletion::Source),
            ("knowledge://collection/{collection}", "collection") => {
                Some(CatalogCompletion::Collection)
            }
            ("knowledge://docs/{doc_id}", "doc_id") | ("knowledge://docs{?cursor}", "cursor") => {
                None
            }
            _ => {
                return Err(ErrorData::invalid_params(
                    "unknown completion argument",
                    None,
                ));
            }
        };
        let values: Vec<String> = if let Some(domain) = domain {
            // Narrow the approval vocabulary before the database performs prefix
            // matching, deduplication and LIMIT. A specific resource policy can
            // exclude values even when this template is exposed.
            let approvals = admitted
                .approvals
                .iter()
                .filter(|(id, _)| {
                    let address = match domain {
                        CatalogCompletion::Source => {
                            crate::contract::KnowledgeResource::Source(id.server().clone())
                        }
                        CatalogCompletion::Collection => {
                            crate::contract::KnowledgeResource::Collection((*id).clone())
                        }
                    };
                    address.to_uri().is_ok_and(|uri| {
                        admitted.allows(
                            &identity,
                            GatewayAction::ResourcesRead,
                            &PolicyTarget::Resource {
                                server: "knowledge".parse().unwrap(),
                                uri,
                            },
                        )
                    })
                })
                .map(|(id, a)| (id.clone(), a.clone()))
                .collect();
            self.store
                .complete_knowledge_catalog(
                    &identity.authority.tenant,
                    &approvals,
                    &identity.actor.scopes,
                    domain,
                    prefix,
                )
                .await
                .map_err(crate::ServiceError::from)
                .map_err(error)?
                .into_iter()
                .map(|value| value.to_string())
                .collect()
        } else {
            setup::DOCUMENTS
                .iter()
                .filter(|doc| doc.id.starts_with(prefix))
                .filter(|doc| {
                    crate::contract::KnowledgeResource::Document(
                        doc.id.parse().expect("declared document"),
                    )
                    .to_uri()
                    .is_ok_and(|uri| {
                        admitted.allows(
                            &identity,
                            GatewayAction::ResourcesRead,
                            &PolicyTarget::Resource {
                                server: "knowledge".parse().unwrap(),
                                uri,
                            },
                        )
                    })
                })
                .map(|doc| doc.id.to_owned())
                .collect()
        };
        let current = authorize(
            &self.store,
            &self.catalog_registry,
            &identity,
            KnowledgeScope::Read,
            GatewayAction::CompletionComplete,
            &target,
        )
        .await
        .map_err(error)?;
        if current.control_digest != admitted.control_digest
            || current.approvals != admitted.approvals
        {
            return Err(error(crate::ServiceError::AccessChanged));
        }
        let more = values.len() > CompletionInfo::MAX_VALUES;
        let total = (!more).then_some(values.len() as u32);
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(
                values
                    .into_iter()
                    .take(CompletionInfo::MAX_VALUES)
                    .collect(),
                total,
                more,
            )
            .map_err(|_| ErrorData::internal_error("invalid completion page", None))?,
        ))
    }
}
