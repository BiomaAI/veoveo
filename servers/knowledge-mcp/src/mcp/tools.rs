use super::*;
use crate::{
    contract::{EmbedRequest, EmbedResponse, SearchRequest, SearchResponse},
    search::SearchService,
};
use rmcp::handler::server::{
    router::tool::ToolRoute,
    tool::{ToolCallContext, schema_for_type},
};
use serde::{Serialize, de::DeserializeOwned};
use veoveo_types::LocalToolName;

pub(super) fn scope(name: &str) -> Option<KnowledgeScope> {
    match name {
        "search" => Some(KnowledgeScope::Search),
        "embed" => Some(KnowledgeScope::Embed),
        _ => None,
    }
}
pub(super) fn definitions() -> Vec<Tool> {
    let annotations = ToolAnnotations::new()
        .read_only(true)
        .destructive(false)
        .idempotent(true)
        .open_world(false);
    vec![
        Tool::new("search", "Find readable members in approved Knowledge collections. Follow source links for current content.", schema_for_type::<SearchRequest>())
            .with_title("Search knowledge").with_output_schema::<SearchResponse>().with_annotations(annotations.clone()),
        Tool::new("embed", "Embed document or query texts in the installation's declared embedding space.", schema_for_type::<EmbedRequest>())
            .with_title("Embed texts").with_output_schema::<EmbedResponse>().with_annotations(annotations),
    ]
}
/// The declared tools, each dispatched through [`KnowledgeMcp::call`] so policy
/// and scope checks stay in one place.
pub(super) fn router<E: Embeddings + 'static>() -> ToolRouter<KnowledgeMcp<E>> {
    let mut router = ToolRouter::new();
    for tool in definitions() {
        router.add_route(ToolRoute::new_dyn(
            tool,
            |call: ToolCallContext<'_, KnowledgeMcp<E>>| {
                Box::pin(async move {
                    let mut request = CallToolRequestParams::new(call.name.clone());
                    request.arguments = call.arguments;
                    call.service
                        .call(request, call.request_context)
                        .await
                        .map(Into::into)
                })
            },
        ));
    }
    router
}
impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    pub(super) async fn call(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let required = scope(&request.name)
            .ok_or_else(|| ErrorData::invalid_params("unknown Knowledge tool", None))?;
        let target = PolicyTarget::Tool {
            server: "knowledge".parse().unwrap(),
            tool: LocalToolName::new(request.name.as_ref())
                .map_err(|_| ErrorData::invalid_params("invalid tool name", None))?,
        };
        let (identity, admitted) = self
            .authority(&context, required, GatewayAction::ToolsCall, &target)
            .await?;
        let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            match required {
                KnowledgeScope::Search => {
                    let input: SearchRequest = input(&request)?;
                    let output = SearchService {
                        store: &self.store,
                        embeddings: self.embeddings.as_ref(),
                    }
                    .search(&admitted.caller, &input)
                    .await
                    .map_err(error)?;
                    let mut result = structured(&output)?;
                    result.content.extend(output.results.iter().map(|item| {
                        ContentBlock::resource_link(
                            Resource::new(item.uri.as_str(), item.title.as_str())
                                .with_title(item.title.as_str()),
                        )
                    }));
                    Ok(result)
                }
                KnowledgeScope::Embed => {
                    let input: EmbedRequest = input(&request)?;
                    let vectors = match input {
                        EmbedRequest::Document { texts } => self.embeddings.documents(texts).await,
                        EmbedRequest::Query { texts, task } => {
                            self.embeddings.queries(task, texts).await
                        }
                    }
                    .map_err(error)?;
                    structured(&EmbedResponse {
                        space: self.embeddings.space().clone(),
                        vectors,
                    })
                }
                _ => unreachable!("declared tool scope"),
            }
        })
        .await
        .map_err(|_| error(crate::ServiceError::Deadline))??;
        let current = authorize(
            &self.store,
            &identity,
            required,
            GatewayAction::ToolsCall,
            &target,
        )
        .await
        .map_err(error)?;
        if current.control_digest != admitted.control_digest
            || current.collections != admitted.collections
        {
            return Err(error(crate::ServiceError::AccessChanged));
        }
        Ok(result)
    }
}
fn input<T: DeserializeOwned>(request: &CallToolRequestParams) -> Result<T, ErrorData> {
    serde_json::from_value(serde_json::Value::Object(
        request.arguments.clone().unwrap_or_default(),
    ))
    .map_err(|_| {
        ErrorData::invalid_params("arguments do not match the Knowledge tool schema", None)
    })
}
fn structured<T: Serialize>(output: &T) -> Result<CallToolResult, ErrorData> {
    let value = serde_json::to_value(output)
        .map_err(|_| ErrorData::internal_error("invalid Knowledge result", None))?;
    let mut result = CallToolResult::success(vec![ContentBlock::text(value.to_string())]);
    result.structured_content = Some(value);
    Ok(result)
}
