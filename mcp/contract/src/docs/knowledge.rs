//! The same authenticated docs path handles ordinary and conditional reads.
use super::ServerDocs;
use rmcp::{
    ErrorData, RoleServer,
    model::{
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, ResourceContents,
        ResourceTemplate, ServerCapabilities,
    },
    service::RequestContext,
};
use veoveo_mcp_knowledge_extension::{self as knowledge, DocumentId};
use veoveo_types::{ResourceScheme, ResourceUriParts, ServerSlug};

impl ServerDocs {
    pub fn document_page(
        &self,
        scheme: &ResourceScheme,
        cursor: Option<&DocumentId>,
    ) -> Result<knowledge::docs::DocumentPage, knowledge::KnowledgeError> {
        let entries = self
            .iter()
            .map(|doc| {
                let id = DocumentId::parse(doc.id)?;
                Ok(knowledge::docs::DocumentEntry {
                    uri: knowledge::docs::member_uri(scheme, &id),
                    id,
                    title: doc.title.to_owned(),
                })
            })
            .collect::<Result<Vec<_>, knowledge::KnowledgeError>>()?;
        knowledge::docs::page(entries, cursor)
    }
    pub fn declare_knowledge(&self, capabilities: &mut ServerCapabilities) {
        knowledge::server::declare(capabilities);
    }

    pub fn knowledge_template(
        &self,
        scheme: &ResourceScheme,
        template: &mut ResourceTemplate,
    ) -> Result<(), knowledge::KnowledgeError> {
        if template.uri_template != knowledge::docs::member_template(scheme).as_str() {
            return Err(knowledge::KnowledgeError(
                "docs template does not match its scheme",
            ));
        }
        let slug = ServerSlug::parse(self.server())
            .map_err(|_| knowledge::KnowledgeError("invalid document owner"))?;
        knowledge::server::attach_collection(template, &knowledge::docs::collection(&slug, scheme));
        Ok(())
    }

    /// Handles only this server's docs routes. The gateway-auth middleware must
    /// have admitted the profile; missing authenticated identity fails closed.
    /// Domain resource handlers continue to own every other route and policy.
    pub fn read_knowledge(
        &self,
        scheme: &ResourceScheme,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<ReadResourceResponse>, ErrorData> {
        let parts = ResourceUriParts::parse(&request.uri)
            .map_err(|_| ErrorData::invalid_params("invalid resource URI", None))?;
        if parts.scheme() != scheme.as_str() || parts.authority() != "docs" {
            return Ok(None);
        }
        let http = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| ErrorData::invalid_request(crate::GATEWAY_ROUTING_REQUIRED, None))?;
        http.extensions
            .get::<crate::GatewayInternalIdentity>()
            .ok_or_else(|| ErrorData::invalid_request(crate::GATEWAY_ROUTING_REQUIRED, None))?;
        self.read_authorized_knowledge(scheme, request, &context.meta)
    }

    /// For adapters with their own authenticated profile boundary. Call only
    /// after that boundary admits the caller; conditional reads require the same check.
    pub fn read_authorized_knowledge(
        &self,
        scheme: &ResourceScheme,
        request: &ReadResourceRequestParams,
        meta: &rmcp::model::RequestMetaObject,
    ) -> Result<Option<ReadResourceResponse>, ErrorData> {
        let parts = ResourceUriParts::parse(&request.uri)
            .map_err(|_| ErrorData::invalid_params("invalid resource URI", None))?;
        if parts.scheme() != scheme.as_str() || parts.authority() != "docs" {
            return Ok(None);
        }
        if request.request_state.is_some() || request.input_responses.is_some() {
            return Err(ErrorData::invalid_params(
                "document reads do not accept continuation state",
                None,
            ));
        }
        let path: Vec<_> = parts.path_segments().collect();
        let invalid = |_| ErrorData::invalid_params("invalid document address or cursor", None);
        let result = match path.as_slice() {
            [] => {
                if parts.query_parameters().keys().any(|k| k != "cursor") {
                    return Err(invalid(()));
                }
                let cursor = parts
                    .query_parameters()
                    .get("cursor")
                    .map(|s| DocumentId::parse(s.clone()))
                    .transpose()
                    .map_err(|_| invalid(()))?;
                let page = self
                    .document_page(scheme, cursor.as_ref())
                    .map_err(|_| invalid(()))?;
                ReadResourceResult::new(vec![
                    ResourceContents::text(
                        serde_json::to_string(&page).map_err(|_| {
                            ErrorData::internal_error("document page serialization failed", None)
                        })?,
                        &request.uri,
                    )
                    .with_mime_type("application/json"),
                ])
            }
            [id] if !parts.has_query() => {
                let id = DocumentId::parse(id.as_ref()).map_err(|_| invalid(()))?;
                let doc = self
                    .doc(id.as_str())
                    .ok_or_else(|| ErrorData::resource_not_found("unknown document", None))?;
                let slug = ServerSlug::parse(self.server())
                    .map_err(|_| ErrorData::internal_error("invalid document owner", None))?;
                let collection = knowledge::docs::collection(&slug, scheme);
                let observation = knowledge::docs::observation(
                    &collection,
                    doc.digest.clone(),
                    chrono::Utc::now(),
                );
                let uri = parts.clone().into_uri();
                knowledge::server::member_result(
                    &uri,
                    "text/markdown",
                    doc.body.to_owned(),
                    observation,
                    &collection,
                    Some(meta),
                )
                .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?
            }
            _ => return Err(invalid(())),
        };
        Ok(Some(crate::private_resource_response(result, true)))
    }
}
