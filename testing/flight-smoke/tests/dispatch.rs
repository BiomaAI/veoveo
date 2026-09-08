//! Measure command-entry-to-validation through the real xtask build selection.
//! The deliberately absent scenario stops before Kubernetes, OAuth or GPU work.
use std::{path::Path, process::Command, time::Instant};

#[allow(dead_code)]
#[path = "../../../tools/xtask/src/process.rs"]
mod cargo_process;

#[test]
#[ignore = "build timing: run explicitly on a warm, otherwise idle developer host"]
fn warm_flight_dispatch_stays_under_two_seconds() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let missing = std::env::temp_dir().join(format!(
        "veoveo-absent-scenario-{}.json",
        uuid::Uuid::new_v4()
    ));
    assert!(!missing.exists());
    let mut timings = Vec::new();
    for attempt in 0..4 {
        let start = Instant::now();
        let mut command = Command::new(env!("CARGO"));
        command
            .current_dir(&repository)
            .args(["xtask", "smoke", "uav-showcase-up", "--scenario"])
            .arg(&missing)
            .args([
                "--context",
                "veoveo-dispatch-no-cluster",
                "--public-base-url",
                "https://dispatch.invalid",
            ]);
        // A Cargo test receives package-scoped environment variables. Forwarding
        // them would measure build-script invalidation caused by the test itself.
        cargo_process::remove_parent_cargo_package_environment(&mut command);
        let output = command.output().unwrap();
        let elapsed = start.elapsed();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(
            !output.status.success(),
            "the nonexistent scenario must reject dispatch"
        );
        assert!(
            stderr.contains("reading UAV acceptance scenario")
                && stderr.contains(&missing.to_string_lossy().to_string()),
            "unexpected failure: {stderr}"
        );
        if attempt > 0 {
            assert!(
                !stderr.contains("Compiling "),
                "warm dispatch unexpectedly compiled: {stderr}"
            );
            timings.push(elapsed.as_secs_f64());
        }
    }
    let receipt = serde_json::json!({
        "schema": "veoveo.io/flight-dispatch-timing/v1",
        "command": "cargo xtask smoke uav-showcase-up --scenario <absent> --context veoveo-dispatch-no-cluster --public-base-url https://dispatch.invalid",
        "warmSeconds": timings,
        "assertion": "typed scenario rejection before Kubernetes or authentication",
        "gpuWorkflowVerified": false,
    });
    let directory = repository.join("output/development/flight-iteration");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("dispatch.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    println!("{receipt}");
    assert!(
        timings.iter().all(|seconds| *seconds < 2.0),
        "warm dispatch exceeded two seconds: {timings:?}"
    );
}
