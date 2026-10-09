#[path = "support/cleanup.rs"]
mod cleanup;
#[path = "support/faults.rs"]
mod faults;
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
async fn stop(
    runtime: &OpenShellRuntime,
    provider: veoveo_computers_runtime::ProviderInstanceId,
    binding: &Binding,
) -> Result<()> {
    let before = runtime.get(binding).await?.unwrap();
    let checkpoint = LifecycleCheckpoint::stop(
        provider,
        LifecycleOperationId::new(),
        binding.clone(),
        &before,
    )?;
    let initial = runtime.stop(binding, &before).await?;
    runtime
        .wait_for_lifecycle(&checkpoint, &initial, Duration::from_secs(30))
        .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires local candidate host image, reachable digest-pinned Computer image, privileged Docker and ext4; owns all fixture state"]
async fn composite_host_replaces_its_namespace_and_retains_the_computer() -> Result<()> {
    exercise(fixture::HostReplacementProfile::ImageUpgrade).await
}

#[tokio::test]
#[ignore = "requires stock host image, reachable digest-pinned Computer image, privileged Docker and ext4; owns all fixture state"]
async fn composite_host_restarts_stock_image_and_retains_the_computer() -> Result<()> {
    exercise(fixture::HostReplacementProfile::Restart).await
}

async fn exercise(profile: fixture::HostReplacementProfile) -> Result<()> {
    if let Ok(mode) = std::env::var("VEOVEO_HOST_PROBE_FAULT") {
        return faults::run(&mode);
    }
    if std::env::var_os("VEOVEO_HOST_PROBE_CLEANUP").is_some() {
        cleanup::cleanup();
        return Ok(());
    }
    let images = fixture::FixtureImages::admit().await?;
    let template = template::retained_template(images.template_image().to_owned());
    let mut fixture = fixture::Fixture::start(&template, &images, profile).await?;
    fixture.assert_template_home_absent(&images).await?;
    let engine = fixture.engine().await?;
    let runtime = fixture.runtime().await?;
    ensure!(
        fixture.runtime_in("not-provisioned").await.is_err(),
        "a missing provider workspace advertised ready capacity"
    );
    fixture.assert_provider_security().await?;
    let allocator = fixture.allocator(&template).await?;
    allocator.ready().await?;
    let binding = Binding::new(Uuid::now_v7(), template.fingerprint())?;
    fixture
        .fault("hide-loop-nodes", binding.computer_id())
        .await?;
    allocator.prepare(&binding).await?;
    fixture
        .fault("interrupt-allocation", binding.computer_id())
        .await?;
    allocator.prepare(&binding).await?;
    fixture
        .fault("assert-recovery", binding.computer_id())
        .await?;
    let create = LifecycleCheckpoint::create(
        fixture.provider,
        LifecycleOperationId::new(),
        binding.clone(),
    )?;
    let initial = runtime.create(&binding, &template).await?;
    let original = runtime
        .wait_for_lifecycle(&create, &initial, Duration::from_secs(30))
        .await?;
    exec(&runtime, &binding, "import os; from pathlib import Path; assert os.getuid()==10001; assert Path('allocation-marker').read_bytes()==b'retained before Ready acknowledgement', 'stock Create changed allocator-seeded bytes'; Path('retained.txt').write_text('same retained Computer'); assert not Path('/run/veoveo-computers').exists(); assert not Path('/var/run/docker.sock').exists()").await?;
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
    fixture.assert_provider_security().await?;
    let start = LifecycleCheckpoint::start(
        fixture.provider,
        LifecycleOperationId::new(),
        binding.clone(),
        &before,
    )?;
    let initial = runtime.start(&binding, &before).await?;
    let restored = runtime
        .wait_for_lifecycle(&start, &initial, Duration::from_secs(30))
        .await?;
    ensure!(
        restored.main_process_instance_id != original.main_process_instance_id,
        "Start reused the old process"
    );
    exec(&runtime, &binding, "import os; from pathlib import Path; assert os.getuid()==10001; assert Path('allocation-marker').read_bytes()==b'retained before Ready acknowledgement', 'stock Start changed allocator-seeded bytes'; assert Path('retained.txt').read_text()=='same retained Computer'").await?;
    fixture
        .fault("limits-before", binding.computer_id())
        .await?;
    exec(&runtime, &binding, "import multiprocessing as m, time\ndef burn():\n end=time.monotonic()+3\n while time.monotonic()<end: pass\njobs=[m.get_context('fork').Process(target=burn) for _ in range(4)]\nfor job in jobs: job.start()\nfor job in jobs: job.join()\nassert all(job.exitcode==0 for job in jobs)").await?;
    fixture.fault("limits-after", binding.computer_id()).await?;
    stop(&runtime, fixture.provider, &binding).await?;
    std::fs::write(
        fixture.dir.join("result.txt"),
        "Composite Host profile, private network/mount namespace replacement, stable Docker/provider identity, absent image mount target, allocator-seeded bytes unchanged across stock Create/Start, retained bytes, new process and guest user-authority denial passed.\n",
    )?;
    fixture.finish().await
}
