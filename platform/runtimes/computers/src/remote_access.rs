//! Narrow provider adapter used only after Computers has authorized an exact binding.
use crate::{
    Binding, Observation, OpenShellRuntime, Phase, Result, RuntimeFailure, client::request,
    protocol::v1 as api,
};
use futures::Stream;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tonic::{Request, Response, Streaming};
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct OpenShellAccess {
    runtime: OpenShellRuntime,
    binding: Binding,
    sandbox_id: String,
    main_process_id: String,
    expires_at: SystemTime,
}

impl OpenShellRuntime {
    pub async fn open_shell_access(
        &self,
        binding: &Binding,
        expires_at: SystemTime,
    ) -> Result<OpenShellAccess> {
        remaining(expires_at)?;
        let observed = self.get(binding).await?.ok_or(RuntimeFailure::NotFound)?;
        if observed.phase != Phase::Ready || observed.main_process_instance_id.is_empty() {
            return Err(RuntimeFailure::InvalidState);
        }
        Ok(OpenShellAccess {
            runtime: self.clone(),
            binding: binding.clone(),
            sandbox_id: observed.sandbox_id,
            main_process_id: observed.main_process_instance_id,
            expires_at,
        })
    }
}

impl OpenShellAccess {
    pub fn sandbox_name(&self) -> String {
        self.binding.name()
    }

    pub fn workspace(&self) -> &str {
        &self.runtime.workspace
    }

    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }

    pub fn main_process_id(&self) -> &str {
        &self.main_process_id
    }

    async fn current_sandbox(&self) -> Result<api::SandboxResponse> {
        remaining(self.expires_at)?;
        let response = self
            .runtime
            .client
            .clone()
            .get_sandbox(request(
                api::GetSandboxRequest {
                    name: self.binding.name(),
                    workspace: self.runtime.workspace.clone(),
                },
                10,
            ))
            .await
            .map_err(|_| RuntimeFailure::Unavailable)?
            .into_inner();
        let observed = Observation::checked(
            response
                .sandbox
                .clone()
                .ok_or(RuntimeFailure::BindingMismatch)?,
            &self.binding,
            &self.runtime.workspace,
        )?;
        if observed.phase != Phase::Ready
            || observed.sandbox_id != self.sandbox_id
            || observed.main_process_instance_id != self.main_process_id
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        Ok(response)
    }

    pub async fn health(&self) -> Result<api::HealthResponse> {
        self.current_sandbox().await?;
        self.runtime
            .client
            .clone()
            .health(request(api::HealthRequest {}, 10))
            .await
            .map(Response::into_inner)
            .map_err(|_| RuntimeFailure::Unavailable)
    }

    pub async fn gateway_info(&self) -> Result<api::GetGatewayInfoResponse> {
        self.current_sandbox().await?;
        let mut response = self
            .runtime
            .client
            .clone()
            .get_gateway_info(request(api::GetGatewayInfoRequest {}, 10))
            .await
            .map(Response::into_inner)
            .map_err(|_| RuntimeFailure::Unavailable)?;
        response.compute_drivers.clear();
        Ok(response)
    }

    pub async fn get_sandbox(
        &self,
        requested_name: &str,
        requested_workspace: &str,
    ) -> Result<api::SandboxResponse> {
        if requested_name != self.binding.name() || requested_workspace != self.runtime.workspace {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let mut response = self.current_sandbox().await?;
        let sandbox = response
            .sandbox
            .as_mut()
            .ok_or(RuntimeFailure::BindingMismatch)?;
        let metadata = sandbox
            .metadata
            .as_mut()
            .ok_or(RuntimeFailure::BindingMismatch)?;
        metadata.created_at_ms = 0;
        metadata.labels.clear();
        metadata.resource_version = 0;
        metadata.annotations.clear();
        metadata.deletion_timestamp_ms = 0;
        sandbox.spec = None;
        sandbox.status = None;
        Ok(response)
    }

    pub async fn create_ssh_session(
        &self,
        requested_sandbox_id: &str,
    ) -> Result<api::CreateSshSessionResponse> {
        self.current_sandbox().await?;
        if requested_sandbox_id != self.sandbox_id {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let mut response = self
            .runtime
            .client
            .clone()
            .create_ssh_session(request(
                api::CreateSshSessionRequest {
                    sandbox_id: self.sandbox_id.clone(),
                },
                10,
            ))
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?
            .into_inner();
        let token = Zeroizing::new(std::mem::take(&mut response.token));
        let access_expires_at_ms =
            system_time_ms(self.expires_at).ok_or(RuntimeFailure::TerminalFailed)?;
        let accepted = response.sandbox_id == self.sandbox_id
            && valid_token(&token)
            && response.expires_at_ms > now_ms();
        if !accepted {
            let _ = self.revoke_inner(token).await;
            return Err(RuntimeFailure::TerminalFailed);
        }
        response.expires_at_ms = response.expires_at_ms.min(access_expires_at_ms);
        response.token = token.to_string();
        // A bearer-mode OpenShell CLI uses the registered external gateway URL.
        // Keep the private provider listener out of the public response anyway.
        response.gateway_host = "localhost".to_owned();
        response.gateway_port = 443;
        response.gateway_scheme = "https".to_owned();
        Ok(response)
    }

    pub async fn revoke_ssh_session(
        &self,
        token: Zeroizing<String>,
    ) -> Result<api::RevokeSshSessionResponse> {
        self.current_sandbox().await?;
        self.revoke_inner(token).await
    }

    async fn revoke_inner(
        &self,
        token: Zeroizing<String>,
    ) -> Result<api::RevokeSshSessionResponse> {
        self.runtime
            .client
            .clone()
            .revoke_ssh_session(request(
                api::RevokeSshSessionRequest {
                    token: token.to_string(),
                },
                5,
            ))
            .await
            .map(Response::into_inner)
            .map_err(|_| RuntimeFailure::TerminalFailed)
    }

    pub async fn forward_tcp<S>(&self, stream: S) -> Result<Streaming<api::TcpForwardFrame>>
    where
        S: Stream<Item = api::TcpForwardFrame> + Send + 'static,
    {
        self.current_sandbox().await?;
        let mut request = Request::new(stream);
        request.set_timeout(remaining(self.expires_at)?);
        self.runtime
            .client
            .clone()
            .forward_tcp(request)
            .await
            .map(Response::into_inner)
            .map_err(|_| RuntimeFailure::TerminalFailed)
    }
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 4096
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._~+/=-".contains(&byte))
}

fn now_ms() -> i64 {
    system_time_ms(SystemTime::now()).unwrap_or(i64::MAX)
}

fn system_time_ms(value: SystemTime) -> Option<i64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

fn remaining(expires_at: SystemTime) -> Result<Duration> {
    expires_at
        .duration_since(SystemTime::now())
        .ok()
        .filter(|duration| !duration.is_zero() && *duration <= Duration::from_secs(900))
        .ok_or(RuntimeFailure::LeaseExpired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_tokens_use_the_upstream_wire_charset_and_bound() {
        assert!(valid_token("abc.DEF_012-~+/="));
        assert!(!valid_token(""));
        assert!(!valid_token("space is rejected"));
        assert!(!valid_token(&"a".repeat(4097)));
    }

    #[test]
    fn access_lifetime_never_exceeds_fifteen_minutes() {
        assert!(remaining(SystemTime::now() + Duration::from_secs(30)).is_ok());
        assert!(remaining(SystemTime::now() + Duration::from_secs(901)).is_err());
        assert!(remaining(SystemTime::now() - Duration::from_secs(1)).is_err());
    }
}
