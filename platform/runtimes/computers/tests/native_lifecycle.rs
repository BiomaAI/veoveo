#[path = "native_support/block_home.rs"]
mod block_home;
mod native_support;
#[path = "native_support/template.rs"]
mod retained_template;
#[path = "native_support/stock_cli.rs"]
mod stock_cli;
use native_support::Provider;
use std::time::Duration;
use uuid::Uuid;
use veoveo_computers_runtime::{protocol::sandbox::v1 as policy, *};

fn template(image: String) -> DevelopmentTemplate {
    DevelopmentTemplate::new(
        image,
        2,
        2048,
        policy::SandboxPolicy {
            version: 1,
            filesystem: Some(policy::FilesystemPolicy {
                include_workdir: true,
                read_only: [
                    "/usr",
                    "/lib",
                    "/lib64",
                    "/bin",
                    "/etc/passwd",
                    "/etc/group",
                    "/etc/profile",
                    "/etc/profile.d",
                    "/etc/bash.bashrc",
                    "/etc/nsswitch.conf",
                    "/etc/resolv.conf",
                    "/etc/ssl/certs",
                    "/proc",
                ]
                .map(str::to_owned)
                .into(),
                read_write: ["/tmp", "/dev/null", "/dev/tty", "/dev/pts"]
                    .map(str::to_owned)
                    .into(),
            }),
            landlock: Some(policy::LandlockPolicy {
                compatibility: "hard_requirement".into(),
            }),
            process: Some(policy::ProcessPolicy {
                run_as_user: "10001".into(),
                run_as_group: "10001".into(),
            }),
            ..Default::default()
        },
        vec!["/bin/bash".into(), "-l".into()],
        None,
    )
    .unwrap()
}

async fn replay(terminal: &mut Terminal) {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            match terminal
                .read()
                .await
                .unwrap()
                .expect("terminal remains open")
            {
                TerminalOutput::ReplayComplete => return,
                TerminalOutput::Data(_) => {}
            }
        }
    })
    .await
    .expect("native replay boundary");
}

#[tokio::test]
#[ignore = "requires exact provider binaries and a digest-pinned Computer image; starts an isolated Docker provider"]
async fn native_lifecycle_terminal_and_epoch_recovery() {
    if native_support::registry_child().await {
        return;
    }
    let mut provider = Provider::start("native_lifecycle_terminal_and_epoch_recovery").await;
    let runtime = provider.runtime.clone();
    let template = retained_template::retained_template(provider.image.clone());
    let computer = Uuid::now_v7();
    let binding = Binding::new(computer, template.fingerprint()).unwrap();
    let home = block_home::BlockHome::create(
        provider.dir.clone(),
        provider.image.clone(),
        computer,
        provider.docker_socket(),
    );
    let create = LifecycleCheckpoint::create(
        "00000000-0000-7000-8000-000000000064".parse().unwrap(),
        veoveo_computers_runtime::LifecycleOperationId::new(),
        binding.clone(),
    )
    .unwrap();
    let created = runtime.create(&binding, &template).await.unwrap();
    let ready = runtime
        .wait_for_lifecycle(&create, &created, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(!ready.main_process_instance_id.is_empty());
    let (_authority, lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30)).unwrap();
    let mut terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), lease.clone())
        .await
        .unwrap();
    replay(&mut terminal).await;
    // A shell-local variable proves that reattachment retains the same process.
    terminal
        .write(b"export VEOVEO_NATIVE_RETAINED=kept\r")
        .await
        .unwrap();
    terminal.detach().await.unwrap();
    let mut terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), lease.clone())
        .await
        .unwrap();
    assert_eq!(
        terminal.main_process_instance_id(),
        ready.main_process_instance_id
    );
    assert_eq!(terminal.sandbox_id(), ready.sandbox_id);
    replay(&mut terminal).await;
    terminal
        .write(b"printf '\\nretained=%s uid=%s\\n' \"$VEOVEO_NATIVE_RETAINED\" \"$(id -u)\"\r")
        .await
        .unwrap();
    let output = tokio::time::timeout(Duration::from_secs(10), async {
        let mut output = Vec::new();
        loop {
            if let Some(TerminalOutput::Data(bytes)) = terminal.read().await.unwrap() {
                output.extend(bytes);
                assert!(output.len() <= 65536);
                if String::from_utf8_lossy(&output).contains("retained=kept uid=10001") {
                    return output;
                }
            } else {
                panic!("terminal ended before native result");
            }
        }
    })
    .await
    .expect("retained shell and numeric identity");
    assert!(!output.is_empty());
    terminal.write(b"printf '%s' 'controller-retained-bytes-v1' > \"$HOME/controller-native-marker\" && sync \"$HOME/controller-native-marker\" && printf '\\npre-restart-marker=%s\\n' \"$(cat \"$HOME/controller-native-marker\")\"\r").await.unwrap();
    marker_output(
        &mut terminal,
        "pre-restart-marker=controller-retained-bytes-v1",
    )
    .await;
    terminal.detach().await.unwrap();
    assert!(matches!(
        runtime
            .reconcile_lifecycle(&create, Duration::from_secs(10))
            .await
            .unwrap(),
        LifecycleObservation::Reached(_)
    ));
    home.assert_registered_no_copy();
    let before_controller = retained_identity(&provider, &ready, &home.volume).await;
    let old_supervisor = supervisor_identity(&provider, &ready).await;
    let controller_pids = provider.restart_controller().await;
    let runtime = provider.runtime.clone();
    let recovered = runtime
        .wait_for_lifecycle(&create, &created, Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(recovered.sandbox_id, ready.sandbox_id);
    assert_eq!(
        recovered.main_process_instance_id, ready.main_process_instance_id,
        "controller restart must preserve the running canonical process"
    );
    assert_eq!(
        retained_identity(&provider, &recovered, &home.volume).await,
        before_controller
    );
    assert_ne!(
        supervisor_identity(&provider, &recovered).await,
        old_supervisor,
        "the replacement controller must own a fresh single supervisor"
    );
    home.assert_registered_no_copy();
    let (_fresh_authority, fresh_lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30)).unwrap();
    let mut terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), fresh_lease)
        .await
        .unwrap();
    replay(&mut terminal).await;
    terminal.write(b"printf '\\npost-controller-marker=%s\\n' \"$(cat \"$HOME/controller-native-marker\")\"\r").await.unwrap();
    marker_output(
        &mut terminal,
        "post-controller-marker=controller-retained-bytes-v1",
    )
    .await;
    terminal.detach().await.unwrap();
    let ready = recovered;
    let stop = LifecycleCheckpoint::stop(
        "00000000-0000-7000-8000-000000000064".parse().unwrap(),
        veoveo_computers_runtime::LifecycleOperationId::new(),
        binding.clone(),
        &ready,
    )
    .unwrap();
    let stopping = runtime.stop(&binding, &ready).await.unwrap();
    let stopped = runtime
        .wait_for_lifecycle(&stop, &stopping, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(matches!(
        runtime
            .reconcile_lifecycle(&stop, Duration::from_secs(10))
            .await
            .unwrap(),
        LifecycleObservation::Reached(_)
    ));
    let start = LifecycleCheckpoint::start(
        "00000000-0000-7000-8000-000000000064".parse().unwrap(),
        veoveo_computers_runtime::LifecycleOperationId::new(),
        binding.clone(),
        &stopped,
    )
    .unwrap();
    let starting = runtime.start(&binding, &stopped).await.unwrap();
    let restarted = runtime
        .wait_for_lifecycle(&start, &starting, Duration::from_secs(30))
        .await
        .unwrap();
    assert_ne!(
        ready.main_process_instance_id,
        restarted.main_process_instance_id
    );
    assert!(matches!(
        runtime
            .reconcile_lifecycle(&start, Duration::from_secs(10))
            .await
            .unwrap(),
        LifecycleObservation::Reached(_)
    ));
    assert_eq!(restarted.sandbox_id, ready.sandbox_id);
    assert_eq!(
        retained_identity(&provider, &restarted, &home.volume).await,
        before_controller
    );
    home.assert_registered_no_copy();
    let (_final_authority, final_lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30)).unwrap();
    let mut terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), final_lease)
        .await
        .unwrap();
    replay(&mut terminal).await;
    terminal.write(b"printf '\\npost-start-marker=%s uid=%s\\n' \"$(cat \"$HOME/controller-native-marker\")\" \"$(id -u)\"\r").await.unwrap();
    marker_output(
        &mut terminal,
        "post-start-marker=controller-retained-bytes-v1 uid=10001",
    )
    .await;
    terminal.detach().await.unwrap();
    std::fs::write(provider.dir.join("controller-restart-result.json"), serde_json::to_vec_pretty(&serde_json::json!({"oldControllerPid":controller_pids.0,"newControllerPid":controller_pids.1,"retained":before_controller,"sandboxId":restarted.sandbox_id,"newMainProcess":restarted.main_process_instance_id,"scope":"actual controller restart and Stop/Start retained state; no installation claim"})).unwrap()).unwrap();
    std::fs::set_permissions(
        provider.dir.join("controller-restart-result.json"),
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .unwrap();
    std::fs::write(provider.dir.join("result.txt"), "native controller/driver restart, retained image/container/home/bytes, current auth, uid, stop/start and reconciled epoch passed\n").unwrap();
    provider.assert_running();
}

#[tokio::test]
#[ignore = "requires exact provider binaries and native Computer image; exercises renewable Veoveo terminal authority"]
async fn native_terminal_renews_without_reconnecting_and_revokes_access() {
    if native_support::registry_child().await {
        return;
    }
    let (mut provider, _) = Provider::start_with_session_ttl(
        3,
        "native_terminal_renews_without_reconnecting_and_revokes_access",
    )
    .await;
    let runtime = &provider.runtime;
    let template = template(provider.image.clone());
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
    let (authority, lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(2)).unwrap();
    let mut terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), lease)
        .await
        .unwrap();
    replay(&mut terminal).await;
    terminal
        .write(b"export VEOVEO_RENEWED=kept\r")
        .await
        .unwrap();
    for _ in 0..8 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        authority
            .renew(tokio::time::Instant::now(), Duration::from_secs(2))
            .unwrap();
    }
    terminal
        .write(b"printf '\\nrenewed=%s\\n' \"$VEOVEO_RENEWED\"\r")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        let mut output = Vec::new();
        loop {
            if let Some(TerminalOutput::Data(bytes)) = terminal.read().await.unwrap() {
                output.extend(bytes);
                assert!(output.len() <= 65536);
                if String::from_utf8_lossy(&output).contains("renewed=kept") {
                    break;
                }
            } else {
                panic!("renewed native terminal ended");
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        terminal.main_process_instance_id(),
        ready.main_process_instance_id
    );
    authority.revoke();
    assert!(matches!(
        terminal.read().await,
        Err(RuntimeFailure::LeaseExpired)
    ));
    assert!(matches!(
        terminal.write(b"forbidden").await,
        Err(RuntimeFailure::LeaseExpired)
    ));
    let detached = tokio::time::timeout(Duration::from_secs(5), terminal.detach())
        .await
        .unwrap();
    assert!(matches!(
        detached,
        Ok(()) | Err(RuntimeFailure::LeaseExpired)
    ));
    // Access loss preserves the original process. Fresh authority reattaches to it.
    let (_authority, lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30)).unwrap();
    let mut terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), lease)
        .await
        .unwrap();
    replay(&mut terminal).await;
    assert_eq!(
        terminal.main_process_instance_id(),
        ready.main_process_instance_id
    );
    terminal.detach().await.unwrap();
    std::fs::write(provider.dir.join("renewal-result.txt"), "native terminal exchanges data across lease renewal and provider admission credential expiry without reattach; revocation denies input/output, fresh authority retains the same process\n").unwrap();
    provider.assert_running();
}

async fn marker_output(terminal: &mut Terminal, expected: &str) {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut output = Vec::new();
        loop {
            match terminal
                .read()
                .await
                .unwrap()
                .expect("canonical process remains open")
            {
                TerminalOutput::Data(bytes) => {
                    output.extend(bytes);
                    assert!(
                        output.len() <= 65536,
                        "terminal marker output exceeded 64 KiB"
                    );
                    if String::from_utf8_lossy(&output).contains(expected) {
                        return;
                    }
                }
                TerminalOutput::ReplayComplete => {}
            }
        }
    })
    .await
    .expect("settled retained marker command");
}

#[derive(serde::Deserialize)]
struct InspectedContainer {
    id: String,
    image: String,
    labels: std::collections::BTreeMap<String, String>,
    mounts: Vec<MountFact>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct MountFact {
    source: Option<String>,
    target: Option<String>,
    #[serde(default)]
    read_only: Option<bool>,
    volume_options: Option<VolumeFact>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct VolumeFact {
    no_copy: Option<bool>,
    subpath: Option<String>,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct RetainedIdentity {
    daemon_id: String,
    daemon_socket_inode: u64,
    database_inode: u64,
    physical_home_inode: u64,
    container_id: String,
    image_id: String,
    home_mount: MountFact,
    restart_state_sha256: String,
}
async fn docker_observation(provider: &Provider, arguments: &[&str]) -> String {
    let mut command = tokio::process::Command::new("docker");
    command
        .arg("--host")
        .arg(format!("unix://{}", provider.docker_socket().display()))
        .args(arguments)
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .expect("owned Docker read deadline")
        .expect("owned Docker read transport");
    assert!(
        output.status.success(),
        "owned Docker identity observation failed; raw output withheld"
    );
    assert!(
        output.stdout.len() <= 16384,
        "owned Docker observation exceeded 16 KiB"
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
async fn owned_container(
    provider: &Provider,
    observation: &Observation,
    role: &str,
) -> InspectedContainer {
    let namespace = format!(
        "label=openshell.ai/sandbox-namespace={}",
        provider.namespace()
    );
    let sandbox = format!("label=openshell.ai/sandbox-id={}", observation.sandbox_id);
    let role_filter = format!("label=openshell.ai/isolation-role={role}");
    let ids = docker_observation(
        provider,
        &[
            "ps",
            "--all",
            "--quiet",
            "--no-trunc",
            "--filter",
            &namespace,
            "--filter",
            &sandbox,
            "--filter",
            &role_filter,
        ],
    )
    .await;
    let ids: Vec<_> = ids.split_whitespace().collect();
    assert_eq!(
        ids.len(),
        1,
        "exactly one owned resource must exist for the role"
    );
    assert_eq!(ids[0].len(), 64);
    assert!(ids[0].bytes().all(|b| b.is_ascii_hexdigit()));
    let format = r#"{"id":{{json .Id}},"image":{{json .Image}},"labels":{{json .Config.Labels}},"mounts":{{json .HostConfig.Mounts}}}"#;
    let body = docker_observation(provider, &["inspect", "--format", format, ids[0]]).await;
    let inspected: InspectedContainer =
        serde_json::from_str(&body).expect("typed owned Docker identity projection");
    assert_eq!(inspected.id, ids[0]);
    for (label, expected) in [
        ("openshell.ai/sandbox-id", observation.sandbox_id.as_str()),
        ("openshell.ai/sandbox-namespace", provider.namespace()),
        ("openshell.ai/isolation-role", role),
        ("openshell.ai/managed-by", "openshell"),
    ] {
        assert_eq!(
            inspected.labels.get(label).map(String::as_str),
            Some(expected)
        );
    }
    inspected
}
async fn supervisor_identity(provider: &Provider, observation: &Observation) -> String {
    owned_container(provider, observation, "supervisor")
        .await
        .id
}
async fn retained_identity(
    provider: &Provider,
    observation: &Observation,
    home: &str,
) -> RetainedIdentity {
    use sha2::{Digest, Sha256};
    let inspected = owned_container(provider, observation, "sandbox").await;
    let image = docker_observation(
        provider,
        &["image", "inspect", "--format", "{{.Id}}", &provider.image],
    )
    .await;
    assert_eq!(
        inspected.image, image,
        "selected admitted image must remain exact"
    );
    assert_eq!(
        inspected
            .labels
            .get("openshell.ai/sandbox-workspace")
            .map(String::as_str),
        Some("default")
    );
    let mounts: Vec<_> = inspected
        .mounts
        .iter()
        .filter(|m| m.source.as_deref() == Some(home))
        .collect();
    assert_eq!(mounts.len(), 1);
    let mount = mounts[0];
    assert_eq!(mount.target.as_deref(), Some(PERSISTENT_HOME));
    assert_eq!(mount.read_only, Some(false));
    assert_eq!(mount.volume_options.as_ref().unwrap().no_copy, Some(true));
    assert_eq!(
        mount.volume_options.as_ref().unwrap().subpath.as_deref(),
        Some("home")
    );
    let path = provider
        .dir
        .join("state/openshell/docker-sandbox-tokens")
        .join(provider.namespace())
        .join(&observation.sandbox_id)
        .join("supervisor-restart-v1.json");
    let metadata =
        std::fs::symlink_metadata(&path).expect("current immutable restart record exists");
    use std::os::unix::fs::PermissionsExt;
    assert!(metadata.is_file() && metadata.permissions().mode() & 0o077 == 0);
    assert!(metadata.len() <= 1_048_576);
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() <= 1_048_576);
    use std::os::unix::fs::MetadataExt;
    RetainedIdentity {
        daemon_socket_inode: std::fs::metadata(provider.docker_socket()).unwrap().ino(),
        database_inode: std::fs::metadata(provider.dir.join("gateway.sqlite"))
            .unwrap()
            .ino(),
        physical_home_inode: std::fs::metadata(provider.dir.join("home.ext4"))
            .unwrap()
            .ino(),
        daemon_id: docker_observation(provider, &["info", "--format", "{{.ID}}"]).await,
        container_id: inspected.id,
        image_id: inspected.image,
        home_mount: mount.clone(),
        restart_state_sha256: hex::encode(Sha256::digest(bytes)),
    }
}
