//! Selection for the knowledge requirements at a direct or gateway MCP endpoint.
use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use rmcp::model::{ResourceTemplate, Tool};
use url::Url;
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_knowledge_extension::client;
use veoveo_types::{LocalToolName, ResourceScheme, ServerSlug};

/// The transport's tool naming contract. Resource addresses retain their owners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeRoute {
    Direct,
    Gateway,
}

/// Checked source selection; credentials and domain lifecycle callbacks stay separate.
#[derive(Debug, Clone)]
pub struct KnowledgeSourceTarget {
    endpoint: Url,
    server: ServerSlug,
    schemes: BTreeSet<ResourceScheme>,
    route: KnowledgeRoute,
}

impl KnowledgeSourceTarget {
    pub fn new(
        endpoint: Url,
        server: ServerSlug,
        schemes: BTreeSet<ResourceScheme>,
        route: KnowledgeRoute,
    ) -> Result<Self> {
        ensure!(
            matches!(endpoint.scheme(), "https" | "http")
                && endpoint.host_str().is_some()
                && endpoint.username().is_empty()
                && endpoint.password().is_none()
                && endpoint.fragment().is_none(),
            "knowledge endpoint requires HTTP(S) without URL credentials or a fragment"
        );
        ensure!(
            !schemes.is_empty(),
            "source must own at least one URI scheme"
        );
        Ok(Self {
            endpoint,
            server,
            schemes,
            route,
        })
    }

    pub fn endpoint(&self) -> &Url {
        &self.endpoint
    }
    pub fn server(&self) -> &ServerSlug {
        &self.server
    }
    pub(crate) fn gateway_server(&self) -> Option<&ServerSlug> {
        (self.route == KnowledgeRoute::Gateway).then_some(&self.server)
    }
    pub fn schemes(&self) -> &BTreeSet<ResourceScheme> {
        &self.schemes
    }
    pub(crate) fn owns_scheme(&self, scheme: &str) -> bool {
        self.schemes.iter().any(|owned| owned.as_str() == scheme)
    }

    pub(crate) fn tool_name(&self, local: &LocalToolName) -> Result<String> {
        Ok(match self.route {
            KnowledgeRoute::Direct => local.to_string(),
            KnowledgeRoute::Gateway => {
                GatewayToolName::from_parts(&self.server, local)?.to_string()
            }
        })
    }

    /// Hosted certification supplies one server's entire catalog. Gateway selection
    /// narrows by the declared collection owner, retaining duplicate declarations
    /// for K01 to reject rather than silently coalescing them.
    pub(crate) fn templates(
        &self,
        templates: Vec<ResourceTemplate>,
    ) -> Result<Vec<ResourceTemplate>> {
        if self.route == KnowledgeRoute::Direct {
            return Ok(templates);
        }
        let mut selected = Vec::new();
        for template in templates {
            if let Some(collection) = client::collection(&template)?
                && collection.collection().server() == &self.server
            {
                selected.push(template);
            }
        }
        Ok(selected)
    }

    pub(crate) fn tools(&self, tools: Vec<Tool>) -> Result<Vec<Tool>> {
        if self.route == KnowledgeRoute::Direct {
            return Ok(tools);
        }
        let mut selected = Vec::new();
        for mut tool in tools {
            let projected = GatewayToolName::parse(tool.name.as_ref())?;
            let (server, local) = projected
                .as_str()
                .split_once("__")
                .ok_or_else(|| anyhow::anyhow!("gateway tool has no source namespace"))?;
            ensure!(
                !local.contains("__"),
                "gateway tool has multiple namespace separators"
            );
            let server = ServerSlug::parse(server)?;
            let local = LocalToolName::parse(local)?;
            if server == self.server {
                ensure!(
                    self.tool_name(&local)? == projected.as_str(),
                    "gateway tool projection disagrees"
                );
                tool.name = local.to_string().into();
                selected.push(tool);
            }
        }
        Ok(selected)
    }
}

impl TryFrom<&crate::HostedServerConformanceProfile> for KnowledgeSourceTarget {
    type Error = anyhow::Error;
    fn try_from(profile: &crate::HostedServerConformanceProfile) -> Result<Self> {
        Self::new(
            Url::parse(&profile.endpoint)?,
            profile.server_slug.parse()?,
            profile
                .owned_resource_schemes
                .iter()
                .map(|s| s.parse())
                .collect::<Result<_, _>>()?,
            KnowledgeRoute::Direct,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_knowledge_extension::{docs, server};

    #[test]
    fn gateway_selection_preserves_owner_checks_and_projects_local_tools() -> Result<()> {
        let target = KnowledgeSourceTarget::new(
            Url::parse("https://installation.example/mcp/operator")?,
            "selected".parse()?,
            ["selected".parse()?].into(),
            KnowledgeRoute::Gateway,
        )?;
        let templates = ["selected", "other"].map(|owner| {
            let scheme = owner.parse().unwrap();
            let descriptor = docs::collection(&owner.parse().unwrap(), &scheme);
            let mut template = ResourceTemplate::new(
                docs::member_template(&scheme).to_string(),
                format!("{owner} docs"),
            );
            server::attach_collection(&mut template, &descriptor);
            template
        });
        let selected = target.templates(templates.to_vec())?;
        assert_eq!(selected.len(), 1);
        assert_eq!(
            target
                .templates(vec![selected[0].clone(), selected[0].clone()])?
                .len(),
            2
        );
        let tool =
            |name: &str| Tool::new(name.to_owned(), "fixture", rmcp::model::JsonObject::new());
        let selected = target.tools(vec![tool("other__search"), tool("selected__search")])?;
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "search");
        assert_eq!(target.tool_name(&"search".parse()?)?, "selected__search");
        assert!(target.tools(vec![tool("selected__search__other")]).is_err());
        assert!(target.tools(vec![tool("search")]).is_err());
        for endpoint in [
            "https://user:secret@example.test/mcp",
            "https://example.test/mcp#fragment",
            "file:///tmp/mcp",
        ] {
            assert!(
                KnowledgeSourceTarget::new(
                    Url::parse(endpoint)?,
                    "selected".parse()?,
                    ["selected".parse()?].into(),
                    KnowledgeRoute::Gateway
                )
                .is_err()
            );
        }
        Ok(())
    }
}
