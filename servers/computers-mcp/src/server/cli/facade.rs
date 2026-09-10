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
    pub computer: uuid::Uuid,
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
        if request.name != self.computer.to_string() || request.workspace != "default" {
            return Err(Status::permission_denied("Computer target denied"));
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
        metadata.name = self.computer.to_string();
        metadata.workspace = "default".into();
        Ok(Response::new(response))
    }
    async fn create_ssh_session(
        &self,
        request: Request<api::CreateSshSessionRequest>,
    ) -> Result<Response<api::CreateSshSessionResponse>, Status> {
        self.access
            .create_ssh_session(&request.into_inner().sandbox_id)
            .await
            .map(Response::new)
            .map_err(error)
    }
    async fn forward_tcp(
        &self,
        request: Request<tonic::Streaming<api::TcpForwardFrame>>,
    ) -> Result<Response<Output>, Status> {
        let activity = self.activity.clone();
        let input = request.into_inner().take_while(|result| future::ready(result.is_ok())).map(move |result| {
            let frame = result.expect("successful stream item");
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
        RuntimeFailure::BindingMismatch => Status::permission_denied("Computer target denied"),
        RuntimeFailure::NotFound | RuntimeFailure::InvalidState => {
            Status::failed_precondition("Computer is not connectable")
        }
        _ => Status::unavailable("Computer connection interrupted"),
    }
}
