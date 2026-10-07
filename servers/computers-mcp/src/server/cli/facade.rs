use super::Activity;
use futures::{StreamExt, future};
use std::{pin::Pin, sync::Arc};
use tonic::{Request, Response, Status};
use veoveo_computers_runtime::{
    OpenShellAccess, RuntimeFailure,
    protocol::v1::{self as api, open_shell_server::OpenShell},
};

#[derive(Clone)]
pub(super) struct Restricted {
    pub access: OpenShellAccess,
    pub computer: veoveo_computers_contract::ComputerId,
    pub activity: Arc<Activity>,
}
type Output = Pin<Box<dyn futures::Stream<Item = Result<api::TcpForwardFrame, Status>> + Send>>;

#[tonic::async_trait]
impl OpenShell for Restricted {
    async fn health(
        &self,
        _: Request<api::HealthRequest>,
    ) -> Result<Response<api::HealthResponse>, Status> {
        self.access.health().await.map(Response::new).map_err(error)
    }
    async fn get_gateway_info(
        &self,
        _: Request<api::GetGatewayInfoRequest>,
    ) -> Result<Response<api::GetGatewayInfoResponse>, Status> {
        self.access
            .gateway_info()
            .await
            .map(Response::new)
            .map_err(error)
    }
    async fn get_sandbox(
        &self,
        request: Request<api::GetSandboxRequest>,
    ) -> Result<Response<api::SandboxResponse>, Status> {
        let request = request.into_inner();
        if !admitted_selector(
            self.computer,
            &request.name,
            request.workspace_scope.as_ref(),
        ) {
            return Err(Status::permission_denied(
                "You don't have permission to access this Computer.",
            ));
        }
        let mut response = self
            .access
            .get_sandbox(&self.access.sandbox_name(), self.access.workspace())
            .await
            .map_err(error)?;
        let metadata = response
            .sandbox
            .as_mut()
            .and_then(|s| s.metadata.as_mut())
            .ok_or_else(|| Status::unavailable("Computer connection interrupted"))?;
        metadata.id = self.computer.to_string();
        metadata.name = self.computer.to_string();
        metadata.workspace = "default".into();
        Ok(Response::new(response))
    }
    async fn create_ssh_session(
        &self,
        request: Request<api::CreateSshSessionRequest>,
    ) -> Result<Response<api::CreateSshSessionResponse>, Status> {
        let request = request.into_inner();
        if !admitted_selector(
            self.computer,
            &request.sandbox,
            request.workspace_scope.as_ref(),
        ) {
            return Err(Status::permission_denied(
                "You don't have permission to access this Computer.",
            ));
        }
        let mut response = self
            .access
            .create_ssh_session(self.access.sandbox_id())
            .await
            .map_err(error)?;
        response.sandbox_id = self.computer.to_string();
        Ok(Response::new(response))
    }
    async fn forward_tcp(
        &self,
        request: Request<tonic::Streaming<api::TcpForwardFrame>>,
    ) -> Result<Response<Output>, Status> {
        let activity = self.activity.clone();
        let computer = self.computer;
        let sandbox_name = self.access.sandbox_name();
        let sandbox_id = self.access.sandbox_id().to_owned();
        let workspace = self.access.workspace().to_owned();
        let input = request.into_inner().take_while(|result| future::ready(result.is_ok())).map(move |result| {
            let mut frame = result.expect("successful stream item");
            if let Some(api::tcp_forward_frame::Payload::Init(header)) = &mut frame.payload {
                if !translate_forward_header(header, computer, &sandbox_name, &sandbox_id, &workspace) {
                    frame.payload = None;
                }
            }
            if matches!(&frame.payload, Some(api::tcp_forward_frame::Payload::Data(bytes)) if !bytes.is_empty() && bytes.len() <= 65536) {
                // SSH is encrypted. Its payload includes user bytes and may include
                // SSH-level keepalives. WebSocket/HTTP2 keepalives never reach here.
                activity.record();
            }
            frame
        });
        let output = self.access.forward_tcp(input).await.map_err(error)?;
        Ok(Response::new(Box::pin(output)))
    }
}
fn error(error: RuntimeFailure) -> Status {
    match error {
        RuntimeFailure::LeaseExpired => Status::unauthenticated("Computer access ended"),
        RuntimeFailure::BindingMismatch => {
            Status::permission_denied("You don't have permission to access this Computer.")
        }
        RuntimeFailure::NotFound | RuntimeFailure::InvalidState => {
            Status::failed_precondition("Computer is not connectable")
        }
        _ => Status::unavailable("Computer connection interrupted"),
    }
}

fn default_workspace(
    scope: Option<&veoveo_computers_runtime::protocol::datamodel::v1::WorkspaceSelector>,
) -> bool {
    matches!(scope.and_then(|scope| scope.selection.as_ref()), Some(veoveo_computers_runtime::protocol::datamodel::v1::workspace_selector::Selection::Workspace(workspace)) if workspace == "default")
}

fn admitted_selector(
    computer: veoveo_computers_contract::ComputerId,
    name: &str,
    scope: Option<&veoveo_computers_runtime::protocol::datamodel::v1::WorkspaceSelector>,
) -> bool {
    name.parse::<veoveo_computers_contract::ComputerId>().ok() == Some(computer)
        && default_workspace(scope)
}
fn translate_forward_header(
    header: &mut api::TcpForwardInit,
    computer: veoveo_computers_contract::ComputerId,
    sandbox_name: &str,
    sandbox_id: &str,
    workspace: &str,
) -> bool {
    if header
        .sandbox
        .parse::<veoveo_computers_contract::ComputerId>()
        .ok()
        != Some(computer)
        || header.workspace != "default"
        || header.service_id != format!("ssh-proxy:{computer}")
        || !matches!(header.target, Some(api::tcp_forward_init::Target::Ssh(_)))
    {
        return false;
    }
    header.sandbox = sandbox_name.into();
    header.workspace = workspace.into();
    header.service_id = format!("ssh-proxy:{sandbox_id}");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_computers_runtime::protocol::datamodel::v1::{
        AllWorkspaces, WorkspaceSelector, workspace_selector::Selection,
    };
    fn computer() -> veoveo_computers_contract::ComputerId {
        "00000000-0000-7000-8000-000000000064".parse().unwrap()
    }
    fn workspace(value: &str) -> WorkspaceSelector {
        WorkspaceSelector {
            selection: Some(Selection::Workspace(value.into())),
        }
    }
    #[test]
    fn private_cli_selectors_admit_only_attached_public_computer_and_default_workspace() {
        let scope = workspace("default");
        assert!(admitted_selector(
            computer(),
            &computer().to_string(),
            Some(&scope)
        ));
        for name in [
            "private-provider-name",
            "invalid",
            "00000000-0000-7000-8000-000000000065",
        ] {
            assert!(!admitted_selector(computer(), name, Some(&scope)));
        }
        for scope in [
            None,
            Some(workspace("")),
            Some(workspace("other-tenant")),
            Some(WorkspaceSelector { selection: None }),
            Some(WorkspaceSelector {
                selection: Some(Selection::AllWorkspaces(AllWorkspaces {})),
            }),
        ] {
            assert!(!admitted_selector(
                computer(),
                &computer().to_string(),
                scope.as_ref()
            ));
        }
    }
    #[test]
    fn cli_tunnel_translation_uses_checked_private_binding_and_preserves_opaque_token() {
        let initial = api::TcpForwardInit {
            sandbox: computer().to_string(),
            workspace: "default".into(),
            service_id: format!("ssh-proxy:{}", computer()),
            target: Some(api::tcp_forward_init::Target::Ssh(api::SshRelayTarget {})),
            authorization_token: "opaque-token".into(),
        };
        let mut header = initial.clone();
        assert!(translate_forward_header(
            &mut header,
            computer(),
            "bound-provider-name",
            "bound-resource-id",
            "bound-workspace"
        ));
        assert!(
            header.sandbox == "bound-provider-name"
                && header.workspace == "bound-workspace"
                && header.service_id == "ssh-proxy:bound-resource-id"
        );
        assert!(
            header.authorization_token == "opaque-token"
                && matches!(header.target, Some(api::tcp_forward_init::Target::Ssh(_)))
        );
        for fault in 0..5 {
            let mut invalid = initial.clone();
            match fault {
                0 => invalid.sandbox = "00000000-0000-7000-8000-000000000065".into(),
                1 => invalid.sandbox = "bound-provider-name".into(),
                2 => invalid.workspace = "bound-workspace".into(),
                3 => invalid.service_id = "ssh-proxy:bound-resource-id".into(),
                4 => invalid.target = None,
                _ => unreachable!(),
            }
            let original = invalid.clone();
            assert!(!translate_forward_header(
                &mut invalid,
                computer(),
                "bound-provider-name",
                "bound-resource-id",
                "bound-workspace"
            ));
            assert!(
                invalid == original,
                "rejected selector cannot become a private provider request"
            );
        }
    }
}
