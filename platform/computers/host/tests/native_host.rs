#[path = "support/cleanup.rs"]
mod cleanup;
#[path = "support/fixture.rs"]
mod fixture;
#[path = "../../../runtimes/computers/tests/native_support/guest_authority.rs"]
mod guest_authority;
#[path = "../../../runtimes/computers/tests/native_support/template.rs"]
mod template;
#[path = "support/trust.rs"]
mod trust;

use anyhow::{Result, ensure};
use std::time::Duration;
use uuid::Uuid;
use veoveo_computers_runtime::*;

async fn exec(runtime: &OpenShellRuntime, binding: &Binding, program: &str) -> Result<()> {
    let intent = ExecIntent::new(
        vec!["/usr/bin/python3".into(), "-c".into(), program.into()],
        PERSISTENT_HOME.into(),
        15,
        65536,
        vec![],
    )?;
    let result = runtime
        .execute(binding, &intent, |chunk| async move {
            eprint!("{}", String::from_utf8_lossy(&chunk.data));
            Ok(())
        })
        .await?;
    ensure!(result.exit_code == 0, "native Computer exec failed");
    Ok(())
}
async fn stop(runtime: &OpenShellRuntime, provider: Uuid, binding: &Binding) -> Result<()> {
    let before = runtime.get(binding).await?.unwrap();
    let checkpoint = LifecycleCheckpoint::stop(provider, Uuid::now_v7(), binding.clone(), &before)?;
    let initial = runtime.stop(binding, &before).await?;
    runtime
        .wait_for_lifecycle(&checkpoint, &initial, Duration::from_secs(30))
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires local candidate host image, reachable digest-pinned Computer image, privileged Docker and ext4; owns all fixture state"]
async fn composite_host_replaces_its_namespace_and_retains_the_computer() -> Result<()> {
    if std::env::var_os("VEOVEO_HOST_PROBE_CLEANUP").is_some() {
        cleanup::cleanup();
        return Ok(());
    }
    let image = std::env::var("VEOVEO_COMPUTERS_HOST_TEST_IMAGE")?;
    let template = template::retained_template(image.clone());
    let mut fixture = fixture::Fixture::start(&template, &image).await?;
    let engine = fixture.engine().await?;
    let runtime = fixture.runtime().await?;
    ensure!(
        fixture.runtime_in("not-provisioned").await.is_err(),
        "a missing provider workspace advertised ready capacity"
    );
    guest_authority::assert_denied(&fixture.dir.join("provider"), &fixture.endpoint).await;
    let allocator = fixture.allocator(&template).await?;
    allocator.ready().await?;
    let binding = Binding::new(Uuid::now_v7(), template.fingerprint())?;
    allocator.prepare(&binding).await?;
    let create = LifecycleCheckpoint::create(fixture.provider, Uuid::now_v7(), binding.clone())?;
    let initial = runtime.create(&binding, &template).await?;
    let original = runtime
        .wait_for_lifecycle(&create, &initial, Duration::from_secs(30))
        .await?;
    exec(&runtime, &binding, "import os; from pathlib import Path; assert os.getuid()==10001; Path('retained.txt').write_text('same retained Computer'); assert not Path('/run/veoveo-computers').exists(); assert not Path('/var/run/docker.sock').exists()").await?;
    stop(&runtime, fixture.provider, &binding).await?;
    drop(runtime);
    drop(allocator);
    fixture.replace().await?;
    ensure!(
        fixture.engine().await? == engine,
        "host replacement changed retained Docker identity"
    );
    let runtime = fixture.runtime().await?;
    let allocator = fixture.allocator(&template).await?;
    allocator.restore(&binding).await?;
    let before = runtime.get(&binding).await?.unwrap();
    ensure!(
        before.phase == Phase::Stopped && before.sandbox_id == original.sandbox_id,
        "retained provider resource changed"
    );
    let start =
        LifecycleCheckpoint::start(fixture.provider, Uuid::now_v7(), binding.clone(), &before)?;
    let initial = runtime.start(&binding, &before).await?;
    let restored = runtime
        .wait_for_lifecycle(&start, &initial, Duration::from_secs(30))
        .await?;
    ensure!(
        restored.main_process_instance_id != original.main_process_instance_id,
        "Start reused the old process"
    );
    exec(&runtime, &binding, "import os; from pathlib import Path; assert os.getuid()==10001; assert Path('retained.txt').read_text()=='same retained Computer'").await?;
    stop(&runtime, fixture.provider, &binding).await?;
    std::fs::write(
        fixture.dir.join("result.txt"),
        "Exact composite OCI image, private network/mount namespace replacement, stable Docker/provider identity, retained bytes, new process and guest user-authority denial passed.\n",
    )?;
    fixture.finish().await
}
