//! Actual maintained frameworks qualify the selector, independently of report-model admission.
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::Duration,
};
use veoveo_testing_support::{
    framework::{Framework, FrameworkOutcome},
    lifecycle::OwnedProcess,
};
#[test]
#[ignore = "requires Node and the locked independent-fixture Python dev environment; no services or graphics"]
fn native_frameworks_refuse_missing_failed_and_skipped_selection() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let fixtures = repository.join("testing/support/framework");
    let temporary = tempfile::tempdir().unwrap();
    for framework in [Framework::Node, Framework::Pytest] {
        for case in ["passed_case", "skipped_case", "failed_case", "missing_case"] {
            let lease = tempfile::tempdir_in(temporary.path()).unwrap();
            let groups = lease.path().join("groups");
            std::fs::create_dir(&groups).unwrap();
            let report = lease.path().join("outcome.json");
            let (mut command, expected) = match framework {
                Framework::Node => {
                    let mut command = Command::new("node");
                    command
                        .arg(fixtures.join("node.mjs"))
                        .arg(fixtures.join("cases.test.mjs"))
                        .arg(case)
                        .arg(&report);
                    (command, case.to_owned())
                }
                Framework::Pytest => {
                    let expected = format!("{}::test_{case}", fixtures.join("cases.py").display());
                    let mut command = Command::new("uv");
                    command
                        .args(["run", "--locked", "--extra", "dev", "--project"])
                        .arg(repository.join("testing/fixtures/fork-workload"))
                        .args(["python"])
                        .arg(fixtures.join("run_pytest.py"))
                        .arg(&expected)
                        .arg(&report);
                    (command, expected)
                }
                Framework::Libtest => unreachable!(),
            };
            command
                .current_dir(&repository)
                .stdout(Stdio::from(
                    std::fs::File::create(lease.path().join("stdout")).unwrap(),
                ))
                .stderr(Stdio::from(
                    std::fs::File::create(lease.path().join("stderr")).unwrap(),
                ));
            let status = OwnedProcess::spawn(
                command,
                &groups,
                std::time::Instant::now() + Duration::from_secs(30),
                &AtomicBool::new(false),
            )
            .unwrap()
            .finish(
                Duration::from_secs(30),
                Duration::from_secs(2),
                &AtomicBool::new(false),
            )
            .unwrap();
            if case == "passed_case" {
                assert!(
                    status.success(),
                    "{framework:?} positive selector failed; private diagnostics at {}",
                    lease.path().display()
                );
                FrameworkOutcome::read(&report, framework, &expected).unwrap();
            } else {
                assert!(!status.success(), "{framework:?} accepted {case}");
                assert!(FrameworkOutcome::read(&report, framework, &expected).is_err());
            }
        }
    }
}
