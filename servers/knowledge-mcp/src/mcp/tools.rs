use super::*;
use crate::{
    contract::{EmbedRequest, EmbedResponse, SearchRequest, SearchResponse},
    search::SearchService,
};
use rmcp::{handler::server::wrapper::Parameters, tool, tool_router};
use serde::Serialize;
use veoveo_types::LocalToolName;

/// Names here classify the declared catalog for caller-specific visibility;
/// invocation dispatch and argument decoding belong to the generated router.
pub(super) fn scope(name: &str) -> Option<KnowledgeScope> {
    match name {
        "search" => Some(KnowledgeScope::Search),
        "embed" => Some(KnowledgeScope::Embed),
        _ => None,
    }
}

struct ToolAuthority {
    identity: GatewayInternalIdentity,
    admitted: RequestAuthority,
    required: KnowledgeScope,
    target: PolicyTarget,
}

#[tool_router(router = declared_tool_router, vis = "pub(super)")]
impl<E: Embeddings + 'static> KnowledgeMcp<E> {
    #[tool(
        title = "Search knowledge",
        description = "Find readable members in approved Knowledge collections. Follow source links for current content.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SearchResponse>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn search(
        &self,
        Parameters(input): Parameters<SearchRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let authority = self
            .admit_tool(&context, KnowledgeScope::Search, Self::search_tool_attr())
            .await?;
        let service = SearchService {
            store: &self.store,
            embeddings: self.embeddings.as_ref(),
        };
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            service.search(&authority.admitted.caller, &input),
        )
        .await
        .map_err(|_| error(crate::ServiceError::Deadline))?
        .map_err(error)?;
        self.recheck_tool(&authority).await?;
        let mut result = structured(&output)?;
        result.content.extend(output.results.iter().map(|item| {
            ContentBlock::resource_link(
                Resource::new(item.uri.as_str(), item.title.as_str())
                    .with_title(item.title.as_str()),
            )
        }));
        Ok(result)
    }

    #[tool(
        title = "Embed texts",
        description = "Embed document or query texts in the installation's declared embedding space.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<EmbedResponse>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn embed(
        &self,
        Parameters(input): Parameters<EmbedRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let authority = self
            .admit_tool(&context, KnowledgeScope::Embed, Self::embed_tool_attr())
            .await?;
        let vectors = tokio::time::timeout(std::time::Duration::from_secs(60), async {
            match input {
                EmbedRequest::Document { texts } => self.embeddings.documents(texts).await,
                EmbedRequest::Query { texts, task } => self.embeddings.queries(task, texts).await,
            }
        })
        .await
        .map_err(|_| error(crate::ServiceError::Deadline))?
        .map_err(error)?;
        self.recheck_tool(&authority).await?;
        structured(&EmbedResponse {
            space: self.embeddings.space().clone(),
            vectors,
        })
    }

    async fn admit_tool(
        &self,
        context: &RequestContext<RoleServer>,
        required: KnowledgeScope,
        tool: Tool,
    ) -> Result<ToolAuthority, ErrorData> {
        let target = PolicyTarget::Tool {
            server: "knowledge".parse().unwrap(),
            tool: LocalToolName::parse(tool.name.as_ref()).expect("declared Knowledge tool"),
        };
        let (identity, admitted) = self
            .authority(context, required, GatewayAction::ToolsCall, &target)
            .await?;
        Ok(ToolAuthority {
            identity,
            admitted,
            required,
            target,
        })
    }

    async fn recheck_tool(&self, authority: &ToolAuthority) -> Result<(), ErrorData> {
        let current = authorize(
            &self.store,
            &self.catalog_registry,
            &authority.identity,
            authority.required,
            GatewayAction::ToolsCall,
            &authority.target,
        )
        .await
        .map_err(error)?;
        if current.control_digest != authority.admitted.control_digest
            || current.collections != authority.admitted.collections
        {
            return Err(error(crate::ServiceError::AccessChanged));
        }
        Ok(())
    }
}

fn structured<T: Serialize>(output: &T) -> Result<CallToolResult, ErrorData> {
    let value = serde_json::to_value(output)
        .map_err(|_| ErrorData::internal_error("invalid Knowledge result", None))?;
    let mut result = CallToolResult::success(vec![ContentBlock::text(value.to_string())]);
    result.structured_content = Some(value);
    Ok(result)
}
