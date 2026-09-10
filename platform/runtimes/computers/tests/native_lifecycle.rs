mod native_support;
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
    let mut provider = Provider::start().await;
    let runtime = &provider.runtime;
    let template = template(provider.image.clone());
    let binding = Binding::new(Uuid::now_v7(), template.fingerprint()).unwrap();
    let create =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), binding.clone()).unwrap();
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
    terminal.detach().await.unwrap();
    let stop = LifecycleCheckpoint::stop(
        Uuid::from_u128(100),
        Uuid::now_v7(),
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
        Uuid::from_u128(100),
        Uuid::now_v7(),
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
    std::fs::write(provider.dir.join("result.txt"), "native create, terminal replay, retained shell, uid, stop/start and reconciled epoch passed\n").unwrap();
    provider.assert_running();
}

#[tokio::test]
#[ignore = "requires exact provider binaries and native Computer image; exercises renewable Veoveo terminal authority"]
async fn native_terminal_renews_without_reconnecting_and_revokes_access() {
    let (mut provider, _) = Provider::start_with_session_ttl(3).await;
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
