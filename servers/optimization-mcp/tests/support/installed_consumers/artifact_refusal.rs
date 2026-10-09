//! Loopback HTTP status control reaches the production Artifact client/adapter.
use anyhow::{Result, ensure};
use std::{collections::BTreeSet, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use veoveo_artifact_contract::ArtifactId;
use veoveo_mcp_contract::{PlaneCaller, internal_auth::GatewayInternalIdentity};
use veoveo_optimization_mcp::artifacts::ArtifactRepository;

#[tokio::test]
async fn artifact_get_hides_typed_denial_but_preserves_operational_failures() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let now = chrono::Utc::now();
        let caller = PlaneCaller {
            bearer_token: "native-adapter-status-control".into(),
            identity: GatewayInternalIdentity {
                issuer: "veoveo-internal".parse()?, profile: "operator".parse()?, server: "optimization".parse()?,
                actor: veoveo_mcp_contract::hosting::testing::principal(),
                authority: veoveo_mcp_contract::hosting::testing::authority(),
                // No authentication is performed by this loopback status fixture.
                request_context: None, jwt_id: uuid::Uuid::new_v4().to_string().parse()?,
                issued_at: now, not_before: now, expires_at: now + chrono::TimeDelta::minutes(1),
            }, memberships: BTreeSet::new(),
        };
        for (status, body, absent) in [("403 Forbidden", "denied", true), ("404 Not Found", "missing", true),
            ("401 Unauthorized", "unauthenticated", false), ("400 Bad Request", "invalid", false),
            ("500 Internal Server Error", "backend", false), ("200 OK", "malformed-object", false)] {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await?;
                let mut request = Vec::new(); let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buffer).await?;
                    ensure!(n > 0 && request.len() + n <= 8192, "invalid native HTTP request");
                    request.extend_from_slice(&buffer[..n]);
                }
                ensure!(request.starts_with(b"GET /artifacts/"), "actual Artifact GET was not reached");
                let response = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}", body.len());
                stream.write_all(response.as_bytes()).await?;
                Ok::<_, anyhow::Error>(())
            });
            let client = reqwest::Client::builder().timeout(Duration::from_secs(1)).build()?;
            let repository = ArtifactRepository::with_client(format!("http://{address}"), client);
            let result = repository.get(&caller, &ArtifactId::new()).await;
            server.await??;
            if absent {ensure!(matches!(result, Ok(None)), "denial/absence escaped non-disclosure adapter");}
            else {ensure!(result.is_err(), "operational failure collapsed to absence");}
        }
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?; drop(listener);
        let client = reqwest::Client::builder().timeout(Duration::from_secs(1)).build()?;
        let repository = ArtifactRepository::with_client(format!("http://{address}"), client);
        ensure!(repository.get(&caller, &ArtifactId::new()).await.is_err(), "transport failure collapsed to absence");
        Ok(())
    }).await?
}
