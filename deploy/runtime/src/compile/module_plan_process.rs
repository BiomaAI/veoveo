//! Owned offline composition execution; bounded pipes and process groups.

use anyhow::{Context, Result, ensure};
use nix::{
    fcntl::{FcntlArg, OFlag, fcntl},
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use std::{
    io::{ErrorKind, Read},
    os::{fd::AsFd, unix::process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_LIMIT: usize = 2 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(120);

pub(super) fn generate(image: &str, selection: &Path, composition: &str) -> Result<Vec<u8>> {
    bounded(Command::new("docker").args(["pull", image]), DEADLINE)?;
    let directory = tempfile::Builder::new()
        .prefix("veoveo-plan-process-")
        .tempdir()?;
    let name = directory
        .path()
        .file_name()
        .context("owned process directory has no name")?
        .to_str()
        .context("owned process name must be UTF-8")?;
    generate_named(Path::new("docker"), image, selection, composition, name)
}

fn generate_named(
    program: &Path,
    image: &str,
    selection: &Path,
    composition: &str,
    name: &str,
) -> Result<Vec<u8>> {
    ensure!(
        !selection.to_string_lossy().contains(','),
        "module selection path cannot contain Docker mount separator"
    );
    let mount = format!(
        "type=bind,source={},target=/module-selection.json,readonly",
        selection
            .to_str()
            .context("module selection path must be UTF-8")?
    );
    let created = bounded(
        Command::new(program).args([
            "create",
            "--name",
            name,
            "--network=none",
            "--read-only",
            "--cap-drop=ALL",
            "--security-opt=no-new-privileges",
            "--memory=512m",
            "--pids-limit=64",
            "--cpus=1",
            "--mount",
            &mount,
            image,
            "module-plan",
            "--modules",
            "/module-selection.json",
            "--composition",
            composition,
        ]),
        DEADLINE,
    )
    .context("composition container creation failed; no unproved container will be removed")?;
    let id = String::from_utf8(created)
        .context("Docker container ID must be UTF-8")?
        .trim()
        .to_owned();
    ensure!(
        id.len() == 64
            && id
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "Docker create did not prove an owned container ID; cleanup requires diagnosis"
    );
    let mut owned = ContainerCleanup {
        program: program.to_owned(),
        id,
        removed: false,
    };
    let output = bounded(
        Command::new(program).args(["start", "--attach", &owned.id]),
        DEADLINE,
    );
    owned.remove()?;
    output
}

struct ContainerCleanup {
    program: std::path::PathBuf,
    id: String,
    removed: bool,
}
impl ContainerCleanup {
    fn remove(&mut self) -> Result<()> {
        bounded(
            Command::new(&self.program).args(["rm", "--force", &self.id]),
            Duration::from_secs(10),
        )
        .context("owned composition container cleanup failed")?;
        self.removed = true;
        Ok(())
    }
}
impl Drop for ContainerCleanup {
    fn drop(&mut self) {
        if !self.removed {
            let _ = self.remove();
        }
    }
}

fn nonblocking(input: &impl AsFd) -> Result<()> {
    let flags = OFlag::from_bits_truncate(fcntl(input, FcntlArg::F_GETFL)?);
    fcntl(input, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))?;
    Ok(())
}

fn drain(input: &mut impl Read, output: &mut Vec<u8>) -> Result<bool> {
    let mut buffer = [0; 8192];
    loop {
        match input.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                ensure!(
                    output.len() + count <= OUTPUT_LIMIT,
                    "composition process output exceeds 2 MiB"
                );
                output.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(false),
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

struct ProcessCleanup {
    child: Child,
    group: Pid,
    finished: bool,
}
impl ProcessCleanup {
    fn terminate(&mut self) {
        let _ = killpg(self.group, Signal::SIGKILL);
        let _ = self.child.kill();
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(2) {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}
impl Drop for ProcessCleanup {
    fn drop(&mut self) {
        if !self.finished {
            self.terminate();
        }
    }
}

fn bounded(command: &mut Command, deadline: Duration) -> Result<Vec<u8>> {
    let child = command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting composition process")?;
    let group = Pid::from_raw(
        i32::try_from(child.id()).context("composition process ID exceeds supported range")?,
    );
    let mut owned = ProcessCleanup {
        child,
        group,
        finished: false,
    };
    let mut stdout = owned
        .child
        .stdout
        .take()
        .context("composition stdout unavailable")?;
    let mut stderr = owned
        .child
        .stderr
        .take()
        .context("composition stderr unavailable")?;
    nonblocking(&stdout)?;
    nonblocking(&stderr)?;
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let started = Instant::now();
    let mut status = None;
    loop {
        let output_done = drain(&mut stdout, &mut output)?;
        let diagnostic_done = drain(&mut stderr, &mut diagnostics)?;
        if status.is_none() {
            status = owned
                .child
                .try_wait()
                .context("observing composition process")?;
        }
        if let Some(status) = status.as_ref().filter(|_| output_done && diagnostic_done) {
            owned.finished = true;
            ensure!(
                status.success(),
                "composition process failed with {status}: {}",
                String::from_utf8_lossy(&diagnostics)
            );
            return Ok(output);
        }
        ensure!(
            started.elapsed() < deadline,
            "composition process exceeded its deadline"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn oversized_output_is_rejected() {
        assert!(
            bounded(
                Command::new("head").args(["-c", "3000000", "/dev/zero"]),
                Duration::from_secs(2)
            )
            .unwrap_err()
            .to_string()
            .contains("2 MiB")
        );
    }

    #[test]
    fn timeout_and_descendant_held_pipes_are_bounded() {
        for command in ["sleep 10", "sleep 10 & exit 0"] {
            let started = Instant::now();
            let result = bounded(
                Command::new("sh").args(["-c", command]),
                Duration::from_millis(50),
            );
            assert!(result.unwrap_err().to_string().contains("deadline"));
            assert!(started.elapsed() < Duration::from_secs(2));
        }
    }

    #[test]
    fn failed_create_never_removes_a_colliding_name() {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("docker");
        let log = directory.path().join("calls");
        fs::write(
            &program,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{}'\nexit 1\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        let selection = directory.path().join("selection.json");
        fs::write(&selection, "{}").unwrap();
        assert!(
            generate_named(
                &program,
                "image@sha256:digest",
                &selection,
                "sha256:digest",
                "preexisting-name"
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(&log).unwrap().trim(), "create");
    }

    #[test]
    fn success_and_start_failure_remove_only_the_confirmed_container_id() {
        for start_status in [0, 1] {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("docker");
            let log = directory.path().join("calls");
            let id = "c".repeat(64);
            fs::write(&program, format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\ncase \"$1\" in\ncreate) printf '%s\\n' '{}';;\nstart) printf '{{}}\\n'; exit {};;\nrm) exit 0;;\n*) exit 1;;\nesac\n",
                log.display(), id, start_status
            )).unwrap();
            fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
            let selection = directory.path().join("selection.json");
            fs::write(&selection, "{}").unwrap();
            let result = generate_named(
                &program,
                "image@sha256:digest",
                &selection,
                "sha256:digest",
                "chosen-name",
            );
            assert_eq!(result.is_ok(), start_status == 0);
            let calls = fs::read_to_string(&log).unwrap();
            assert!(
                calls
                    .lines()
                    .any(|call| call == format!("start --attach {id}"))
            );
            assert!(calls.lines().any(|call| call == format!("rm --force {id}")));
            assert!(!calls.lines().any(|call| call == "rm --force chosen-name"));
        }
    }
}
