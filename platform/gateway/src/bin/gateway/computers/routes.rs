use super::Fault;
use axum::http::Method;
use serde::Deserialize;
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, GatewayProfileId, LocalToolName, PolicyTarget, ResourceUri, ServerSlug,
};

#[derive(Deserialize)]
pub(super) struct Route {
    pub profile: GatewayProfileId,
    pub id: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Operation {
    List,
    Read(Uuid),
    Create,
    Start(Uuid),
    Stop(Uuid),
    Ticket(Uuid),
    Terminal(Uuid),
}
impl Operation {
    pub fn from_route(matched: &str, method: &Method, id: Option<Uuid>) -> Result<Self, Fault> {
        if id.is_some_and(|id| id.is_nil()) {
            return Err(Fault::invalid());
        }
        match (matched, method, id) {
            ("/computers/{profile}", &Method::GET, None) => Ok(Self::List),
            ("/computers/{profile}", &Method::POST, None) => Ok(Self::Create),
            ("/computers/{profile}/{id}", &Method::GET, Some(id)) => Ok(Self::Read(id)),
            ("/computers/{profile}/{id}/start", &Method::POST, Some(id)) => Ok(Self::Start(id)),
            ("/computers/{profile}/{id}/stop", &Method::POST, Some(id)) => Ok(Self::Stop(id)),
            ("/computers/{profile}/{id}/terminal-ticket", &Method::POST, Some(id)) => {
                Ok(Self::Ticket(id))
            }
            ("/computers/{profile}/{id}/terminal", &Method::GET, Some(id)) => {
                Ok(Self::Terminal(id))
            }
            _ => Err(Fault::invalid()),
        }
    }
    pub fn service_path(self) -> String {
        match self {
            Self::List | Self::Create => "computers".into(),
            Self::Read(id) => format!("computers/{id}"),
            Self::Start(id) => format!("computers/{id}/start"),
            Self::Stop(id) => format!("computers/{id}/stop"),
            Self::Ticket(id) => format!("computers/{id}/terminal-ticket"),
            Self::Terminal(id) => format!("computers/{id}/terminal"),
        }
    }
    pub fn authorization(self) -> (PolicyTarget, &'static [GatewayAction]) {
        let server = ServerSlug::new("computers").expect("static server");
        let tool = match self {
            Self::Create => Some("create"),
            Self::Start(_) => Some("start"),
            Self::Stop(_) => Some("stop"),
            _ => None,
        };
        if let Some(tool) = tool {
            return (
                PolicyTarget::Tool {
                    server,
                    tool: LocalToolName::new(tool).expect("static tool"),
                },
                &[GatewayAction::ToolsCall],
            );
        }
        let uri = match self {
            Self::Read(id) | Self::Ticket(id) | Self::Terminal(id) => {
                veoveo_computers_contract::computer_uri(id)
            }
            _ => veoveo_computers_contract::COMPUTERS_URI.into(),
        };
        let actions: &'static [_] = if self.is_attachment() {
            &[GatewayAction::ComputerAttach, GatewayAction::ResourcesRead]
        } else {
            &[GatewayAction::ResourcesRead]
        };
        (
            PolicyTarget::Resource {
                server,
                uri: ResourceUri::new(uri).expect("canonical resource"),
            },
            actions,
        )
    }
    pub fn is_attachment(self) -> bool {
        matches!(self, Self::Ticket(_) | Self::Terminal(_))
    }
    pub fn requires_contributor(self) -> bool {
        !matches!(self, Self::List | Self::Read(_))
    }
}
