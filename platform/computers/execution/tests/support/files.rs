use super::*;

#[test]
#[ignore = "requires Docker and VEOVEO_COMPUTERS_EXEC_IMAGE pinned by digest; races only owned fixture paths"]
fn import_cannot_publish_through_a_parent_moved_outside_the_retained_home() {
    use sha2::{Digest, Sha256};
    use veoveo_computer_execution::{FileFailure, FileReceipt, FileRequest};
    let image = std::env::var("VEOVEO_COMPUTERS_EXEC_IMAGE").unwrap();
    assert!(image.contains("@sha256:"));
    let base = tempfile::tempdir().unwrap();
    let home = base.path().join("home");
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    for path in [&home, &project] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o777)).unwrap();
    }
    let container = create_fixture(&image, &home, "10001:10001", true);
    let data = vec![47u8; 1024 * 1024];
    let request = FileRequest::import(
        "project/result".into(),
        data.len() as u64,
        Sha256::digest(&data).into(),
    )
    .unwrap();
    let mut child = Command::new("timeout")
        .args([
            "--kill-after=2",
            "25",
            "docker",
            "start",
            "--attach",
            "--interactive",
            &container.0,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(&request.encode().unwrap()).unwrap();
    input.write_all(&data[..65536]).unwrap();
    input.flush().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let fds = docker(
            &[
                "exec",
                "--user",
                "10001:10001",
                &container.0,
                "/bin/sh",
                "-c",
                "for fd in /proc/1/fd/*; do readlink \"$fd\"; done",
            ],
            None,
        );
        if String::from_utf8_lossy(&fds.stdout)
            .lines()
            .any(|p| p.contains("/project/#") && p.ends_with(" (deleted)"))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "helper did not open its anonymous allocation"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // The adversary moves the already-open parent beyond the fixed bind mount,
    // then replaces its original name. Neither directory may receive publication.
    let outside = base.path().join("outside");
    fs::rename(&project, &outside).unwrap();
    fs::create_dir(&project).unwrap();
    fs::set_permissions(&project, fs::Permissions::from_mode(0o777)).unwrap();
    input.write_all(&data[65536..]).unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    let result: Result<FileReceipt, FileFailure> = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(result, Err(FileFailure::FileChanged));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_dir(&project).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[test]
#[ignore = "requires Docker and VEOVEO_COMPUTERS_EXEC_IMAGE pinned by digest; runs owned disposable containers"]
fn actual_file_helper_enforces_atomic_import_integrity_and_confined_export() {
    use sha2::{Digest, Sha256};
    use veoveo_computer_execution::{FileFailure, FileReceipt, FileRequest};

    let image = std::env::var("VEOVEO_COMPUTERS_EXEC_IMAGE").unwrap();
    let (_, digest) = image.rsplit_once("@sha256:").expect("image must be pinned");
    assert!(digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()));
    let home = tempfile::tempdir().unwrap();
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o777)).unwrap();
    fs::create_dir(home.path().join("project")).unwrap();
    fs::set_permissions(
        home.path().join("project"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let run = |request: FileRequest, body: &[u8]| {
        let mut input = request.encode().unwrap();
        input.extend_from_slice(body);
        let (state, output) = execute_mode(&image, home.path(), "10001:10001", &input, true);
        let result: Result<FileReceipt, FileFailure> = serde_json::from_slice(&output.stderr)
            .unwrap_or_else(|_| {
                panic!(
                    "invalid bounded file result: {}",
                    String::from_utf8_lossy(&output.stderr)
                )
            });
        (state.exit_code, output.stdout, result)
    };
    let bytes: Vec<u8> = (0..1_000_003).map(|i| (i % 251) as u8).collect();
    let hash: [u8; 32] = Sha256::digest(&bytes).into();
    let (code, out, result) = run(
        FileRequest::import("project/data.bin".into(), bytes.len() as u64, hash).unwrap(),
        &bytes,
    );
    assert_eq!(code, 0, "{result:?}");
    assert!(out.is_empty());
    assert_eq!(
        result.unwrap(),
        FileReceipt {
            bytes: bytes.len() as u64,
            sha256: hash
        }
    );
    let (code, exported, result) = run(
        FileRequest::export("project/data.bin".into(), bytes.len() as u64).unwrap(),
        &[],
    );
    assert_eq!(code, 0, "{result:?}");
    assert_eq!(exported, bytes);
    assert_eq!(result.unwrap().sha256, hash);
    let (_, out, result) = run(
        FileRequest::import("project/data.bin".into(), bytes.len() as u64, hash).unwrap(),
        &bytes,
    );
    assert!(out.is_empty());
    assert_eq!(result, Err(FileFailure::DestinationExists));
    let (_, out, result) = run(
        FileRequest::import("bad-digest".into(), bytes.len() as u64, [0; 32]).unwrap(),
        &bytes,
    );
    assert!(out.is_empty());
    assert_eq!(result, Err(FileFailure::Integrity));
    assert!(!home.path().join("bad-digest").exists());
    let (_, _, result) = run(
        FileRequest::import("partial".into(), bytes.len() as u64, hash).unwrap(),
        &bytes[..31],
    );
    assert_eq!(result, Err(FileFailure::InputIncomplete));
    assert!(!home.path().join("partial").exists());
    let (_, out, result) = run(
        FileRequest::export("project/data.bin".into(), 1).unwrap(),
        &[],
    );
    assert!(out.is_empty());
    assert_eq!(result, Err(FileFailure::TooLarge));
    symlink("/etc", home.path().join("escape")).unwrap();
    let (_, out, result) = run(
        FileRequest::export("escape/passwd".into(), 1024).unwrap(),
        &[],
    );
    assert!(out.is_empty());
    assert_eq!(result, Err(FileFailure::PathUnavailable));
    let (_, _, result) = run(
        FileRequest::import("escape/new".into(), 0, Sha256::digest([]).into()).unwrap(),
        &[],
    );
    assert_eq!(result, Err(FileFailure::PathUnavailable));
    nix::unistd::mkfifo(
        &home.path().join("pipe"),
        nix::sys::stat::Mode::S_IRWXU
            | nix::sys::stat::Mode::S_IRWXG
            | nix::sys::stat::Mode::S_IRWXO,
    )
    .unwrap();
    fs::set_permissions(home.path().join("pipe"), fs::Permissions::from_mode(0o777)).unwrap();
    let (_, out, result) = run(FileRequest::export("pipe".into(), 1024).unwrap(), &[]);
    assert!(out.is_empty());
    assert_eq!(result, Err(FileFailure::UnsupportedFile));
    fs::write(home.path().join("source"), b"a").unwrap();
    fs::hard_link(home.path().join("source"), home.path().join("hardlink")).unwrap();
    let (_, out, result) = run(FileRequest::export("hardlink".into(), 1024).unwrap(), &[]);
    assert!(out.is_empty());
    assert_eq!(result, Err(FileFailure::UnsupportedFile));
    let request = FileRequest::export("source".into(), 1)
        .unwrap()
        .encode()
        .unwrap();
    let (state, output) = execute_mode(&image, home.path(), "0:0", &request, true);
    assert_eq!(state.exit_code, 125);
    assert_eq!(
        serde_json::from_slice::<Result<FileReceipt, FileFailure>>(&output.stderr).unwrap(),
        Err(FileFailure::WrongIdentity)
    );
    assert_eq!(
        fs::read_dir(home.path().join("project")).unwrap().count(),
        1
    );
}
