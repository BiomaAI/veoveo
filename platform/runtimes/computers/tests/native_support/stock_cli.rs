#[path = "diagnostics.rs"]
#[allow(dead_code)] // This fixture reuses only the shared output redactor.
mod diagnostics;
use super::{Binding, LifecycleCheckpoint, Provider, Uuid, template};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

struct Cli {
    child: tokio::process::Child,
    group: u32,
}
impl Drop for Cli {
    fn drop(&mut self) {
        // The stock client starts SSH and a ProxyCommand. Kill only this fixture's
        // explicitly created process group, including on assertion failure.
        let _ = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.group)])
            .output();
        let _ = self.child.start_kill();
    }
}

fn command(binary: &Path, provider: &Provider) -> Command {
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("XDG_CONFIG_HOME", provider.dir.join("config"))
        .env("XDG_STATE_HOME", provider.dir.join("state"))
        .env("XDG_DATA_HOME", provider.dir.join("data"))
        .env("TERM", "xterm-256color")
        .kill_on_drop(true);
    command
}

// Stock 0.1.2 bootstrap::metadata and bootstrap::oidc_token wire profiles.
// Seed an already-authenticated fixture identity; this does not qualify login.
#[derive(serde::Serialize)]
struct GatewayMetadata<'a> {
    name: &'a str,
    gateway_endpoint: &'a str,
    is_remote: bool,
    gateway_port: u16,
    auth_mode: AuthenticationMode,
    oidc_issuer: &'a str,
    oidc_client_id: &'a str,
    oidc_audience: &'a str,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum AuthenticationMode {
    #[vocabulary(rename = "oidc")]
    Oidc,
}
#[derive(serde::Serialize)]
struct OidcTokenBundle<'a> {
    access_token: &'a str,
    issuer: &'a str,
    client_id: &'a str,
}
fn private_json(path: &Path, value: &impl serde::Serialize) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("create private stock CLI input");
    let bytes = serde_json::to_vec(value).expect("encode stock CLI input");
    file.write_all(&bytes)
        .expect("write private stock CLI input");
    file.sync_all().expect("persist private stock CLI input");
}
fn authenticated_profile(provider: &Provider, endpoint: &str) -> PathBuf {
    let mut dir = provider.dir.clone();
    for segment in ["config", "openshell", "gateways", "native-probe", "mtls"] {
        dir.push(segment);
        fs::create_dir_all(&dir).expect("create isolated stock CLI directory");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))
            .expect("restrict stock CLI directory");
    }
    let gateway = dir.parent().expect("stock CLI gateway directory");
    let issuer = provider.test_issuer();
    // This is the existing native issuer's checked client identity. Its bearer
    // lasts one hour; the three-second expiry below belongs only to SSH admission.
    let client_id = "fixture-worker";
    private_json(
        &gateway.join("metadata.json"),
        &GatewayMetadata {
            name: "native-probe",
            gateway_endpoint: endpoint,
            is_remote: false,
            gateway_port: 0,
            auth_mode: AuthenticationMode::Oidc,
            oidc_issuer: issuer.config.issuer().as_str(),
            oidc_client_id: client_id,
            oidc_audience: issuer.config.resource().as_str(),
        },
    );
    // refresh_token/expires_at are optional in the stock cache. JWT verification
    // still enforces the actual signed expiry; this case does not exercise refresh.
    private_json(
        &gateway.join("oidc_token.json"),
        &OidcTokenBundle {
            access_token: issuer.token.as_str(),
            issuer: issuer.config.issuer().as_str(),
            client_id,
        },
    );
    dir
}

#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum ReadEnd {
    Matched,
    Timeout,
    Eof,
    ReadFailure,
    BoundsExceeded,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum ReadStage {
    Admitted,
    AfterAdmissionExpiry,
}
#[derive(serde::Serialize)]
struct FailureReceipt {
    schema: &'static str,
    stage: ReadStage,
    outcome: ReadEnd,
    stdout_received_bytes: usize,
    stdout_sanitized_bytes: usize,
    stdout_sha256: String,
    stderr_unavailable: bool,
    stderr_bytes: usize,
    stderr_sha256: String,
    child_exited: bool,
    child_exit_code: Option<i32>,
    child_status_unavailable: bool,
}
async fn until(
    reader: &mut tokio::process::ChildStdout,
    marker: &str,
    cli: &mut Cli,
    provider: &Provider,
    stage: ReadStage,
) {
    const MAXIMUM: usize = 65536;
    // Caller-owned capture survives cancellation of the timed read future.
    let mut output = Vec::new();
    let end = tokio::time::timeout(Duration::from_secs(15), async {
        let mut chunk = [0; 4096];
        loop {
            let count = match reader.read(&mut chunk).await {
                Ok(0) => return ReadEnd::Eof,
                Ok(count) => count,
                Err(_) => return ReadEnd::ReadFailure,
            };
            let retained = count.min(MAXIMUM - output.len());
            output.extend_from_slice(&chunk[..retained]);
            if retained < count {
                return ReadEnd::BoundsExceeded;
            }
            if String::from_utf8_lossy(&output).contains(marker) {
                return ReadEnd::Matched;
            }
        }
    })
    .await
    .unwrap_or(ReadEnd::Timeout);
    if end == ReadEnd::Matched {
        return;
    }
    let status = cli.child.try_wait();
    let mut stderr = Vec::new();
    let stderr_unavailable = match fs::File::open(provider.dir.join("stock-cli.log")) {
        Ok(file) => {
            std::io::Read::read_to_end(&mut std::io::Read::take(file, MAXIMUM as u64), &mut stderr)
                .is_err()
        }
        Err(_) => true,
    };
    let private = [provider.test_issuer().token.as_str()];
    let stdout =
        diagnostics::sanitized_output(&String::from_utf8_lossy(&output), &private, MAXIMUM);
    let stderr =
        diagnostics::sanitized_output(&String::from_utf8_lossy(&stderr), &private, MAXIMUM);
    let receipt = FailureReceipt {
        schema: "veoveo.ai/native-stock-cli-failure/v1",
        stage,
        outcome: end,
        stdout_received_bytes: output.len(),
        stdout_sanitized_bytes: stdout.len(),
        stdout_sha256: hex::encode(Sha256::digest(stdout.as_bytes())),
        stderr_unavailable,
        stderr_bytes: stderr.len(),
        stderr_sha256: hex::encode(Sha256::digest(stderr.as_bytes())),
        child_exited: matches!(status, Ok(Some(_))),
        child_exit_code: status
            .as_ref()
            .ok()
            .and_then(|value| value.as_ref())
            .and_then(|value| value.code()),
        child_status_unavailable: status.is_err(),
    };
    // No terminal text or credentials enter the printed diagnostic.
    private_json(&provider.dir.join("stock-cli-failure-output.json"), &stdout);
    private_json(&provider.dir.join("stock-cli-failure-stderr.json"), &stderr);
    private_json(&provider.dir.join("stock-cli-failure.json"), &receipt);
    eprintln!(
        "STOCK_CLI_FAILURE {}",
        serde_json::to_string(&receipt).expect("encode stock CLI failure summary")
    );
    panic!("stock CLI marker failure; bounded sanitized private receipt retained");
}

#[tokio::test]
#[ignore = "requires pinned stock CLI, exact provider binaries and native Computer image"]
async fn established_stock_cli_crosses_provider_admission_token_expiry() {
    if crate::native_support::registry_child().await {
        return;
    }
    let binary = PathBuf::from(
        std::env::var_os("VEOVEO_COMPUTERS_NATIVE_CLI").expect("pinned stock CLI path required"),
    );
    assert!(binary.is_absolute());
    let (mut provider, endpoint) = Provider::start_with_session_ttl(
        3,
        "stock_cli::established_stock_cli_crosses_provider_admission_token_expiry",
    )
    .await;
    let version = command(&binary, &provider)
        .arg("--version")
        .output()
        .await
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "openshell 0.1.2"
    );
    let mtls = authenticated_profile(&provider, &endpoint);
    for (source, target) in [
        ("ca.pem", "ca.crt"),
        ("client.pem", "tls.crt"),
        ("client-key.pem", "tls.key"),
    ] {
        fs::copy(provider.dir.join(source), mtls.join(target)).unwrap();
        fs::set_permissions(mtls.join(target), fs::Permissions::from_mode(0o600)).unwrap();
    }
    let runtime = &provider.runtime;
    let base = template(provider.image.clone());
    // Official sandbox connect attaches openshell-main, rather than opening a
    // fresh SSH shell. This case needs an interactive main workload, not the
    // renewal fixture's sleep process; nonlogin bash needs no retained mount.
    let template = veoveo_computers_runtime::DevelopmentTemplate::new(
        base.image().to_owned(),
        base.cpus(),
        base.memory_mib(),
        base.spec(Uuid::now_v7()).unwrap().policy.unwrap(),
        vec!["/bin/bash".into()],
        None,
    )
    .unwrap();
    let binding = Binding::new(Uuid::now_v7(), template.fingerprint()).unwrap();
    let checkpoint = LifecycleCheckpoint::create(
        "00000000-0000-7000-8000-000000000064".parse().unwrap(),
        veoveo_computers_runtime::LifecycleOperationId::new(),
        binding.clone(),
    )
    .unwrap();
    let created = runtime.create(&binding, &template).await.unwrap();
    let ready = runtime
        .wait_for_lifecycle(&checkpoint, &created, Duration::from_secs(30))
        .await
        .unwrap();
    let (_authority, lease) = veoveo_computers_runtime::LeaseAuthority::issue(
        tokio::time::Instant::now(),
        Duration::from_secs(30),
    )
    .unwrap();
    let access = runtime.open_shell_access(&binding, lease).await.unwrap();
    let admission = access.create_ssh_session(&ready.sandbox_id).await.unwrap();
    let timestamp = admission
        .expiration_time
        .expect("bounded session expiration");
    let seconds = u64::try_from(timestamp.seconds).expect("nonnegative session expiration");
    let nanos = u32::try_from(timestamp.nanos).expect("nonnegative expiration nanos");
    assert!(nanos < 1_000_000_000);
    let expires = UNIX_EPOCH
        .checked_add(Duration::new(seconds, nanos))
        .expect("representable session expiration");
    assert!(expires.duration_since(SystemTime::now()).unwrap() <= Duration::from_secs(3));
    access
        .revoke_ssh_session(zeroize::Zeroizing::new(admission.token))
        .await
        .unwrap();

    let stderr = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(provider.dir.join("stock-cli.log"))
        .unwrap();
    let mut child = command(&binary, &provider)
        .args([
            "--gateway",
            "native-probe",
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
    // Escaped marker characters prevent the echoed command from satisfying the check.
    input
        .write_all(b"export VEOVEO_CLI_STATE=kept; printf '\\ncli-%s\\n' admitted\r")
        .await
        .unwrap();
    until(
        &mut output,
        "cli-admitted",
        &mut cli,
        &provider,
        ReadStage::Admitted,
    )
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(cli.child.try_wait().unwrap().is_none());
    input
        .write_all(b"printf '\\nexpired=%s uid=%s\\n' \"$VEOVEO_CLI_STATE\" \"$(id -u)\"\r")
        .await
        .unwrap();
    until(
        &mut output,
        "expired=kept uid=10001",
        &mut cli,
        &provider,
        ReadStage::AfterAdmissionExpiry,
    )
    .await;
    assert_eq!(
        runtime
            .get(&binding)
            .await
            .unwrap()
            .unwrap()
            .main_process_instance_id,
        ready.main_process_instance_id
    );
    drop(cli);
    fs::write(provider.dir.join("stock-cli-result.txt"), "stock 0.1.2 CLI retains shell and input/output across native three-second SSH admission credential expiry; this does not qualify Veoveo renewal, revocation or public ingress\n").unwrap();
    provider.assert_running();
}
