mod native_support;
use native_support::Provider;
use std::time::{Duration, SystemTime};
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
    let created = runtime.create(&binding, &template).await.unwrap();
    let ready = runtime
        .wait_for(&binding, &created, Phase::Ready)
        .await
        .unwrap();
    assert!(!ready.main_process_instance_id.is_empty());
    let mut terminal = runtime
        .attach(
            &binding,
            TerminalSize::new(100, 30).unwrap(),
            SystemTime::now() + Duration::from_secs(120),
        )
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
        .attach(
            &binding,
            TerminalSize::new(100, 30).unwrap(),
            SystemTime::now() + Duration::from_secs(120),
        )
        .await
        .unwrap();
    assert_eq!(
        terminal.main_process_instance_id(),
        ready.main_process_instance_id
    );
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
    let stopping = runtime.stop(&binding).await.unwrap();
    let stopped = runtime
        .wait_for(&binding, &stopping, Phase::Stopped)
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
    let starting = runtime.start(&binding).await.unwrap();
    let restarted = runtime
        .wait_for(&binding, &starting, Phase::Ready)
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
