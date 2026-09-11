//! Actual guest launcher acceptance; owns only the containers it creates by ID.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::fs::{PermissionsExt, symlink},
    process::{Command, Output, Stdio},
};

use serde::Deserialize;
use veoveo_computer_execution::{ExecutionRequest, LAUNCHER_PATH, RETAINED_HOME};

fn docker(arguments: &[&str], input: Option<&[u8]>) -> Output {
    let mut child = Command::new("timeout")
        .args(["--kill-after=2", "25", "docker"])
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}

struct Container(String);
impl Drop for Container {
    fn drop(&mut self) {
        let _ = docker(&["rm", "--force", &self.0], None);
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    status: String,
    running: bool,
    exit_code: i32,
}

fn execute(image: &str, home: &std::path::Path, user: &str, input: &[u8]) -> (State, Output) {
    execute_mode(image, home, user, input, false)
}

fn execute_mode(
    image: &str,
    home: &std::path::Path,
    user: &str,
    input: &[u8],
    files: bool,
) -> (State, Output) {
    let container = create_fixture(image, home, user, files);
    let output = docker(
        &["start", "--attach", "--interactive", &container.0],
        Some(input),
    );
    let inspected = docker(
        &["inspect", "--format", "{{json .State}}", &container.0],
        None,
    );
    assert!(inspected.status.success());
    let state: State = serde_json::from_slice(&inspected.stdout).unwrap();
    assert_eq!(state.status, "exited");
    assert!(!state.running);
    (state, output)
}

fn create_fixture(image: &str, home: &std::path::Path, user: &str, files: bool) -> Container {
    let binary = fs::canonicalize(env!("CARGO_BIN_EXE_veoveo-computer-exec")).unwrap();
    let binary = format!(
        "type=bind,src={},dst={LAUNCHER_PATH},readonly",
        binary.display()
    );
    let home = format!("type=bind,src={},dst={RETAINED_HOME}", home.display());
    let mut arguments = vec![
        "create",
        "--pull=never",
        "--interactive",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,nosuid,nodev,noexec,size=4m",
        "--network=none",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--memory=128m",
        "--pids-limit=32",
        "--user",
        user,
        "--workdir",
        RETAINED_HOME,
        "--mount",
        &binary,
        "--mount",
        &home,
        "--entrypoint",
        "/usr/bin/env",
        image,
        LAUNCHER_PATH,
    ];
    if files {
        arguments.push("--files");
    }
    let created = docker(&arguments, None);
    assert!(
        created.status.success(),
        "creating owned fixture: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let id = String::from_utf8(created.stdout).unwrap().trim().to_owned();
    assert!(id.len() == 64 && id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    Container(id)
}

#[test]
#[ignore = "requires Docker and VEOVEO_COMPUTERS_EXEC_IMAGE pinned by digest; runs owned disposable containers"]
fn actual_guest_preserves_argv_environment_stdin_and_exit_while_rejecting_escape() {
    let image = std::env::var("VEOVEO_COMPUTERS_EXEC_IMAGE")
        .expect("set the qualified local image by digest");
    let (_, digest) = image
        .rsplit_once("@sha256:")
        .expect("image must be pinned by digest");
    assert!(digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let home = tempfile::tempdir().unwrap();
    // Only synthetic fixture data lives here; the selected container user is 10001.
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o777)).unwrap();
    fs::create_dir(home.path().join("project")).unwrap();
    fs::set_permissions(
        home.path().join("project"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    symlink("project", home.path().join("current")).unwrap();
    symlink("/etc", home.path().join("escape")).unwrap();
    let script = "import os,sys; assert os.getuid()==os.getgid()==10001; assert os.getcwd()=='/sandbox/persistent/project'; assert sys.argv[1:]==['',\"a 'quoted' value\"]; assert os.environ['VEOVEO_FIXTURE_VALUE']=='first\\nlast'; sys.stdout.buffer.write(sys.stdin.buffer.read()); sys.stderr.write('stderr-marker'); sys.exit(23)";
    let request = ExecutionRequest::new(
        vec![
            "python3".into(),
            "-c".into(),
            script.into(),
            String::new(),
            "a 'quoted' value".into(),
        ],
        "current".into(),
        BTreeMap::from([("VEOVEO_FIXTURE_VALUE".into(), "first\nlast".into())]),
        vec![0, 255, 13, 10],
    )
    .unwrap()
    .encode()
    .unwrap();
    let (state, output) = execute(&image, home.path(), "10001:10001", &request);
    assert_eq!(
        state.exit_code,
        23,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, [0, 255, 13, 10]);
    assert_eq!(output.stderr, b"stderr-marker");

    let escaped = ExecutionRequest::new(
        vec!["/bin/echo".into(), "unexpected".into()],
        "escape".into(),
        BTreeMap::new(),
        vec![],
    )
    .unwrap()
    .encode()
    .unwrap();
    for (user, frame) in [
        ("10001:10001", escaped.as_slice()),
        ("10002:10002", request.as_slice()),
    ] {
        let (state, output) = execute(&image, home.path(), user, frame);
        assert_eq!(state.exit_code, 125);
        assert!(output.stdout.is_empty());
        assert_eq!(
            output.stderr,
            b"Computer execution request could not be launched\n"
        );
    }
}

#[path = "support/files.rs"]
mod files;
