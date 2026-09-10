//! Guest transport trust must not confer the worker's provider user authority.
pub async fn assert_denied(root: &std::path::Path, endpoint: &str) {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    use veoveo_computers_runtime::protocol::v1 as api;
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(Certificate::from_pem(
            std::fs::read(root.join("ca.pem")).unwrap(),
        ))
        .identity(Identity::from_pem(
            std::fs::read(root.join("guest.pem")).unwrap(),
            std::fs::read(root.join("guest-key.pem")).unwrap(),
        ));
    let channel = Endpoint::from_shared(format!("https://{endpoint}"))
        .unwrap()
        .tls_config(tls)
        .unwrap()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(5))
        .connect()
        .await
        .expect("guest certificate reaches its trusted TLS endpoint");
    let result = api::open_shell_client::OpenShellClient::new(channel)
        .list_sandboxes(api::ListSandboxesRequest::default())
        .await;
    assert!(
        matches!(result, Err(status) if status.code() == tonic::Code::Unauthenticated),
        "guest transport certificate became a provider user without a sandbox JWT"
    );
}
