#[path = "native_support/block_home.rs"]
mod block_home;
mod native_support;
use block_home::BlockHome;
use native_support::Provider;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computers_runtime::*;
#[path = "native_support/template.rs"]
mod template;
use template::retained_template;

async fn ready(
    runtime: &OpenShellRuntime,
    binding: &Binding,
    template: &DevelopmentTemplate,
) -> Observation {
    let create =
        LifecycleCheckpoint::create(Uuid::from_u128(100), Uuid::now_v7(), binding.clone()).unwrap();
    let created = runtime.create(binding, template).await.unwrap();
    runtime
        .wait_for_lifecycle(&create, &created, Duration::from_secs(30))
        .await
        .unwrap()
}
async fn python(runtime: &OpenShellRuntime, binding: &Binding, program: &str) -> String {
    let intent = ExecIntent::new(
        vec!["/usr/bin/python3".into(), "-c".into(), program.into()],
        PERSISTENT_HOME.into(),
        30,
        65536,
        vec![],
    )
    .unwrap();
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let output = bytes.clone();
    let result = runtime
        .execute(binding, &intent, move |chunk| {
            output.lock().unwrap().extend(chunk.data);
            async { Ok(()) }
        })
        .await
        .unwrap();
    assert_eq!(
        result.exit_code,
        0,
        "native exec output: {}",
        String::from_utf8_lossy(&bytes.lock().unwrap())
    );
    String::from_utf8(bytes.lock().unwrap().clone()).unwrap()
}
#[tokio::test]
#[ignore = "requires exact native provider binaries/image; owns isolated privileged loop-device helpers and a 512 MiB ext4 volume"]
async fn native_retention_enospc_and_offline_restore() {
    let mut provider = Provider::start().await;
    let runtime = &provider.runtime;
    let computer = Uuid::now_v7();
    let template = retained_template(provider.image.clone());
    let binding = Binding::new(computer, template.fingerprint()).unwrap();
    let mut home = BlockHome::create(provider.dir.clone(), provider.image.clone(), computer);
    let original = ready(runtime, &binding, &template).await;
    assert_eq!(
        python(
            runtime,
            &binding,
            r#"from pathlib import Path
assert Path('/etc/passwd').is_file()
for target in ['/sandbox/outside-home', '/etc/outside-home', '/run/openshell/outside-home']:
    try:
        Path(target).write_text('must not escape')
    except (PermissionError, FileNotFoundError):
        continue
    raise AssertionError('write escaped the admitted home')
assert not Path('/var/run/docker.sock').exists()
print('filesystem boundaries enforced')
"#
        )
        .await
        .trim(),
        "filesystem boundaries enforced"
    );
    assert_eq!(python(runtime, &binding, "import os; from pathlib import Path; assert os.getuid()==10001; Path('retained.txt').write_text('same-computer'); print('written')").await.trim(), "written");
    let quota = python(
        runtime,
        &binding,
        r#"import errno, os
from pathlib import Path
written=0
try:
    with open('fill.bin', 'wb', buffering=0) as f:
        chunk=b'x'*(1024*1024)
        for _ in range(600):
            written += f.write(chunk)
        os.fsync(f.fileno())
    raise AssertionError('capacity was not enforced')
except OSError as e:
    assert e.errno == errno.ENOSPC, e
    assert 400*1024*1024 < written < 512*1024*1024, written
finally:
    Path('fill.bin').unlink(missing_ok=True)
assert Path('retained.txt').read_text() == 'same-computer'
print('ENOSPC enforced; original file retained')
"#,
    )
    .await;
    assert!(quota.contains("ENOSPC enforced"));
    let stop = LifecycleCheckpoint::stop(
        Uuid::from_u128(100),
        Uuid::now_v7(),
        binding.clone(),
        &original,
    )
    .unwrap();
    let stopping = runtime.stop(&binding, &original).await.unwrap();
    runtime
        .wait_for_lifecycle(&stop, &stopping, Duration::from_secs(30))
        .await
        .unwrap();
    home.backup_restore();
    let replacement =
        Binding::replacement(computer, Uuid::now_v7(), template.fingerprint()).unwrap();
    let restored = ready(runtime, &replacement, &template).await;
    assert_ne!(original.sandbox_id, restored.sandbox_id);
    assert_ne!(
        original.main_process_instance_id,
        restored.main_process_instance_id
    );
    assert_eq!(python(runtime, &replacement, "import os; from pathlib import Path; assert os.getuid()==10001; print(Path('retained.txt').read_text()); assert not Path('fill.bin').exists()").await.trim(), "same-computer");
    std::fs::write(provider.dir.join("retention-result.txt"), "native 512 MiB ext4 capacity, ENOSPC, retained file, physical source-container removal, offline block backup/restore, new resource/process and preserved uid passed\n").unwrap();
    home.finish();
    provider.assert_running();
}
