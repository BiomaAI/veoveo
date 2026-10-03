use super::*;
use crate::contract::{MapKnowledgeMember, MapKnowledgePageUri};

impl MapMcp {
    pub(super) async fn read_knowledge_page(
        &self,
        address: MapKnowledgePageUri,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, address.collection().scope())?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let page = crate::knowledge::enumerate(
            &self.state.catalog,
            &self.state.analytics,
            &identity,
            &scope,
            &address,
        )
        .await
        .map_err(internal)?;
        json_read(uri, &page)
    }
    pub(super) async fn read_knowledge_member(
        &self,
        address: MapKnowledgeMember,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, address.collection().scope())?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let member = crate::knowledge::read(
            &self.state.catalog,
            &self.state.analytics,
            &identity,
            &scope,
            &address,
        )
        .await
        .map_err(internal)?
        .ok_or_else(|| not_found("Map knowledge member"))?;
        if member.address != address {
            return Err(internal("Map member identity mismatch"));
        }
        let (text, observation) = member.document().map_err(internal)?;
        let result = veoveo_mcp_knowledge_extension::server::member_result(
            &address.to_uri(),
            "application/json",
            text,
            observation,
            &address.collection().descriptor(),
            Some(&context.meta),
        )
        .map_err(invalid_params)?;
        Ok(result
            .with_ttl_ms(0)
            .with_cache_scope(rmcp::model::CacheScope::Private))
    }
}
