//! Native stock-provider user admission over authenticated client TLS.
use std::{io::Read, path::Path, time::Duration};
use tonic::{
    Code, Request,
    transport::{Certificate, Channel, ClientTlsConfig, Endpoint, Identity},
};
use veoveo_computers_runtime::protocol::v1 as api;

const DEADLINE: Duration = Duration::from_secs(5);

// Tokens deliberately have no Debug or Display implementation.
pub struct ProviderProbeTokens<'a> {
    pub authorized: &'a str,
    pub wrong_signature: &'a str,
    pub wrong_issuer: &'a str,
    pub wrong_audience: &'a str,
    pub expired: &'a str,
    pub unauthorized_roles: &'a str,
}
#[derive(Clone, Copy, Debug)]
enum Scenario {
    GuestCertificateOnly,
    WorkerCertificateOnly,
    WrongSignature,
    WrongIssuer,
    WrongAudience,
    Expired,
    UnauthorizedRoles,
    Authorized,
}
enum ClientRole {
    Worker,
    Supervisor,
}
fn input(root: &Path, name: &str, scenario: Scenario) -> Vec<u8> {
    let file = std::fs::File::open(root.join(name))
        .unwrap_or_else(|_| panic!("scenario={scenario:?} stage=TrustInput"));
    let metadata = file
        .metadata()
        .unwrap_or_else(|_| panic!("scenario={scenario:?} stage=TrustMetadata"));
    assert!(
        metadata.is_file() && metadata.len() <= 65536,
        "scenario={scenario:?} stage=TrustBounds"
    );
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .unwrap_or_else(|_| panic!("scenario={scenario:?} stage=TrustRead"));
    assert!(
        !bytes.is_empty() && bytes.len() <= 65536,
        "scenario={scenario:?} stage=TrustBounds"
    );
    bytes
}
async fn connect(root: &Path, endpoint: &str, role: ClientRole, scenario: Scenario) -> Channel {
    let (certificate, key) = match role {
        ClientRole::Worker => ("client.pem", "client-key.pem"),
        ClientRole::Supervisor => ("guest.pem", "guest-key.pem"),
    };
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(Certificate::from_pem(input(root, "ca.pem", scenario)))
        .identity(Identity::from_pem(
            input(root, certificate, scenario),
            input(root, key, scenario),
        ));
    let endpoint = Endpoint::from_shared(format!("https://{endpoint}"))
        .unwrap_or_else(|_| panic!("scenario={scenario:?} stage=Endpoint"))
        .tls_config(tls)
        .unwrap_or_else(|_| panic!("scenario={scenario:?} stage=TlsConfiguration"))
        .connect_timeout(DEADLINE)
        .timeout(DEADLINE);
    match tokio::time::timeout(DEADLINE, endpoint.connect()).await {
        Ok(Ok(channel)) => channel,
        Ok(Err(_)) => panic!("scenario={scenario:?} stage=TlsConnection"),
        Err(_) => panic!("scenario={scenario:?} stage=TlsDeadline"),
    }
}
async fn observe(channel: Channel, token: Option<&str>, scenario: Scenario) -> Result<(), Code> {
    let mut request = Request::new(api::ListSandboxesRequest {
        workspace_scope: Some(
            veoveo_computers_runtime::protocol::datamodel::v1::WorkspaceSelector {
                selection: Some(
                    veoveo_computers_runtime::protocol::datamodel::v1::workspace_selector::Selection::Workspace(
                        "default".into(),
                    ),
                ),
            },
        ),
        page_size: 1,
        page_token: String::new(),
        label_selector: String::new(),
    });
    request.set_timeout(DEADLINE);
    if let Some(token) = token {
        let mut bearer = format!("Bearer {token}")
            .parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()
            .unwrap_or_else(|_| panic!("scenario={scenario:?} stage=BearerEncoding"));
        bearer.set_sensitive(true);
        request.metadata_mut().insert("authorization", bearer);
    }
    let mut client = api::open_shell_client::OpenShellClient::new(channel);
    match tokio::time::timeout(DEADLINE, client.list_sandboxes(request)).await {
        Ok(Ok(response)) => {
            assert!(
                response_is_bounded(&response.into_inner()),
                "scenario={scenario:?} stage=ResponseBounds"
            );
            Ok(())
        }
        Ok(Err(status)) => {
            eprintln!(
                "scenario={scenario:?} request=ListSandboxes workspace=default page_size=1 code={:?} message={}",
                status.code(),
                safe_message(status.message(), token),
            );
            Err(status.code())
        }
        Err(_) => panic!("scenario={scenario:?} stage=RpcDeadline"),
    }
}
fn response_is_bounded(response: &api::ListSandboxesResponse) -> bool {
    response.sandboxes.len() <= 1
}
fn safe_message(message: &str, token: Option<&str>) -> String {
    let redacted = match token.filter(|token| !token.is_empty()) {
        Some(token) => message.replace(token, "[redacted]"),
        None => message.to_owned(),
    };
    redacted
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect()
}

fn denied(result: Result<(), Code>, expected: Code, scenario: Scenario) {
    assert_eq!(result, Err(expected), "scenario={scenario:?}");
}

/// Retained entry point for the shared guest certificate rejection control.
pub async fn assert_denied(root: &Path, endpoint: &str) {
    let scenario = Scenario::GuestCertificateOnly;
    let channel = connect(root, endpoint, ClientRole::Supervisor, scenario).await;
    denied(
        observe(channel, None, scenario).await,
        Code::Unauthenticated,
        scenario,
    );
}

/// Read-only native probes; each ListSandboxes request has a five-second deadline.
pub async fn assert_provider_security(
    root: &Path,
    endpoint: &str,
    tokens: ProviderProbeTokens<'_>,
) {
    assert_denied(root, endpoint).await;
    let channel = connect(
        root,
        endpoint,
        ClientRole::Worker,
        Scenario::WorkerCertificateOnly,
    )
    .await;
    denied(
        observe(channel.clone(), None, Scenario::WorkerCertificateOnly).await,
        Code::Unauthenticated,
        Scenario::WorkerCertificateOnly,
    );
    // Stock OIDC rejects these token failures before method authorization. A
    // signed identity without either configured role reaches RBAC and is denied.
    for (scenario, token, expected) in [
        (
            Scenario::WrongSignature,
            tokens.wrong_signature,
            Code::Unauthenticated,
        ),
        (
            Scenario::WrongIssuer,
            tokens.wrong_issuer,
            Code::Unauthenticated,
        ),
        (
            Scenario::WrongAudience,
            tokens.wrong_audience,
            Code::Unauthenticated,
        ),
        (Scenario::Expired, tokens.expired, Code::Unauthenticated),
        (
            Scenario::UnauthorizedRoles,
            tokens.unauthorized_roles,
            Code::PermissionDenied,
        ),
    ] {
        denied(
            observe(channel.clone(), Some(token), scenario).await,
            expected,
            scenario,
        );
    }
    assert_eq!(
        observe(channel, Some(tokens.authorized), Scenario::Authorized).await,
        Ok(()),
        "scenario=Authorized"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_probe_refuses_an_oversized_page() {
        let mut response = api::ListSandboxesResponse::default();
        assert!(response_is_bounded(&response));
        response.sandboxes.push(Default::default());
        assert!(response_is_bounded(&response));
        response.sandboxes.push(Default::default());
        assert!(!response_is_bounded(&response));
    }

    #[test]
    fn rpc_diagnostics_redact_credentials_and_bound_untrusted_messages() {
        let token = "private-test-credential";
        let message = format!("invalid bearer {token}\n{}", "x".repeat(300));
        let safe = safe_message(&message, Some(token));
        assert!(!safe.contains(token));
        assert!(!safe.contains('\n'));
        assert!(safe.contains("[redacted]"));
        assert_eq!(safe.len(), 256);
    }
}
