use super::{Binding, LifecycleCheckpoint, Provider, Uuid, template};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
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

async fn until(reader: &mut tokio::process::ChildStdout, marker: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut output = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let count = reader.read(&mut chunk).await.unwrap();
            assert!(count > 0, "stock CLI exited before marker");
            output.extend_from_slice(&chunk[..count]);
            assert!(output.len() <= 65536, "bounded fixture output");
            if String::from_utf8_lossy(&output).contains(marker) {
                return;
            }
        }
    })
    .await
    .expect("stock CLI native terminal response");
}

#[tokio::test]
#[ignore = "requires pinned stock CLI, exact provider binaries and native Computer image"]
async fn established_stock_cli_crosses_provider_admission_token_expiry() {
    let binary = PathBuf::from(
        std::env::var_os("VEOVEO_COMPUTERS_NATIVE_CLI").expect("pinned stock CLI path required"),
    );
    assert!(binary.is_absolute());
    let (mut provider, endpoint) = Provider::start_with_session_ttl(3).await;
    let version = command(&binary, &provider)
        .arg("--version")
        .output()
        .await
        .unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8_lossy(&version.stdout).trim(),
        "openshell 0.0.116"
    );
    let mtls = provider
        .dir
        .join("config/openshell/gateways/native-probe/mtls");
    fs::create_dir_all(&mtls).unwrap();
    fs::set_permissions(&mtls, fs::Permissions::from_mode(0o700)).unwrap();
    for (source, target) in [
        ("ca.pem", "ca.crt"),
        ("client.pem", "tls.crt"),
        ("client-key.pem", "tls.key"),
    ] {
        fs::copy(provider.dir.join(source), mtls.join(target)).unwrap();
        fs::set_permissions(mtls.join(target), fs::Permissions::from_mode(0o600)).unwrap();
    }
    let registered = command(&binary, &provider)
        .args([
            "gateway",
            "add",
            &endpoint,
            "--local",
            "--name",
            "native-probe",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        registered.status.success(),
        "stock CLI registration: {}",
        String::from_utf8_lossy(&registered.stderr)
    );
    let runtime = &provider.runtime;
    let template = template(provider.image.clone());
    let binding = Binding::new(Uuid::now_v7(), template.fingerprint()).unwrap();
    let checkpoint =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), binding.clone()).unwrap();
    let created = runtime.create(&binding, &template).await.unwrap();
    let ready = runtime
        .wait_for_lifecycle(&checkpoint, &created, Duration::from_secs(30))
        .await
        .unwrap();
    let access = runtime
        .open_shell_access(&binding, SystemTime::now() + Duration::from_secs(60))
        .await
        .unwrap();
    let admission = access.create_ssh_session(&ready.sandbox_id).await.unwrap();
    let expires = UNIX_EPOCH + Duration::from_millis(admission.expires_at_ms as u64);
    assert!(expires.duration_since(SystemTime::now()).unwrap() <= Duration::from_secs(3));
    access
        .revoke_ssh_session(zeroize::Zeroizing::new(admission.token))
        .await
        .unwrap();

    let stderr = fs::File::create(provider.dir.join("stock-cli.log")).unwrap();
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
    until(&mut output, "cli-admitted").await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(cli.child.try_wait().unwrap().is_none());
    input
        .write_all(b"printf '\\nexpired=%s uid=%s\\n' \"$VEOVEO_CLI_STATE\" \"$(id -u)\"\r")
        .await
        .unwrap();
    until(&mut output, "expired=kept uid=10001").await;
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
    fs::write(provider.dir.join("stock-cli-result.txt"), "stock 0.0.116 CLI retains shell and input/output across native three-second SSH admission credential expiry; this does not qualify Veoveo renewal, revocation or public ingress\n").unwrap();
    provider.assert_running();
}
