//! Actual stock CLI and native provider through two bounded relay hops. The
//! fixture obtains a credential through actual worker HTTP pairing and confirmation;
//! browser SSO confirmation and the installed public endpoint are separate checks.
use crate::{browser_terminal::Server, cli_edges::Edges, signing::Signing, support};
use chrono::{TimeDelta, Utc};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::atomic::Ordering,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use uuid::Uuid;
use veoveo_computers::{ComputerActor, ComputersStore};
use veoveo_computers_runtime::{DevelopmentTemplate, OpenShellRuntime};

struct Cli {
    child: tokio::process::Child,
    group: u32,
}
impl Drop for Cli {
    fn drop(&mut self) {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.group)])
            .output();
        let _ = self.child.start_kill();
    }
}
fn command(binary: &Path, directory: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .env("XDG_STATE_HOME", directory.join("state"))
        .env("XDG_DATA_HOME", directory.join("data"))
        .env("SSL_CERT_FILE", directory.join("cli-ca.pem"))
        .env("TERM", "xterm-256color")
        .kill_on_drop(true);
    command
}
async fn until(reader: &mut tokio::process::ChildStdout, marker: &str, directory: &Path) {
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut output = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let count = reader.read(&mut buffer).await.unwrap();
            assert!(
                count > 0,
                "stock CLI ended before marker; private diagnostics: {}",
                directory.display()
            );
            output.extend_from_slice(&buffer[..count]);
            assert!(output.len() <= 65536, "bounded stock CLI output");
            if String::from_utf8_lossy(&output).contains(marker) {
                return;
            }
        }
    })
    .await
    .expect("native CLI output deadline");
}
fn write_private(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn revoked_grant_tunnel_url(
    worker_base: &str,
    computer: &veoveo_computers_contract::ComputerId,
) -> reqwest::Url {
    let mut url = reqwest::Url::parse(worker_base).expect("fixture worker base URL");
    assert_eq!(
        url.path_segments()
            .and_then(|mut segments| segments.next_back()),
        Some("admin"),
        "fixture worker base ends in the admin route",
    );
    assert!(url.query().is_none() && url.fragment().is_none());
    let scheme = match url.scheme() {
        "http" => "ws",
        "https" => "wss",
        _ => panic!("fixture worker base uses HTTP or HTTPS"),
    };
    url.set_scheme(scheme).unwrap();
    let computer_segment = computer.to_string();
    url.path_segments_mut()
        .expect("fixture worker base has hierarchical path segments")
        .pop()
        .extend(["cli", "operator", computer_segment.as_str(), "_ws_tunnel"]);
    url
}

pub async fn qualify(
    db: &support::TestDb,
    runtime: OpenShellRuntime,
    template: DevelopmentTemplate,
    computer: veoveo_computers_contract::ComputerId,
) {
    tokio::time::timeout(
        Duration::from_secs(110),
        run(db, runtime, template, computer),
    )
    .await
    .expect("bounded native CLI service qualification");
}
async fn run(
    db: &support::TestDb,
    runtime: OpenShellRuntime,
    template: DevelopmentTemplate,
    computer: veoveo_computers_contract::ComputerId,
) {
    let binary = PathBuf::from(
        std::env::var_os("VEOVEO_COMPUTERS_NATIVE_CLI").expect("pinned stock CLI required"),
    );
    assert!(binary.is_absolute());
    let directory = PathBuf::from(std::env::var_os("VEOVEO_COMPUTERS_NATIVE_OUTPUT").unwrap())
        .join(format!("cli-access-{}", Uuid::now_v7().simple()));
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let version = tokio::time::timeout(
        Duration::from_secs(5),
        command(&binary, &directory).arg("--version").output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "openshell 0.1.2"
    );
    support::policy::install(&db.a, support::interactive::control()).await;
    let store = ComputersStore::new(
        db.a.clone(),
        runtime.provider_instance_id(),
        veoveo_gateway_catalog::registry().expect("installed owner catalog recipe"),
    )
    .unwrap();
    store
        .install_session_grant_policy(
            None,
            veoveo_computers::session_grants::SessionGrantPolicy {
                max_grants: 4,
                absolute_seconds: 120,
                idle_seconds: 60,
            },
        )
        .await
        .unwrap();
    let other = ComputersStore::new(
        db.b.clone(),
        runtime.provider_instance_id(),
        veoveo_gateway_catalog::registry().expect("installed owner catalog recipe"),
    )
    .unwrap();
    let signing = Signing::new();
    let a = Server::start(db.a.clone(), runtime.clone(), template.clone(), &signing).await;
    let b = Server::start(db.b.clone(), runtime.clone(), template.clone(), &signing).await;
    let edges = Edges::start([a.base.clone(), b.base.clone()], computer, &directory).await;
    let mut identity = support::browser::identity(db, "alice").await;
    identity.expires_at = Utc::now() + TimeDelta::seconds(8);
    identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .expires_at = identity.expires_at;
    let auth = crate::browser_terminal::bearer(&signing, &identity);
    let http = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let pairing_url = format!("{}/computers/{computer}/cli-pairings", a.base);
    let input = veoveo_computers_contract::CliPairingInputValue {
        name: "Native CLI fixture".into(),
        code: "ABC-2345".into(),
        callback_port: 49152,
    }
    .build()
    .unwrap();
    for origin in [None, Some("null"), Some("https://foreign.invalid")] {
        let mut request = http.post(&pairing_url).bearer_auth(&auth).json(&input);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        assert_eq!(
            request.send().await.unwrap().status(),
            reqwest::StatusCode::FORBIDDEN
        );
    }
    let response = http
        .post(&pairing_url)
        .bearer_auth(&auth)
        .header("origin", &a.origin)
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let challenge: veoveo_computers::api::CliPairingChallenge = response.json().await.unwrap();
    assert_eq!(challenge.computer_id, computer);
    let confirmation_url = format!(
        "{}/computers/{computer}/cli-pairings/{}/confirm",
        b.base, challenge.pairing_id
    );
    let response = http
        .post(&confirmation_url)
        .bearer_auth(&auth)
        .header("origin", &b.origin)
        .json(&veoveo_computers::api::CliPairingConfirmBody::default())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let grant: veoveo_computers::api::CliPairingResult = response.json().await.unwrap();
    assert_eq!(grant.pairing_id, challenge.pairing_id);
    assert_eq!(grant.computer_id, computer);
    let reused = http
        .post(&confirmation_url)
        .bearer_auth(&auth)
        .header("origin", &b.origin)
        .json(&veoveo_computers::api::CliPairingConfirmBody::default())
        .send()
        .await
        .unwrap();
    // SQL admission excludes consumed pairings from the caller's visible rows.
    assert_eq!(reused.status(), reqwest::StatusCode::NOT_FOUND);
    let error: veoveo_computers::api::ApiError = reused.json().await.unwrap();
    assert_eq!(error.code, veoveo_computers::api::ErrorCode::NotFound);
    let gateway = directory.join("config/openshell/gateways/cli-native");
    fs::create_dir_all(&gateway).unwrap();
    fs::set_permissions(&gateway, fs::Permissions::from_mode(0o700)).unwrap();
    write_private(
        &gateway.join("metadata.json"),
        &serde_json::to_vec(&serde_json::json!({
            "name":"cli-native", "gateway_endpoint":edges.endpoint, "is_remote":true,
            "gateway_port":0, "remote_host":null, "auth_mode":"cloudflare_jwt"
        }))
        .unwrap(),
    );
    write_private(
        &gateway.join("edge_token"),
        grant.token.expose_secret().as_bytes(),
    );
    let grant_id = grant.grant_id;
    let credential = &grant.token;
    let stderr = fs::File::create(directory.join("stock-cli.log")).unwrap();
    let mut child = command(&binary, &directory)
        .args([
            "--gateway",
            "cli-native",
            "sandbox",
            "connect",
            &computer.to_string(),
        ])
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(stderr)
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    let mut cli = Cli {
        group: child.id().unwrap(),
        child,
    };
    input
        .write_all(
            b"export VEOVEO_CLI_WORKER_STATE=retained; printf '\\nworker-cli-%s\\n' admitted\r",
        )
        .await
        .unwrap();
    until(&mut output, "worker-cli-admitted", &directory).await;
    tokio::time::sleep(Duration::from_secs(31)).await;
    assert!(cli.child.try_wait().unwrap().is_none());
    input
        .write_all(b"printf '\\nrenewed=%s uid=%s\\n' \"$VEOVEO_CLI_WORKER_STATE\" \"$(id -u)\"\r")
        .await
        .unwrap();
    until(&mut output, "renewed=retained uid=10001", &directory).await;
    assert!(
        edges.scoped.load(Ordering::SeqCst) > 0,
        "stock Computer-prefixed route"
    );
    assert!(
        edges.root.load(Ordering::SeqCst) > 0,
        "stock root tunnel route"
    );
    let fresh =
        ComputerActor::from_verified(&support::browser::identity(db, "alice").await).unwrap();
    other
        .revoke_access(&fresh, computer, grant_id)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while edges.active.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("revocation must close every actual relay hop within five seconds");
    // The supervised stock client can retry admission for sixty seconds after
    // disconnect. Its process lifetime is separate from relay revocation.
    let _ = input
        .write_all(b"printf '\\nREVOKED_%s\\n' executed\r")
        .await;
    for worker in [&a, &b] {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let url = revoked_grant_tunnel_url(&worker.base, &computer);
        let mut request = url.as_str().into_client_request().unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", credential.expose_secret())
                .parse()
                .unwrap(),
        );
        let refused = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .expect("fresh CLI admission deadline");
        match refused {
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                assert_eq!(response.status().as_u16(), 403);
            }
            _ => panic!("current revoked CLI grant must receive HTTP 403"),
        }
    }
    drop(grant);
    let mut trailing = Vec::new();
    let observed = tokio::time::timeout(
        Duration::from_secs(3),
        (&mut output).take(65537).read_to_end(&mut trailing),
    )
    .await;
    if let Ok(read) = observed {
        read.expect("bounded stock CLI output read");
    }
    assert!(trailing.len() <= 65536, "bounded stock CLI output");
    assert!(!String::from_utf8_lossy(&trailing).contains("REVOKED_executed"));
    assert_eq!(
        store.get(fresh.owner(), computer).await.unwrap().phase,
        veoveo_computers::api::ComputerPhase::Ready
    );
    // Security assertions precede owned teardown. EOF does not cancel the
    // vendor reconnect loop, so explicitly signal and reap this process group.
    drop(input);
    let terminated = Command::new("kill")
        .args(["-TERM", "--", &format!("-{}", cli.group)])
        .status()
        .await
        .unwrap();
    assert!(terminated.success() || cli.child.try_wait().unwrap().is_some());
    if let Ok(status) = tokio::time::timeout(Duration::from_secs(5), cli.child.wait()).await {
        status.expect("owned CLI wait");
    } else {
        Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", cli.group)])
            .status()
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), cli.child.wait())
            .await
            .expect("owned CLI reap deadline")
            .unwrap();
    }
    write_private(&directory.join("result.txt"), b"HTTP pairing and one-use confirmation across replicas; stock CLI 0.1.2 retains shell through source-token and initial-lease expiry across two service replicas and two relay hops; owner revocation closes all relay hops within five seconds, fresh admission is forbidden on both workers, no revoked command output is observed, and Computer remains Ready; vendor reconnect can retry for sixty seconds and owned teardown follows security assertions; public SSO and ingress remain unqualified\n");
    println!("Native CLI diagnostics: {}", directory.display());
    support::policy::install_default(&db.a).await;
}

#[cfg(test)]
mod route_tests {
    use super::revoked_grant_tunnel_url;

    #[test]
    fn revoked_grant_route_preserves_worker_mount_and_computer_identity() {
        let computer =
            veoveo_computers_contract::ComputerId::parse("00000000-0000-7000-8000-000000000064")
                .unwrap();
        for (base, expected) in [
            (
                "http://127.0.0.1:3210/computers/admin",
                "ws://127.0.0.1:3210/computers/cli/operator/00000000-0000-7000-8000-000000000064/_ws_tunnel",
            ),
            (
                "https://worker.example/tenant/computers/admin",
                "wss://worker.example/tenant/computers/cli/operator/00000000-0000-7000-8000-000000000064/_ws_tunnel",
            ),
        ] {
            let url = revoked_grant_tunnel_url(base, &computer);
            assert_eq!(url.as_str(), expected);
            assert!(url.query().is_none() && url.fragment().is_none());
        }
    }
}
