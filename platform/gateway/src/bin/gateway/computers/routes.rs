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
    pub operation_id: Option<Uuid>,
    pub grant_id: Option<Uuid>,
    pub pairing_id: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Operation {
    List,
    Read(Uuid),
    Receipt { computer: Uuid, operation: Uuid },
    Access(Uuid),
    Automation(Uuid),
    AutomationGrant { computer: Uuid, grant: Uuid },
    GrantAutomation(Uuid),
    RevokeAutomation { computer: Uuid, grant: Uuid },
    RevokeAccess { computer: Uuid, grant: Uuid },
    Pairing(Uuid),
    ConfirmPairing { computer: Uuid, pairing: Uuid },
    Create,
    Start(Uuid),
    Stop(Uuid),
    Ticket(Uuid),
    Terminal(Uuid),
}
impl Operation {
    pub fn from_route(
        matched: &str,
        method: &Method,
        id: Option<Uuid>,
        operation_id: Option<Uuid>,
        grant_id: Option<Uuid>,
        pairing_id: Option<Uuid>,
    ) -> Result<Self, Fault> {
        if id.is_some_and(|id| id.is_nil())
            || operation_id.is_some_and(|id| id.is_nil())
            || grant_id.is_some_and(|id| id.is_nil())
            || pairing_id.is_some_and(|id| id.is_nil())
            || [grant_id, operation_id, pairing_id]
                .iter()
                .flatten()
                .count()
                > 1
        {
            return Err(Fault::invalid());
        }
        if let Some(pairing) = pairing_id {
            return match (matched, method, id) {
                (
                    "/computers/{profile}/{id}/cli-pairings/{pairing_id}/confirm",
                    &Method::POST,
                    Some(computer),
                ) => Ok(Self::ConfirmPairing { computer, pairing }),
                _ => Err(Fault::invalid()),
            };
        }
        if let Some(grant) = grant_id {
            return match (matched, method, id) {
                (
                    "/computers/{profile}/{id}/automation/{grant_id}",
                    &Method::GET,
                    Some(computer),
                ) => Ok(Self::AutomationGrant { computer, grant }),
                (
                    "/computers/{profile}/{id}/automation/{grant_id}/revoke",
                    &Method::POST,
                    Some(computer),
                ) => Ok(Self::RevokeAutomation { computer, grant }),
                (
                    "/computers/{profile}/{id}/access/{grant_id}/revoke",
                    &Method::POST,
                    Some(computer),
                ) => Ok(Self::RevokeAccess { computer, grant }),
                _ => Err(Fault::invalid()),
            };
        }
        if let Some(operation) = operation_id {
            return match (matched, method, id) {
                (
                    "/computers/{profile}/{id}/operations/{operation_id}",
                    &Method::GET,
                    Some(computer),
                ) => Ok(Self::Receipt {
                    computer,
                    operation,
                }),
                _ => Err(Fault::invalid()),
            };
        }
        match (matched, method, id) {
            ("/computers/{profile}", &Method::GET, None) => Ok(Self::List),
            ("/computers/{profile}", &Method::POST, None) => Ok(Self::Create),
            ("/computers/{profile}/{id}", &Method::GET, Some(id)) => Ok(Self::Read(id)),
            ("/computers/{profile}/{id}/access", &Method::GET, Some(id)) => Ok(Self::Access(id)),
            ("/computers/{profile}/{id}/automation", &Method::GET, Some(id)) => {
                Ok(Self::Automation(id))
            }
            ("/computers/{profile}/{id}/automation", &Method::POST, Some(id)) => {
                Ok(Self::GrantAutomation(id))
            }
            ("/computers/{profile}/{id}/cli-pairings", &Method::POST, Some(id)) => {
                Ok(Self::Pairing(id))
            }
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
            Self::Access(id) => format!("computers/{id}/access"),
            Self::Automation(id) | Self::GrantAutomation(id) => {
                format!("computers/{id}/automation")
            }
            Self::AutomationGrant { computer, grant } => {
                format!("computers/{computer}/automation/{grant}")
            }
            Self::RevokeAutomation { computer, grant } => {
                format!("computers/{computer}/automation/{grant}/revoke")
            }
            Self::Pairing(id) => format!("computers/{id}/cli-pairings"),
            Self::ConfirmPairing { computer, pairing } => {
                format!("computers/{computer}/cli-pairings/{pairing}/confirm")
            }
            Self::RevokeAccess { computer, grant } => {
                format!("computers/{computer}/access/{grant}/revoke")
            }
            Self::Receipt {
                computer,
                operation,
            } => format!("computers/{computer}/operations/{operation}"),
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
            Self::GrantAutomation(_) => Some("grant_automation"),
            Self::RevokeAutomation { .. } => Some("revoke_automation"),
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
            Self::Automation(id) => format!("computer://computers/{id}/automation"),
            Self::AutomationGrant { computer, grant } => {
                veoveo_computers_contract::automation_grant_uri(computer, grant)
            }
            Self::Read(id)
            | Self::Access(id)
            | Self::RevokeAccess { computer: id, .. }
            | Self::Pairing(id)
            | Self::ConfirmPairing { computer: id, .. }
            | Self::Ticket(id)
            | Self::Terminal(id)
            | Self::Receipt { computer: id, .. } => veoveo_computers_contract::computer_uri(id),
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
        matches!(
            self,
            Self::Ticket(_) | Self::Terminal(_) | Self::Pairing(_) | Self::ConfirmPairing { .. }
        )
    }
    pub fn requires_json(self) -> bool {
        matches!(
            self,
            Self::Create
                | Self::Start(_)
                | Self::Stop(_)
                | Self::GrantAutomation(_)
                | Self::RevokeAutomation { .. }
                | Self::Ticket(_)
                | Self::RevokeAccess { .. }
                | Self::Pairing(_)
                | Self::ConfirmPairing { .. }
        )
    }
    pub fn requires_contributor(self) -> bool {
        !matches!(
            self,
            Self::List
                | Self::Read(_)
                | Self::Receipt { .. }
                | Self::Access(_)
                | Self::Automation(_)
                | Self::AutomationGrant { .. }
                | Self::RevokeAccess { .. }
        )
    }
}
