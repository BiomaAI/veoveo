//! Actual stock CLI and native provider through two bounded relay hops. The
//! fixture installs a domain-issued credential in the stock client's private file;
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
use veoveo_computers::{ComputerActor, ComputersStore, cli_grants::CliPairingRequest};
use veoveo_computers_runtime::{Binding, DevelopmentTemplate, OpenShellRuntime};

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
pub async fn qualify(
    db: &support::TestDb,
    runtime: OpenShellRuntime,
    template: DevelopmentTemplate,
    computer: Uuid,
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
    computer: Uuid,
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
        "openshell 0.0.116"
    );
    support::policy::install(&db.a, support::interactive::control()).await;
    let store = ComputersStore::new(db.a.clone(), runtime.provider_instance_id()).unwrap();
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
    let other = ComputersStore::new(db.b.clone(), runtime.provider_instance_id()).unwrap();
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
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let challenge = store
        .begin_cli_pairing(
            &actor,
            computer,
            &CliPairingRequest {
                name: "Native CLI fixture".into(),
                code: "ABC-2345".into(),
                callback_port: 49152,
            },
        )
        .await
        .unwrap();
    let grant = other
        .confirm_cli_pairing(&actor, computer, challenge.pairing_id)
        .await
        .unwrap();
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
        grant.credential.expose_secret().as_bytes(),
    );
    drop(grant.credential);
    let binding = Binding::new(computer, template.fingerprint()).unwrap();
    let stderr = fs::File::create(directory.join("stock-cli.log")).unwrap();
    let mut child = command(&binary, &directory)
        .args([
            "--gateway",
            "cli-native",
            "sandbox",
            "connect",
            &binding.name(),
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
        .revoke_cli_grant(&fresh, computer, grant.grant_id)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while edges.active.load(Ordering::SeqCst) > 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("revocation must close every actual relay hop within five seconds");
    let idle_client_waited_for_input = cli.child.try_wait().unwrap().is_none();
    // The stock ProxyCommand uses blocking Tokio stdin. Once access is already
    // closed, an idle process may await local input before runtime shutdown ends.
    let _ = input
        .write_all(b"printf '\\nREVOKED_%s\\n' executed\r")
        .await;
    tokio::time::timeout(Duration::from_secs(5), cli.child.wait())
        .await
        .expect("stock CLI must report interruption after local input")
        .unwrap();
    let mut trailing = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(3),
        output.take(65536).read_to_end(&mut trailing),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(!String::from_utf8_lossy(&trailing).contains("REVOKED_executed"));
    println!(
        "CLI relay revocation passed; idle client awaited input: {idle_client_waited_for_input}"
    );
    assert_eq!(
        store.get(fresh.owner(), computer).await.unwrap().phase,
        veoveo_computers::api::ComputerPhase::Ready
    );
    write_private(&directory.join("result.txt"), b"stock CLI 0.0.116 retains shell through source-token and initial-lease expiry across two service replicas and two relay hops; owner revocation closes all relay hops within five seconds without stopping Computer; idle stock ProxyCommand may need local input to finish shutdown; public SSO and ingress remain unqualified\n");
    println!("Native CLI diagnostics: {}", directory.display());
    support::policy::install_default(&db.a).await;
}
