//! Actual provider and guest execution, with an isolated retained block home.
#![allow(dead_code)] // Shared fixtures expose scenario-specific operations.
#[path = "native_support/block_home.rs"]
mod block_home;
mod native_support;
#[path = "native_support/template.rs"]
mod template;

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computer_execution::ExecutionRequest;
use veoveo_computers_runtime::*;

#[derive(Default)]
struct Output {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

async fn execute(
    runtime: &OpenShellRuntime,
    binding: &Binding,
    expected: &Observation,
    request: &ExecutionRequest,
    seconds: u32,
) -> (Result<ExecResult>, Output) {
    let output = Arc::new(Mutex::new(Output::default()));
    let sink = output.clone();
    let result = runtime
        .execute_request(
            binding,
            expected,
            request,
            seconds,
            1024 * 1024,
            move |chunk| {
                let mut output = sink.lock().unwrap();
                match chunk.stream {
                    OutputStream::Stdout => output.stdout.extend(chunk.data),
                    OutputStream::Stderr => output.stderr.extend(chunk.data),
                }
                async { Ok(()) }
            },
        )
        .await;
    (
        result,
        Arc::try_unwrap(output).ok().unwrap().into_inner().unwrap(),
    )
}

fn python(program: &str) -> ExecutionRequest {
    ExecutionRequest::new(
        vec!["python3".into(), "-c".into(), program.into()],
        ".".into(),
        BTreeMap::new(),
        vec![],
    )
    .unwrap()
}

async fn initial_shell_uses_retained_home(runtime: &OpenShellRuntime, binding: &Binding) {
    let (_authority, lease) =
        LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30)).unwrap();
    let mut terminal = runtime
        .attach(binding, TerminalSize::new(100, 30).unwrap(), lease)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match terminal
                .read()
                .await
                .unwrap()
                .expect("initial shell stays open")
            {
                TerminalOutput::ReplayComplete => break,
                TerminalOutput::Data(_) => (),
            }
        }
        terminal
            .write(b"printf '\\ninitial-home=%s\\n' \"$PWD\"\r")
            .await
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            if let Some(TerminalOutput::Data(chunk)) = terminal.read().await.unwrap() {
                bytes.extend(chunk);
                assert!(bytes.len() <= 65536);
                if String::from_utf8_lossy(&bytes).contains("initial-home=/sandbox/persistent") {
                    break;
                }
            } else {
                panic!("initial shell ended");
            }
        }
    })
    .await
    .expect("initial retained shell directory");
    terminal.detach().await.unwrap();
}

#[tokio::test]
#[ignore = "requires exact native provider binaries and launcher-bearing image; owns an isolated 512 MiB retained home"]
async fn structured_execution_preserves_values_and_stop_fences_uncertain_descendants() {
    let mut provider = native_support::Provider::start_with_execution_logging().await;
    let runtime = &provider.runtime;
    let selected = template::retained_template(provider.image.clone());
    let computer = Uuid::now_v7();
    let binding = Binding::new(computer, selected.fingerprint()).unwrap();
    let home =
        block_home::BlockHome::create(provider.dir.clone(), provider.image.clone(), computer);
    let create =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), binding.clone()).unwrap();
    let created = runtime.create(&binding, &selected).await.unwrap();
    let ready = runtime
        .wait_for_lifecycle(&create, &created, Duration::from_secs(30))
        .await
        .unwrap();
    home.assert_registered_no_copy();
    initial_shell_uses_retained_home(runtime, &binding).await;

    let setup = python(
        "import os; os.mkdir('private-directory-fixture'); os.symlink('private-directory-fixture', 'current'); os.symlink('/etc', 'escape')",
    );
    let (result, output) = execute(runtime, &binding, &ready, &setup, 10).await;
    assert_eq!(
        result.unwrap().exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request = ExecutionRequest::new(
        vec!["python3".into(), "-c".into(), "import os,sys; assert os.getuid()==os.getgid()==10001; assert os.getcwd()=='/sandbox/persistent/private-directory-fixture'; assert sys.argv[1:]==['',\"private 'quoted' argument\"]; assert os.environ['FIXTURE_VALUE']=='private-environment-fixture\\nlast'; sys.stdout.buffer.write(sys.stdin.buffer.read()); sys.stderr.write('stderr-fixture'); sys.exit(23)".into(), String::new(), "private 'quoted' argument".into()],
        "current".into(),
        BTreeMap::from([("FIXTURE_VALUE".into(), "private-environment-fixture\nlast".into())]),
        (0..100_000).map(|index| (index % 256) as u8).collect(),
    ).unwrap();
    let (result, output) = execute(runtime, &binding, &ready, &request, 10).await;
    assert_eq!(
        result.unwrap().exit_code,
        23,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.stdout,
        (0..100_000)
            .map(|index| (index % 256) as u8)
            .collect::<Vec<_>>()
    );
    assert_eq!(output.stderr, b"stderr-fixture");
    let escape = ExecutionRequest::new(
        vec!["/bin/echo".into(), "unexpected".into()],
        "escape".into(),
        BTreeMap::new(),
        vec![],
    )
    .unwrap();
    let (result, output) = execute(runtime, &binding, &ready, &escape, 10).await;
    assert_eq!(result.unwrap().exit_code, 125);
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"Computer execution request could not be launched\n"
    );

    // A detached descendant outlives SSH channel loss. The native Stop boundary
    // must fence the entire Computer before a domain can settle cancellation.
    let runaway = python(
        r#"import os,sys,time
from pathlib import Path
Path('heartbeat').write_text('0')
if os.fork()==0:
    os.setsid()
    fd=os.open('/dev/null', os.O_RDWR)
    for target in (0,1,2): os.dup2(fd,target)
    os.close(fd)
    counter=0
    while True:
        counter+=1
        Path('heartbeat').write_text(str(counter))
        time.sleep(0.05)
while True: time.sleep(0.1)
"#,
    );
    let (result, _) = execute(runtime, &binding, &ready, &runaway, 1).await;
    assert_eq!(result, Err(RuntimeFailure::ExecutionUnknown));
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
    let marker = python(
        "from pathlib import Path; Path('rejected-run-marker').write_text('should-not-run')",
    );
    let (result, _) = execute(runtime, &binding, &ready, &marker, 10).await;
    assert_eq!(result, Err(RuntimeFailure::BindingMismatch));
    let (result, output) = execute(runtime, &binding, &restarted, &python("import time; from pathlib import Path; assert not Path('rejected-run-marker').exists(); p=Path('heartbeat'); value=p.read_text(); assert int(value)>0; time.sleep(0.3); assert p.read_text()==value; print('descendant-fenced')"), 10).await;
    assert_eq!(
        result.unwrap().exit_code,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"descendant-fenced\n");

    let log = std::fs::read_to_string(provider.dir.join("gateway.log")).unwrap();
    assert!(
        log.contains("command_preview") && log.contains("veoveo-computer-exec"),
        "execution logging must actually be enabled"
    );
    for value in [
        "private-directory-fixture",
        "private-environment-fixture",
        "private 'quoted' argument",
        "stderr-fixture",
        "descendant-fenced",
    ] {
        assert!(
            !log.contains(value),
            "request/output fixture leaked into provider log"
        );
    }
    std::fs::write(provider.dir.join("execution-result.txt"), "packaged launcher under native confinement; exact argv/environment and 100000 binary input bytes; internal symlink and escape refusal; timeout remains uncertain; Stop fences detached descendant; retained file survives restart; info-level command logs contain only fixed launcher\n").unwrap();
    home.finish();
    provider.assert_running();
}
