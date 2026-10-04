use super::Fault;
use axum::http::Method;
use serde::Deserialize;
use veoveo_computers_contract::{AutomationGrantId, ComputerId, ComputerResource, FileTransferId};
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{GatewayProfileId, LocalToolName, PolicyTarget, ServerSlug};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RouteAction {
    Kernel(GatewayAction),
    Owner(veoveo_computers_contract::ComputerAction),
}
impl RouteAction {
    pub fn resolve(
        self,
        registry: &veoveo_gateway_contract::CatalogRegistry,
    ) -> Result<veoveo_gateway_contract::PolicyAction, veoveo_types::ExtensionError> {
        match self {
            Self::Kernel(action) => Ok(action.into()),
            Self::Owner(action) => Ok(registry
                .action_key::<veoveo_computers_contract::ComputerAction>()?
                .action(action)?
                .into()),
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Route {
    pub profile: GatewayProfileId,
    pub id: Option<ComputerId>,
    pub operation_id: Option<veoveo_types::TaskId>,
    pub grant_id: Option<AutomationGrantId>,
    pub access_grant_id: Option<veoveo_computers_contract::AccessGrantId>,
    pub pairing_id: Option<veoveo_computers_contract::CliPairingId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Operation {
    List,
    Read(ComputerId),
    TransferFile(ComputerId),
    File {
        computer: ComputerId,
        operation: FileTransferId,
    },
    CancelFile {
        computer: ComputerId,
        operation: FileTransferId,
    },
    Receipt {
        computer: veoveo_computers_contract::ComputerId,
        operation: veoveo_types::TaskId,
    },
    Maintenance(ComputerId),
    MaintenanceReceipt {
        computer: veoveo_computers_contract::ComputerId,
        operation: veoveo_types::TaskId,
    },
    UpdateTemplate(ComputerId),
    ResumeUpdate {
        computer: veoveo_computers_contract::ComputerId,
        operation: veoveo_types::TaskId,
    },
    Access(ComputerId),
    Automation(ComputerId),
    AutomationGrant {
        computer: ComputerId,
        grant: AutomationGrantId,
    },
    GrantAutomation(ComputerId),
    RevokeAutomation {
        computer: ComputerId,
        grant: AutomationGrantId,
    },
    RevokeAccess {
        computer: veoveo_computers_contract::ComputerId,
        grant: veoveo_computers_contract::AccessGrantId,
    },
    Pairing(ComputerId),
    ConfirmPairing {
        computer: veoveo_computers_contract::ComputerId,
        pairing: veoveo_computers_contract::CliPairingId,
    },
    Create,
    Start(ComputerId),
    Stop(ComputerId),
    Ticket(ComputerId),
    Terminal(ComputerId),
}
impl Operation {
    pub fn from_route(
        matched: &str,
        method: &Method,
        id: Option<ComputerId>,
        operation_id: Option<veoveo_types::TaskId>,
        grant_id: Option<AutomationGrantId>,
        access_grant_id: Option<veoveo_computers_contract::AccessGrantId>,
        pairing_id: Option<veoveo_computers_contract::CliPairingId>,
    ) -> Result<Self, Fault> {
        if operation_id.is_some_and(|id| id.as_uuid().is_nil())
            || [
                grant_id.is_some(),
                access_grant_id.is_some(),
                operation_id.is_some(),
                pairing_id.is_some(),
            ]
            .into_iter()
            .filter(|present| *present)
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
                _ => Err(Fault::invalid()),
            };
        }
        if let Some(grant) = access_grant_id {
            return match (matched, method, id) {
                (
                    "/computers/{profile}/{id}/access/{access_grant_id}/revoke",
                    &Method::POST,
                    Some(computer),
                ) => Ok(Self::RevokeAccess { computer, grant }),
                _ => Err(Fault::invalid()),
            };
        }
        if let Some(operation) = operation_id {
            return match (matched, method, id) {
                (
                    "/computers/{profile}/{id}/files/{operation_id}",
                    &Method::GET,
                    Some(computer),
                ) => Ok(Self::File {
                    computer,
                    operation: FileTransferId::try_from(operation.as_uuid())
                        .map_err(|_| Fault::invalid())?,
                }),
                (
                    "/computers/{profile}/{id}/files/{operation_id}/cancel",
                    &Method::POST,
                    Some(computer),
                ) => Ok(Self::CancelFile {
                    computer,
                    operation: FileTransferId::try_from(operation.as_uuid())
                        .map_err(|_| Fault::invalid())?,
                }),
                (
                    "/computers/{profile}/{id}/maintenance/{operation_id}/resume",
                    &Method::POST,
                    Some(computer),
                ) => Ok(Self::ResumeUpdate {
                    computer,
                    operation,
                }),
                (
                    "/computers/{profile}/{id}/maintenance/{operation_id}",
                    &Method::GET,
                    Some(computer),
                ) => Ok(Self::MaintenanceReceipt {
                    computer,
                    operation,
                }),
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
            ("/computers/{profile}/{id}/files", &Method::POST, Some(id)) => {
                Ok(Self::TransferFile(id))
            }
            ("/computers/{profile}/{id}/maintenance", &Method::GET, Some(id)) => {
                Ok(Self::Maintenance(id))
            }
            ("/computers/{profile}/{id}/update-template", &Method::POST, Some(id)) => {
                Ok(Self::UpdateTemplate(id))
            }
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
            Self::TransferFile(id) => format!("computers/{id}/files"),
            Self::File {
                computer,
                operation,
            } => format!("computers/{computer}/files/{operation}"),
            Self::CancelFile {
                computer,
                operation,
            } => format!("computers/{computer}/files/{operation}/cancel"),
            Self::Maintenance(id) => format!("computers/{id}/maintenance"),
            Self::MaintenanceReceipt {
                computer,
                operation,
            } => format!("computers/{computer}/maintenance/{operation}"),
            Self::UpdateTemplate(id) => format!("computers/{id}/update-template"),
            Self::ResumeUpdate {
                computer,
                operation,
            } => format!("computers/{computer}/maintenance/{operation}/resume"),
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
    pub fn authorization(self) -> (PolicyTarget, &'static [RouteAction]) {
        let server = ServerSlug::new("computers").expect("static server");
        let tool = match self {
            Self::Create => Some("create"),
            Self::Start(_) => Some("start"),
            Self::Stop(_) => Some("stop"),
            Self::TransferFile(_) => Some("transfer_file"),
            Self::UpdateTemplate(_) => Some("update_template"),
            Self::ResumeUpdate { .. } => Some("resume_update"),
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
                &[RouteAction::Kernel(GatewayAction::ToolsCall)],
            );
        }
        let uri = match self {
            Self::Automation(id) => ComputerResource::Automation(id).to_uri(),
            Self::AutomationGrant { computer, grant } => {
                veoveo_computers_contract::automation_grant_uri(computer, grant)
            }
            Self::Read(id)
            | Self::File { computer: id, .. }
            | Self::CancelFile { computer: id, .. }
            | Self::Maintenance(id)
            | Self::MaintenanceReceipt { computer: id, .. }
            | Self::Access(id)
            | Self::RevokeAccess { computer: id, .. }
            | Self::Pairing(id)
            | Self::ConfirmPairing { computer: id, .. }
            | Self::Ticket(id)
            | Self::Terminal(id)
            | Self::Receipt { computer: id, .. } => veoveo_computers_contract::computer_uri(id),
            _ => ComputerResource::Collection(None).to_uri(),
        };
        let actions: &'static [_] = if self.is_attachment() {
            &[
                RouteAction::Owner(veoveo_computers_contract::ComputerAction::Attach),
                RouteAction::Kernel(GatewayAction::ResourcesRead),
            ]
        } else {
            &[RouteAction::Kernel(GatewayAction::ResourcesRead)]
        };
        (PolicyTarget::Resource { server, uri }, actions)
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
                | Self::TransferFile(_)
                | Self::CancelFile { .. }
                | Self::UpdateTemplate(_)
                | Self::ResumeUpdate { .. }
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
                | Self::File { .. }
                | Self::CancelFile { .. }
                | Self::Receipt { .. }
                | Self::Maintenance(_)
                | Self::MaintenanceReceipt { .. }
                | Self::Access(_)
                | Self::Automation(_)
                | Self::AutomationGrant { .. }
                | Self::RevokeAccess { .. }
        )
    }
}
