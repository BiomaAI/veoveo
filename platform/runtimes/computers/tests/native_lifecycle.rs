// Independent test crates consume different subsets of these shared fixture APIs.
#[path = "native_support/block_home.rs"]
pub mod block_home;
pub mod native_support;
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

#[tokio::test]
#[ignore = "requires exact provider binaries and a digest-pinned Computer image; starts an isolated Docker provider"]
async fn native_lifecycle_terminal_and_epoch_recovery() {
    if native_support::registry_child().await {
        return;
    }
    let mut provider =
        Provider::start_with_execution_logging("native_lifecycle_terminal_and_epoch_recovery")
            .await;
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
    let terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), lease.clone())
        .await
        .unwrap();
    // Every fresh shell uses the same guest identity but starts without prior shell state.
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
                if String::from_utf8_lossy(&output).contains("retained= uid=10001") {
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
    let controller_pids = provider.crash_controller_and_recover().await;
    let runtime = provider.runtime.clone();
    // The old Create response is already settled and cannot prove recovery. Read
    // the replacement controller before assessing readiness or process continuity.
    let current = runtime
        .get(&binding)
        .await
        .expect("fresh replacement-controller observation")
        .expect("retained sandbox exists after abrupt controller loss");
    let recovered = runtime
        .wait_for_lifecycle(&create, &current, Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(recovered.sandbox_id, ready.sandbox_id);
    assert_eq!(
        recovered.main_process_instance_id, ready.main_process_instance_id,
        "abrupt controller-loss recovery must preserve the running canonical process"
    );
    assert_eq!(
        retained_identity(&provider, &recovered, &home.volume).await,
        before_controller
    );
    assert_eq!(
        supervisor_identity(&provider, &recovered).await,
        old_supervisor,
        "gateway-only recovery must preserve the exact running physical supervisor"
    );
    home.assert_registered_no_copy();
    let (_fresh_authority, fresh_lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30)).unwrap();
    let mut terminal = match runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), fresh_lease)
        .await
    {
        Ok(terminal) => terminal,
        Err(error) => {
            // Read-only diagnostics must finish before Provider::drop removes
            // this exact companion. Never replay attachment or change its owner.
            capture_recovery_attachment(&provider, &old_supervisor, &recovered, controller_pids)
                .await;
            panic!("replacement-gateway terminal attachment failed: {error:?}");
        }
    };
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
    terminal.write(b"printf '\\npost-start-marker=%s uid=%s\\n' \"$(cat \"$HOME/controller-native-marker\")\" \"$(id -u)\"\r").await.unwrap();
    marker_output(
        &mut terminal,
        "post-start-marker=controller-retained-bytes-v1 uid=10001",
    )
    .await;
    terminal.detach().await.unwrap();
    std::fs::write(provider.dir.join("controller-restart-result.json"), serde_json::to_vec_pretty(&serde_json::json!({"oldControllerPid":controller_pids.0,"newControllerPid":controller_pids.1,"retained":before_controller,"retainedSupervisor":old_supervisor,"currentAuthenticatedTerminalRelay":true,"sandboxId":restarted.sandbox_id,"newMainProcess":restarted.main_process_instance_id,"scope":"actual controller restart and Stop/Start retained state; no installation claim"})).unwrap()).unwrap();
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
    let terminal = runtime
        .attach(&binding, TerminalSize::new(100, 30).unwrap(), lease)
        .await
        .unwrap();
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
    effective_mounts: Vec<EffectiveMountFact>,
    state: ContainerProcessState,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerProcessState {
    running: bool,
    pid: i64,
    started_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SupervisorIdentity {
    container_id: String,
    process: ContainerProcessState,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct MountFact {
    source: Option<String>,
    target: Option<String>,
    #[serde(default)]
    read_only: bool,
    volume_options: Option<VolumeFact>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct VolumeFact {
    no_copy: Option<bool>,
    subpath: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct EffectiveMountFact {
    #[serde(rename = "Type")]
    mount_type: String,
    name: Option<String>,
    destination: String,
    #[serde(rename = "RW")]
    writable: bool,
}

fn retained_home_mounts<'a>(
    inspected: &'a InspectedContainer,
    home: &str,
) -> (&'a MountFact, &'a EffectiveMountFact) {
    let requested: Vec<_> = inspected
        .mounts
        .iter()
        .filter(|mount| {
            mount.source.as_deref() == Some(home)
                || mount.target.as_deref() == Some(PERSISTENT_HOME)
        })
        .collect();
    assert_eq!(requested.len(), 1, "one exact requested retained home");
    let request = requested[0];
    assert_eq!(request.source.as_deref(), Some(home));
    assert_eq!(request.target.as_deref(), Some(PERSISTENT_HOME));
    // Docker API 1.53 Mount.ReadOnly is optional. Moby's bool/omitempty
    // request form omits false; Serde default bool rejects explicit null.
    assert!(!request.read_only, "retained home request must be writable");
    assert_eq!(request.volume_options.as_ref().unwrap().no_copy, Some(true));
    assert_eq!(
        request.volume_options.as_ref().unwrap().subpath.as_deref(),
        Some("home")
    );
    let effective: Vec<_> = inspected
        .effective_mounts
        .iter()
        .filter(|mount| mount.name.as_deref() == Some(home) || mount.destination == PERSISTENT_HOME)
        .collect();
    assert_eq!(effective.len(), 1, "one exact effective retained home");
    let effective = effective[0];
    assert_eq!(effective.mount_type, "volume");
    assert_eq!(effective.name.as_deref(), Some(home));
    assert_eq!(effective.destination, PERSISTENT_HOME);
    assert!(
        effective.writable,
        "effective retained home must be writable"
    );
    (request, effective)
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
    effective_home_mount: EffectiveMountFact,
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
    let format = r#"{"id":{{json .Id}},"image":{{json .Image}},"labels":{{json .Config.Labels}},"mounts":{{json .HostConfig.Mounts}},"effective_mounts":{{json .Mounts}},"state":{{json .State}}}"#;
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
async fn supervisor_identity(provider: &Provider, observation: &Observation) -> SupervisorIdentity {
    let inspected = owned_container(provider, observation, "supervisor").await;
    assert!(
        inspected.state.running,
        "the admitted supervisor must be running"
    );
    assert!(inspected.state.pid > 0);
    assert!(!inspected.state.started_at.is_empty());
    SupervisorIdentity {
        container_id: inspected.id,
        process: inspected.state,
    }
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
    let (mount, effective_mount) = retained_home_mounts(&inspected, home);
    let home_mount = mount.clone();
    let effective_home_mount = effective_mount.clone();
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
        home_mount,
        effective_home_mount,
        restart_state_sha256: hex::encode(Sha256::digest(bytes)),
    }
}

#[cfg(test)]
mod retained_mount_tests {
    use super::*;

    fn inspected() -> InspectedContainer {
        serde_json::from_str(r#"{
            "id":"owned", "image":"admitted", "labels":{},
            "state":{"Running":true,"Pid":123,"StartedAt":"2026-10-07T00:00:00Z"},
            "mounts":[
                {"Source":"owned-home","Target":"/sandbox/persistent","VolumeOptions":{"NoCopy":true,"Subpath":"home"}},
                {"Source":"channel","Target":"/.openshell/channel","ReadOnly":true,"VolumeOptions":{"NoCopy":true}}
            ],
            "effective_mounts":[
                {"Type":"volume","Name":"owned-home","Destination":"/sandbox/persistent","RW":true},
                {"Type":"volume","Name":"channel","Destination":"/.openshell/channel","RW":false}
            ]
        }"#).unwrap()
    }

    fn refuses(inspected: InspectedContainer) {
        assert!(
            std::panic::catch_unwind(|| retained_home_mounts(&inspected, "owned-home")).is_err()
        );
    }

    #[test]
    fn omitted_and_explicit_false_requests_admit_only_actual_writable_home() {
        let omitted = inspected();
        let mut explicit = inspected();
        explicit.mounts[0] = serde_json::from_str(r#"{"Source":"owned-home","Target":"/sandbox/persistent","ReadOnly":false,"VolumeOptions":{"NoCopy":true,"Subpath":"home"}}"#).unwrap();
        let omitted = retained_home_mounts(&omitted, "owned-home");
        let explicit = retained_home_mounts(&explicit, "owned-home");
        assert_eq!(
            omitted, explicit,
            "false omission normalizes only the request form"
        );
        assert!(omitted.1.writable);
    }

    #[test]
    fn null_wrongtype_readonly_requests_and_unwritable_effective_mounts_refuse() {
        for invalid in ["null", "\"false\"", "0"] {
            let body = format!(r#"{{"ReadOnly":{invalid}}}"#);
            assert!(serde_json::from_str::<MountFact>(&body).is_err());
        }
        let mut readonly = inspected();
        readonly.mounts[0].read_only = true;
        refuses(readonly);
        let mut readonly = inspected();
        readonly.effective_mounts[0].writable = false;
        refuses(readonly);
        for invalid in [
            r#"{"Type":"volume","Name":"owned-home","Destination":"/sandbox/persistent"}"#,
            r#"{"Type":"volume","Name":"owned-home","Destination":"/sandbox/persistent","RW":null}"#,
            r#"{"Type":"volume","Name":"owned-home","Destination":"/sandbox/persistent","RW":"true"}"#,
        ] {
            assert!(serde_json::from_str::<EffectiveMountFact>(invalid).is_err());
        }
    }

    #[test]
    fn retained_home_identity_count_and_no_copy_subpath_cannot_be_substituted() {
        let mut variants = Vec::new();
        let mut missing = inspected();
        missing.effective_mounts.remove(0);
        variants.push(missing);
        let mut wrong = inspected();
        wrong.effective_mounts[0].name = Some("foreign".into());
        variants.push(wrong);
        let mut wrong = inspected();
        wrong.effective_mounts[0].name = None;
        variants.push(wrong);
        let mut wrong = inspected();
        wrong.effective_mounts[0].destination = "/foreign".into();
        variants.push(wrong);
        let mut wrong = inspected();
        wrong.effective_mounts[0].mount_type = "bind".into();
        variants.push(wrong);
        let mut duplicate = inspected();
        duplicate
            .effective_mounts
            .push(duplicate.effective_mounts[0].clone());
        variants.push(duplicate);
        let mut wrong = inspected();
        wrong.mounts[0].source = Some("foreign".into());
        variants.push(wrong);
        let mut wrong = inspected();
        wrong.mounts[0].volume_options.as_mut().unwrap().no_copy = Some(false);
        variants.push(wrong);
        let mut wrong = inspected();
        wrong.mounts[0].volume_options.as_mut().unwrap().subpath = Some("other".into());
        variants.push(wrong);
        for variant in variants {
            refuses(variant);
        }
    }
}

// Diagnostic facts are observations, never session admission or settlement.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum CompanionFact {
    SessionEstablished,
    SessionClosed,
    SessionReconnectFailed,
    SshRelayOpen,
    SshRelayClosed,
    SshRelayFailed,
    NetworkOpen,
    NetworkClose,
    NetworkFail,
}
#[derive(serde::Deserialize)]
struct DiagnosticEvent {
    message: Option<String>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LogFacts {
    facts: Vec<CompanionFact>,
    omitted_lines: usize,
    truncated: bool,
}
fn log_facts(bytes: &[u8], truncated: bool) -> LogFacts {
    let mut facts = Vec::new();
    let mut omitted_lines = 0;
    for line in String::from_utf8_lossy(bytes).lines().take(128) {
        // Full JSON messages are decoded privately; only a closed category is
        // emitted. The shorthand's destinations/reasons/message suffixes are
        // never copied. Arbitrary tracing and process output is omitted.
        let event = serde_json::from_str::<DiagnosticEvent>(line).ok();
        let message = event.as_ref().and_then(|event| event.message.as_deref());
        let fact = match message {
            Some(text) if text.starts_with("supervisor session established (session_id=") => {
                Some(CompanionFact::SessionEstablished)
            }
            Some(text) if text.starts_with("supervisor session ended cleanly (") => {
                Some(CompanionFact::SessionClosed)
            }
            Some(text) if text.starts_with("supervisor session failed, reconnecting (attempt ") => {
                Some(CompanionFact::SessionReconnectFailed)
            }
            Some(text) if text.starts_with("ssh relay open (channel_id=") => {
                Some(CompanionFact::SshRelayOpen)
            }
            Some(text) if text.starts_with("ssh relay closed (channel_id=") => {
                Some(CompanionFact::SshRelayClosed)
            }
            Some(text) if text.starts_with("ssh relay bridge failed (channel_id=") => {
                Some(CompanionFact::SshRelayFailed)
            }
            _ => match line
                .split_once(" OCSF ")
                .map(|(_, text)| text.split_whitespace().next())
            {
                Some(Some("NET:OPEN")) => Some(CompanionFact::NetworkOpen),
                Some(Some("NET:CLOSE")) => Some(CompanionFact::NetworkClose),
                Some(Some("NET:FAIL")) => Some(CompanionFact::NetworkFail),
                _ => None,
            },
        };
        if let Some(fact) = fact {
            if facts.len() < 32 {
                facts.push(fact);
            } else {
                omitted_lines += 1;
            }
        } else {
            omitted_lines += 1;
        }
    }
    LogFacts {
        facts,
        omitted_lines,
        truncated: truncated || bytes.split(|byte| *byte == b'\n').count() > 128,
    }
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case", rename_all_fields = "camelCase")]
enum DiagnosticRead {
    Captured {
        exit_code: Option<i32>,
        stdout: LogFacts,
        stderr: LogFacts,
    },
    Deadline,
    Transport,
    MissingPipe,
}
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ReaderCleanup {
    NotSpawned,
    Reaped,
    Unresolved,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReaderReceipt {
    observation: DiagnosticRead,
    cleanup: ReaderCleanup,
}
async fn cleanup_reader(child: &mut tokio::process::Child) -> ReaderCleanup {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return ReaderCleanup::Reaped;
    }
    // This PID belongs only to the read-only Docker CLI observer. Its exit says
    // nothing about the container, attachment or provider operation outcome.
    let _ = child.start_kill();
    match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
        Ok(Ok(_)) => ReaderCleanup::Reaped,
        _ => ReaderCleanup::Unresolved,
    }
}
async fn observe_reader(child: &mut tokio::process::Child, budget: Duration) -> ReaderReceipt {
    use tokio::io::AsyncReadExt;
    let read = async {
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return DiagnosticRead::MissingPipe;
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut stdout = stdout.take(8193);
        let mut stderr = stderr.take(8193);
        let (a, b) = tokio::join!(stdout.read_to_end(&mut out), stderr.read_to_end(&mut err));
        if a.is_err() || b.is_err() {
            return DiagnosticRead::Transport;
        }
        let truncated = out.len() > 8192 || err.len() > 8192;
        let exit_code = if truncated {
            // Retain the partial read; centralized cleanup owns termination.
            None
        } else {
            let Ok(status) = child.wait().await else {
                return DiagnosticRead::Transport;
            };
            status.code()
        };
        DiagnosticRead::Captured {
            exit_code,
            stdout: log_facts(&out[..out.len().min(8192)], out.len() > 8192),
            stderr: log_facts(&err[..err.len().min(8192)], err.len() > 8192),
        }
    };
    let observation = match tokio::time::timeout(budget, read).await {
        Ok(result) => result,
        Err(_) => DiagnosticRead::Deadline,
    };
    let cleanup = cleanup_reader(child).await;
    ReaderReceipt {
        observation,
        cleanup,
    }
}
async fn selected_companion_logs(
    provider: &Provider,
    identity: &SupervisorIdentity,
) -> ReaderReceipt {
    // Read by the immutable ID admitted immediately before attach.
    let mut command = tokio::process::Command::new("docker");
    command
        .arg("--host")
        .arg(format!("unix://{}", provider.docker_socket().display()))
        .args(["logs", "--tail", "32", &identity.container_id])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    match command.spawn() {
        Ok(mut child) => observe_reader(&mut child, Duration::from_secs(5)).await,
        Err(_) => ReaderReceipt {
            observation: DiagnosticRead::Transport,
            cleanup: ReaderCleanup::NotSpawned,
        },
    }
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryAttachmentReceipt<'a> {
    stage: &'static str,
    gateway_process: Option<u32>,
    reaped_gateway_process: u32,
    sandbox_id: Option<Uuid>,
    main_process_instance_id: Option<Uuid>,
    companion: &'a SupervisorIdentity,
    companion_logs: ReaderReceipt,
    gateway_log: Option<LogFacts>,
}
async fn capture_recovery_attachment(
    provider: &Provider,
    identity: &SupervisorIdentity,
    observation: &Observation,
    controllers: (u32, u32),
) {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::unix::fs::OpenOptionsExt;
    let companion_logs = selected_companion_logs(provider, identity).await;
    let gateway_log = (|| {
        let mut file = std::fs::File::open(provider.dir.join("gateway.log")).ok()?;
        let size = file.metadata().ok()?.len();
        file.seek(SeekFrom::Start(size.saturating_sub(8192))).ok()?;
        let mut bytes = Vec::new();
        file.take(8192).read_to_end(&mut bytes).ok()?;
        Some(log_facts(&bytes, size > 8192))
    })();
    let receipt = RecoveryAttachmentReceipt {
        stage: "replacementGatewayAttach",
        gateway_process: provider.process_id(),
        reaped_gateway_process: controllers.0,
        sandbox_id: observation.sandbox_id.parse().ok(),
        main_process_instance_id: observation.main_process_instance_id.parse().ok(),
        companion: identity,
        companion_logs,
        gateway_log,
    };
    // Never let secondary diagnostic I/O replace the original attach failure.
    if let Ok(bytes) = serde_json::to_vec(&receipt)
        && bytes.len() <= 16384
    {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(provider.dir.join("recovery-attachment-diagnostics.json"))
        {
            let _ = file.write_all(&bytes);
        }
    }
}
#[cfg(test)]
mod recovery_diagnostic_tests {
    use super::*;
    #[test]
    fn session_facts_never_copy_credentials_payloads_or_error_suffixes() {
        let bytes = br#"{"message":"supervisor session failed, reconnecting (attempt 2): Bearer secret-token"}
{"message":"ssh relay open (channel_id=secret, target=private-bytes)"}
{"message":"arbitrary secret payload","authorization":"Bearer secret"}
2026-10-08T05:00:00Z OCSF NET:OPEN private-host [msg:secret]
"#;
        let facts = log_facts(bytes, false);
        assert_eq!(
            facts.facts,
            vec![
                CompanionFact::SessionReconnectFailed,
                CompanionFact::SshRelayOpen,
                CompanionFact::NetworkOpen
            ]
        );
        assert_eq!(facts.omitted_lines, 1);
        let shape = serde_json::to_value(&facts).unwrap();
        assert_eq!(shape["omittedLines"], 1);
        assert!(shape.get("omitted_lines").is_none());
        assert_eq!(
            shape["facts"],
            serde_json::json!(["session_reconnect_failed", "ssh_relay_open", "network_open"])
        );
        let encoded = serde_json::to_string(&facts).unwrap();
        for private in [
            "Bearer",
            "secret",
            "private-host",
            "private-bytes",
            "authorization",
        ] {
            assert!(!encoded.contains(private));
        }
        assert!(log_facts(&vec![b'x'; 8192], true).truncated);
    }
    #[test]
    fn diagnostic_receipt_uses_current_controlled_report_names() {
        let identity = SupervisorIdentity {
            container_id: "a".repeat(64),
            process: ContainerProcessState {
                running: true,
                pid: 42,
                started_at: "2026-10-08T00:00:00Z".into(),
            },
        };
        let receipt = RecoveryAttachmentReceipt {
            stage: "replacementGatewayAttach",
            gateway_process: Some(2),
            reaped_gateway_process: 1,
            sandbox_id: None,
            main_process_instance_id: None,
            companion: &identity,
            companion_logs: ReaderReceipt {
                observation: DiagnosticRead::Captured {
                    exit_code: Some(0),
                    stdout: log_facts(b"", false),
                    stderr: log_facts(b"", false),
                },
                cleanup: ReaderCleanup::Reaped,
            },
            gateway_log: None,
        };
        let value = serde_json::to_value(receipt).unwrap();
        let keys: std::collections::BTreeSet<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "stage",
                "gatewayProcess",
                "reapedGatewayProcess",
                "sandboxId",
                "mainProcessInstanceId",
                "companion",
                "companionLogs",
                "gatewayLog"
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(value["companion"]["containerId"], "a".repeat(64));
        assert!(value["companion"].get("container_id").is_none());
        assert_eq!(value["companion"]["process"]["Running"], true);
        assert_eq!(value["companion"]["process"]["Pid"], 42);
        assert_eq!(
            value["companion"]["process"]["StartedAt"],
            "2026-10-08T00:00:00Z"
        );
        assert_eq!(
            value["companionLogs"]["observation"]["captured"]["exitCode"],
            0
        );
        assert!(
            value["companionLogs"]["observation"]["captured"]
                .get("exit_code")
                .is_none()
        );
        assert_eq!(
            serde_json::to_value(DiagnosticRead::MissingPipe).unwrap(),
            "missing_pipe"
        );
        assert_eq!(
            serde_json::to_value(DiagnosticRead::Deadline).unwrap(),
            "deadline"
        );
        assert_eq!(
            serde_json::to_value(DiagnosticRead::Transport).unwrap(),
            "transport"
        );
    }
    #[tokio::test]
    async fn spawned_reader_missing_pipe_and_deadline_preserve_cause_and_reap() {
        for piped in [false, true] {
            let mut command = tokio::process::Command::new("/bin/sleep");
            command.arg("30").kill_on_drop(true);
            if piped {
                command
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped());
            }
            let mut child = command.spawn().unwrap();
            let receipt = observe_reader(&mut child, Duration::from_millis(20)).await;
            if piped {
                assert!(matches!(receipt.observation, DiagnosticRead::Deadline));
            } else {
                assert!(matches!(receipt.observation, DiagnosticRead::MissingPipe));
            }
            assert_eq!(receipt.cleanup, ReaderCleanup::Reaped);
            assert!(
                child.try_wait().unwrap().is_some(),
                "owned reader is reaped before fixture teardown"
            );
        }
    }
    #[tokio::test]
    async fn completed_reader_preserves_exit_and_redacts_output_before_reap() {
        let mut command = tokio::process::Command::new("/bin/echo");
        command
            .arg("secret arbitrary process payload")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().unwrap();
        let receipt = observe_reader(&mut child, Duration::from_secs(5)).await;
        assert!(matches!(
            receipt.observation,
            DiagnosticRead::Captured {
                exit_code: Some(0),
                ..
            }
        ));
        assert_eq!(receipt.cleanup, ReaderCleanup::Reaped);
        assert!(!serde_json::to_string(&receipt).unwrap().contains("secret"));
    }
}
