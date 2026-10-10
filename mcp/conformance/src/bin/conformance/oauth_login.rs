//! Interactive public-client login through the maintained SDK OAuth state machine.
use anyhow::{Result, ensure};
use axum::{
    Router,
    extract::{OriginalUri, State},
    response::Redirect,
    routing::get,
};
use oauth2::{TokenResponse, basic::BasicTokenType};
use rmcp::transport::auth::{AuthorizationManager, AuthorizationRequest, AuthorizationSession};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    net::{IpAddr, SocketAddr},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::Duration,
};
use tokio::{
    sync::{Mutex, oneshot},
    task::JoinHandle,
    time::Instant,
};
use tracing::instrument::WithSubscriber;
use url::Url;
use veoveo_types::{HttpsUrl, OAuthClientId, ScopeName};

#[derive(clap::Args)]
pub(super) struct LoginArgs {
    #[arg(long)]
    pub(super) issuer: String,
    #[arg(long)]
    pub(super) client_id: String,
    #[arg(long)]
    pub(super) resource: String,
    #[arg(long)]
    pub(super) redirect_uri: String,
    #[arg(long, required = true)]
    pub(super) scope: Vec<ScopeName>,
    #[arg(long)]
    pub(super) authorization_url_file: PathBuf,
    #[arg(long)]
    pub(super) token_file: PathBuf,
    #[arg(long, default_value_t = 180, value_parser = clap::value_parser!(u64).range(30..=900))]
    pub(super) timeout_seconds: u64,
}
struct Admission {
    issuer: Url,
    resource: Url,
    callback: Url,
    address: SocketAddr,
    client: OAuthClientId,
    scopes: BTreeSet<ScopeName>,
}
fn remote(value: &str, loopback: bool) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("OAuth URL admission failed"))?;
    let local = loopback
        && url.scheme() == "http"
        && url
            .host_str()
            .and_then(|host| host.parse::<IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
    ensure!(
        local || HttpsUrl::parse(value).is_ok(),
        "OAuth requires HTTPS issuer and resource"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "OAuth URL admission failed"
    );
    Ok(url)
}
fn admit(args: &LoginArgs, loopback: bool) -> Result<Admission> {
    ensure!(
        (30..=900).contains(&args.timeout_seconds),
        "OAuth timeout must be30..900seconds"
    );
    let issuer = remote(&args.issuer, loopback)?;
    let resource = remote(&args.resource, loopback)?;
    let callback = Url::parse(&args.redirect_uri)
        .map_err(|_| anyhow::anyhow!("OAuth callback admission failed"))?;
    let ip = callback
        .host_str()
        .and_then(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().ok())
        .filter(IpAddr::is_loopback)
        .ok_or_else(|| anyhow::anyhow!("OAuth callback requires numeric loopback"))?;
    ensure!(
        callback.scheme() == "http"
            && callback.username().is_empty()
            && callback.password().is_none()
            && callback.query().is_none()
            && callback.fragment().is_none()
            && callback.port().is_some_and(|port| port != 0)
            && callback.path() != "/"
            && callback.path() != "/done"
            && callback
                .path()
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '_' | '.'))
            && !callback.path().contains("//"),
        "OAuth callback admission failed"
    );
    let client = OAuthClientId::parse(&args.client_id)
        .map_err(|_| anyhow::anyhow!("OAuth client admission failed"))?;
    let scopes: BTreeSet<_> = args.scope.iter().cloned().collect();
    ensure!(
        !scopes.is_empty() && scopes.len() == args.scope.len(),
        "OAuth scopes must be nonempty and unique"
    );
    ensure!(
        args.authorization_url_file.is_absolute()
            && args.token_file.is_absolute()
            && args.authorization_url_file != args.token_file,
        "OAuth requires distinct absolute private output paths"
    );
    Ok(Admission {
        address: SocketAddr::new(ip, callback.port().unwrap()),
        issuer,
        resource,
        callback,
        client,
        scopes,
    })
}
fn private_file(path: &PathBuf) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| anyhow::anyhow!("OAuth requires new private output files"))
}
fn scope_set(value: &str) -> Result<BTreeSet<ScopeName>> {
    value
        .split_ascii_whitespace()
        .map(|scope| {
            ScopeName::parse(scope)
                .map_err(|_| anyhow::anyhow!("OAuth scope response admission failed"))
        })
        .collect()
}
fn admit_authorization(
    session: &AuthorizationSession,
    admission: &Admission,
    loopback: bool,
) -> Result<()> {
    let url = Url::parse(session.get_authorization_url())
        .map_err(|_| anyhow::anyhow!("OAuth authorization request admission failed"))?;
    let mut endpoint = url.clone();
    endpoint.set_query(None);
    remote(endpoint.as_str(), loopback)?;
    let pairs: Vec<_> = url.query_pairs().collect();
    for name in [
        "scope",
        "resource",
        "client_id",
        "redirect_uri",
        "response_type",
        "code_challenge_method",
        "state",
        "code_challenge",
    ] {
        ensure!(
            pairs.iter().filter(|(key, _)| key == name).count() == 1,
            "OAuth authorization request admission failed"
        );
    }
    let field = |name: &str| {
        pairs
            .iter()
            .find(|(key, _)| key == name)
            .unwrap()
            .1
            .as_ref()
    };
    ensure!(
        scope_set(field("scope"))? == admission.scopes,
        "OAuth SDK requested unexpected scopes"
    );
    ensure!(
        field("resource") == admission.resource.as_str()
            && field("client_id") == admission.client.as_str()
            && field("redirect_uri") == admission.callback.as_str()
            && field("response_type") == "code"
            && field("code_challenge_method") == "S256"
            && !field("state").is_empty()
            && !field("code_challenge").is_empty(),
        "OAuth authorization request admission failed"
    );
    Ok(())
}
#[derive(Serialize)]
struct Success<'a> {
    status: Outcome,
    expires_in_seconds: u64,
    scopes: &'a BTreeSet<ScopeName>,
    authorization_url_file: &'a PathBuf,
    token_file: &'a PathBuf,
}
struct ServerTask {
    task: Option<JoinHandle<std::io::Result<()>>>,
    handle: axum_server::Handle<SocketAddr>,
}
impl ServerTask {
    async fn close(&mut self, deadline: Instant) -> Result<()> {
        ensure!(
            Instant::now() < deadline,
            "OAuth callback cleanup deadline expired"
        );
        let remaining = deadline.saturating_duration_since(Instant::now());
        let grace = remaining.min(Duration::from_secs(1));
        self.handle.graceful_shutdown(Some(grace));
        let forced_at = Instant::now() + grace.min(remaining / 2);
        let task = self.task.as_mut().expect("owned OAuth callback task");
        let mut joined = tokio::time::timeout_at(forced_at, &mut *task).await.ok();
        if joined.is_none() || self.handle.connection_count() != 0 {
            self.handle.shutdown();
        }
        if joined.is_none() {
            joined = tokio::time::timeout_at(deadline, &mut *task).await.ok();
        }
        let Some(joined) = joined else {
            self.handle.shutdown();
            task.abort();
            // No new cleanup tail after the original cap; this outcome stays
            // unqualified, and Drop also signals every retained connection.
            anyhow::bail!("OAuth callback cleanup deadline expired");
        };
        self.task.take();
        ensure!(
            matches!(joined, Ok(Ok(()))),
            "OAuth callback cleanup failed"
        );
        tokio::time::timeout_at(deadline, async {
            while self.handle.connection_count() != 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("OAuth callback connection cleanup deadline expired"))?;
        ensure!(
            Instant::now() < deadline,
            "OAuth callback connection cleanup deadline expired"
        );
        Ok(())
    }
}
impl Drop for ServerTask {
    fn drop(&mut self) {
        // The maintained handle signals every accepted connection as well as
        // the listener. Aborting only the listener would orphan connections.
        self.handle.shutdown();
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
async fn callback(
    State(state): State<std::sync::Arc<Callback>>,
    OriginalUri(uri): OriginalUri,
) -> Redirect {
    if let Some(sender) = state.sender.lock().await.take() {
        let mut url = state.url.clone();
        url.set_query(uri.query());
        let _ = sender.send(url);
    }
    Redirect::to("/done")
}
struct Callback {
    url: Url,
    sender: Mutex<Option<oneshot::Sender<Url>>>,
}

pub(super) async fn run(args: &LoginArgs) -> Result<()> {
    run_profile(args, false).await
}
#[cfg(test)]
async fn run_loopback(args: &LoginArgs) -> Result<()> {
    run_profile(args, true).await
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Outcome {
    Authorized,
    Failed,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Phase {
    Admission,
    OutputAdmission,
    CallbackBind,
    Discovery,
    Authorization,
    Callback,
    Token,
    Cleanup,
    Deadline,
}
fn set_phase(phase: &std::sync::Mutex<Phase>, value: Phase) {
    *phase.lock().expect("OAuth phase lock") = value;
}
async fn run_profile(args: &LoginArgs, loopback: bool) -> Result<()> {
    let phase = std::sync::Mutex::new(Phase::Admission);
    let result = run_inner(args, loopback, &phase)
        .with_subscriber(tracing::subscriber::NoSubscriber::default())
        .await;
    if result.is_err() {
        #[derive(Serialize)]
        struct Failure {
            status: Outcome,
            phase: Phase,
        }
        let report = Failure {
            status: Outcome::Failed,
            phase: *phase.lock().expect("OAuth phase lock"),
        };
        println!(
            "{}",
            serde_json::to_string(&report).expect("fixed OAuth failure serialization")
        );
    }
    result
}
async fn run_inner(
    args: &LoginArgs,
    loopback: bool,
    phase: &std::sync::Mutex<Phase>,
) -> Result<()> {
    let admission = admit(args, loopback)?;
    let deadline = Instant::now() + Duration::from_secs(args.timeout_seconds);
    // Keep cleanup inside the original total deadline, including error paths.
    let operation_deadline = deadline - Duration::from_secs(2);
    set_phase(phase, Phase::OutputAdmission);
    let mut authorization_file = private_file(&args.authorization_url_file)?;
    let mut token_file = private_file(&args.token_file)?;
    set_phase(phase, Phase::CallbackBind);
    let listener = tokio::net::TcpListener::bind(admission.address)
        .await
        .map_err(|_| anyhow::anyhow!("OAuth callback bind failed"))?;
    let (received, callback_receiver) = oneshot::channel();
    let state = std::sync::Arc::new(Callback {
        url: admission.callback.clone(),
        sender: Mutex::new(Some(received)),
    });
    let router = Router::new()
        .route(admission.callback.path(), get(callback))
        .route(
            "/done",
            get(|| async { "Authorization callback received. Return to the CLI." }),
        )
        .with_state(state);
    let handle = axum_server::Handle::new();
    let callback_server = axum_server::from_tcp(
        listener
            .into_std()
            .map_err(|_| anyhow::anyhow!("OAuth callback listener setup failed"))?,
    )
    .map_err(|_| anyhow::anyhow!("OAuth callback listener setup failed"))?
    .handle(handle.clone());
    let mut server = ServerTask {
        task: Some(tokio::spawn(async move {
            callback_server.serve(router.into_make_service()).await
        })),
        handle,
    };
    let result = tokio::time::timeout_at(operation_deadline, async {
        set_phase(phase, Phase::Discovery);
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| anyhow::anyhow!("OAuth HTTP client setup failed"))?;
        let mut manager = AuthorizationManager::new(admission.resource.as_str())
            .await
            .map_err(|_| anyhow::anyhow!("OAuth manager setup failed"))?;
        manager
            .with_client(http)
            .map_err(|_| anyhow::anyhow!("OAuth HTTP client setup failed"))?;
        let resolved = manager
            .resolve_metadata()
            .await
            .map_err(|_| anyhow::anyhow!("OAuth metadata discovery failed"))?;
        ensure!(
            resolved.source.is_discovered()
                && resolved.metadata.issuer.as_deref() == Some(admission.issuer.as_str()),
            "OAuth discovered issuer differs from admitted issuer"
        );
        for endpoint in [
            &resolved.metadata.authorization_endpoint,
            &resolved.metadata.token_endpoint,
        ] {
            remote(endpoint, loopback)?;
        }
        manager.set_metadata(resolved.metadata);
        set_phase(phase, Phase::Authorization);
        let request = AuthorizationRequest::new(admission.callback.as_str())
            .with_preregistered_client(admission.client.as_str())
            .with_scopes(admission.scopes.iter().map(|scope| scope.as_str()));
        let session = AuthorizationSession::new(manager, request)
            .await
            .map_err(|_| anyhow::anyhow!("OAuth authorization session failed"))?;
        admit_authorization(&session, &admission, loopback)?;
        authorization_file
            .write_all(session.get_authorization_url().as_bytes())
            .and_then(|_| authorization_file.write_all(b"\n"))
            .and_then(|_| authorization_file.sync_all())
            .map_err(|_| anyhow::anyhow!("OAuth authorization output failed"))?;
        set_phase(phase, Phase::Callback);
        let returned = callback_receiver
            .await
            .map_err(|_| anyhow::anyhow!("OAuth callback listener failed"))?;
        let mut keys = BTreeSet::new();
        ensure!(
            returned
                .query_pairs()
                .all(|(key, _)| keys.insert(key.into_owned())),
            "OAuth callback contains duplicate fields"
        );
        let token = session
            .handle_callback_url(returned.as_str())
            .await
            .map_err(|_| anyhow::anyhow!("OAuth callback or token exchange failed"))?;
        set_phase(phase, Phase::Token);
        ensure!(
            token.token_type() == &BasicTokenType::Bearer,
            "OAuth token admission failed"
        );
        let granted = session
            .auth_manager
            .get_current_scopes()
            .await
            .into_iter()
            .map(|scope| {
                ScopeName::parse(&scope)
                    .map_err(|_| anyhow::anyhow!("OAuth granted scope admission failed"))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        ensure!(
            granted == admission.scopes,
            "OAuth granted scopes differ from admitted scopes"
        );
        let expiry = token
            .expires_in()
            .filter(|expiry| !expiry.is_zero())
            .ok_or_else(|| anyhow::anyhow!("OAuth token expiry absent"))?
            .as_secs();
        // Secret exposure occurs only at the final admitted private-file write.
        let secret = token.access_token().secret();
        ensure!(
            !secret.is_empty() && !secret.chars().any(char::is_whitespace),
            "OAuth token admission failed"
        );
        token_file
            .write_all(secret.as_bytes())
            .and_then(|_| token_file.write_all(b"\n"))
            .and_then(|_| token_file.sync_all())
            .map_err(|_| anyhow::anyhow!("OAuth token output failed"))?;
        Ok::<_, anyhow::Error>(expiry)
    })
    .await
    .map_err(|_| {
        set_phase(phase, Phase::Deadline);
        anyhow::anyhow!("OAuth login deadline expired")
    })
    .and_then(|result| result);
    let operation_phase = *phase.lock().expect("OAuth phase lock");
    set_phase(phase, Phase::Cleanup);
    server.close(deadline).await?;
    if result.is_err() {
        set_phase(phase, operation_phase);
    }
    let expiry = result?;
    let report = Success {
        status: Outcome::Authorized,
        expires_in_seconds: expiry,
        scopes: &admission.scopes,
        authorization_url_file: &args.authorization_url_file,
        token_file: &args.token_file,
    };
    println!(
        "{}",
        serde_json::to_string(&report)
            .map_err(|_| anyhow::anyhow!("OAuth result output failed"))?
    );
    Ok(())
}
#[cfg(test)]
#[path = "oauth_login/tests.rs"]
mod tests;
