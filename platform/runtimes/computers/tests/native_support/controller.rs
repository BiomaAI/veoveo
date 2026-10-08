//! The native fixture owns the gateway process group; its Docker driver is in-process.
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Launch {
    gateway: PathBuf,
    dir: PathBuf,
    path: std::ffi::OsString,
    inputs: Vec<(PathBuf, [u8; 32])>,
}
impl Launch {
    pub(super) fn new(gateway: PathBuf, driver: PathBuf, dir: PathBuf) -> Self {
        let path = std::env::join_paths(
            std::iter::once(driver.parent().unwrap().to_path_buf()).chain(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            )),
        )
        .unwrap();
        let inputs = [
            "gateway.toml",
            "ca.pem",
            "client.pem",
            "client-key.pem",
            "server.pem",
            "server-key.pem",
            "guest.pem",
            "guest-key.pem",
            "jwt-key.pem",
            "jwt-public.pem",
            "jwt-kid",
        ]
        .into_iter()
        .map(|name| {
            let path = dir.join(name);
            let digest = Sha256::digest(fs::read(&path).expect("owned launch input")).into();
            (path, digest)
        })
        .chain([gateway.clone(), driver.clone()].into_iter().map(|path| {
            let digest =
                Sha256::digest(fs::read(&path).expect("qualified controller binary")).into();
            (path, digest)
        }))
        .collect();
        Self {
            gateway,
            dir,
            path,
            inputs,
        }
    }
    pub(super) fn spawn(&self) -> io::Result<Child> {
        for (path, digest) in &self.inputs {
            if <[u8; 32]>::from(Sha256::digest(fs::read(path)?)) != *digest {
                return Err(io::Error::other(
                    "owned controller launch input changed; refusing restart",
                ));
            }
        }
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("gateway.log"))?;
        let mut command = Command::new(&self.gateway);
        command
            .env_clear()
            .env("PATH", &self.path)
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .arg("--config")
            .arg(self.dir.join("gateway.toml"))
            .arg("--db-url")
            .arg(format!(
                "sqlite://{}?mode=rwc",
                self.dir.join("gateway.sqlite").display()
            ))
            .arg("--tls-cert")
            .arg(self.dir.join("server.pem"))
            .arg("--tls-key")
            .arg(self.dir.join("server-key.pem"))
            .arg("--tls-client-ca")
            .arg(self.dir.join("ca.pem"))
            .args([
                "--enable-mtls-auth",
                "true",
                "--enable-loopback-service-http",
                "false",
            ])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .process_group(0);
        command.spawn()
    }
    pub(super) fn admit_controller(&self, child: &mut Child) -> io::Result<()> {
        if child.try_wait()?.is_some() || process_group(child.id())? != Some(child.id()) {
            return Err(io::Error::other(
                "owned controller leader/group no longer live",
            ));
        }
        if fs::read_link(format!("/proc/{}/exe", child.id()))? != fs::canonicalize(&self.gateway)? {
            return Err(io::Error::other(
                "owned controller executable differs from qualified gateway",
            ));
        }
        // provider_config selects the qualified gateway's DockerFactory in-process
        // profile. Its complete config and binaries were captured before initial spawn.
        for (path, digest) in &self.inputs {
            if <[u8; 32]>::from(Sha256::digest(fs::read(path)?)) != *digest {
                return Err(io::Error::other("owned controller admission input changed"));
            }
        }
        Ok(())
    }
}

fn bounded(program: &str, args: &[&str]) -> io::Result<String> {
    let output = Command::new("timeout")
        .args(["--signal=KILL", "3s", program])
        .args(args)
        .output()?;
    if (!output.status.success()
        && !(program == "ps" && output.status.code() == Some(1) && output.stdout.is_empty()))
        || output.stdout.len() > 4096
    {
        return Err(io::Error::other("bounded owned process command failed"));
    }
    String::from_utf8(output.stdout).map_err(|_| io::Error::other("invalid process command output"))
}
fn process_group(pid: u32) -> io::Result<Option<u32>> {
    let output = bounded(
        "ps",
        &["--no-headers", "-o", "pgid=", "--pid", &pid.to_string()],
    )?;
    if output.trim().is_empty() {
        return Ok(None);
    }
    output
        .trim()
        .parse()
        .map(Some)
        .map_err(|_| io::Error::other("invalid owned process group"))
}

pub(super) fn stop(child: &mut Child) -> io::Result<ExitStatus> {
    if let Some(exit) = child.try_wait()? {
        return Ok(exit);
    }
    let pid = child.id();
    if process_group(pid)? != Some(pid) {
        return Err(io::Error::other(
            "controller is not its admitted group leader; refusing signal",
        ));
    }
    let descendants = bounded(
        "ps",
        &[
            "--no-headers",
            "-o",
            "pid=,pgid=",
            "--ppid",
            &pid.to_string(),
        ],
    )?;
    let members: Vec<_> = descendants
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 2 {
                return Err(io::Error::other("invalid owned child observation"));
            }
            let child_pid = fields[0]
                .parse::<u32>()
                .map_err(|_| io::Error::other("invalid child PID"))?;
            let group = fields[1]
                .parse::<u32>()
                .map_err(|_| io::Error::other("invalid child group"))?;
            if group != pid {
                return Err(io::Error::other(
                    "owned child escaped controller group; refusing signal",
                ));
            }
            use std::os::unix::fs::MetadataExt;
            let metadata = fs::metadata(format!("/proc/{child_pid}"))?;
            Ok((child_pid, metadata.ino()))
        })
        .collect::<io::Result<_>>()?;
    bounded("kill", &["--signal", "TERM", "--", &format!("-{pid}")])?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(exit) = child.try_wait()? {
            // A subprocess can outlive its leader's graceful exit. Its original group
            // remains ours; kill only that group and never an escaped group.
            use std::os::unix::fs::MetadataExt;
            // An observed original live member keeps this group ID from reuse.
            // Once the group is empty, never signal its old numeric ID again.
            let original_live = members.iter().any(|(member, inode)| {
                fs::metadata(format!("/proc/{member}")).is_ok_and(|m| m.ino() == *inode)
                    && Path::new(&format!("/proc/{member}/exe")).exists()
                    && process_group(*member).ok().flatten() == Some(pid)
            });
            if original_live {
                bounded("kill", &["--signal", "KILL", "--", &format!("-{pid}")])?;
            }
            return Ok(exit);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    bounded("kill", &["--signal", "KILL", "--", &format!("-{pid}")])?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(exit) = child.try_wait()? {
            return Ok(exit);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::other(
        "owned controller group did not exit within TERM/KILL/reap budget",
    ))
}

/// Reap only the exact child handle if group admission or observation failed.
/// Escaped processes are never signalled through a guessed PID or process group.
pub(super) fn stop_child_only(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    child.kill()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(io::Error::other(
        "owned controller child did not reap within three seconds",
    ))
}

fn assert_process_exited(pid: u32) {
    // Zombies have exited and hold no sockets or executable; their parent is the
    // container/host reaper. Never signal a reused or escaped PID by itself.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !Path::new(&format!("/proc/{pid}/exe")).exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("old owned process is still executable after the exit budget");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn controller_group_refuses_foreign_group_before_signalling() {
        let mut child = OwnedChild(Command::new("sleep").arg("30").spawn().unwrap());
        assert_ne!(process_group(child.0.id()).unwrap(), Some(child.0.id()));
        assert!(stop(&mut child.0).is_err());
        assert!(
            child.0.try_wait().unwrap().is_none(),
            "refusal must leave foreign group untouched"
        );
    }
    #[test]
    fn controller_group_admits_exact_gateway_and_reaps_owned_group() {
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & printf '%s\\n' $!; wait"])
            .process_group(0)
            .stdout(Stdio::piped());
        let mut child = OwnedChild(command.spawn().unwrap());
        let mut pid = String::new();
        std::io::BufReader::new(child.0.stdout.take().unwrap())
            .read_line(&mut pid)
            .unwrap();
        let descendant_pid: u32 = pid.trim().parse().unwrap();
        let mut launch = Launch {
            gateway: PathBuf::from("/usr/bin/sh"),
            dir: std::env::temp_dir(),
            path: std::env::var_os("PATH").unwrap(),
            inputs: Vec::new(),
        };
        launch.admit_controller(&mut child.0).unwrap();
        launch.gateway = PathBuf::from("/usr/bin/sleep");
        assert!(launch.admit_controller(&mut child.0).is_err());
        assert!(child.0.try_wait().unwrap().is_none());
        stop(&mut child.0).unwrap();
        assert_process_exited(descendant_pid);
        assert!(child.0.try_wait().unwrap().is_some());
    }
    #[test]
    fn controller_launch_refuses_changed_retained_inputs_before_spawn_and_redacts_bytes() {
        let dir =
            std::env::temp_dir().join(format!("veoveo-controller-input-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&dir).unwrap();
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _directory = Directory(dir.clone());
        for name in [
            "gateway.toml",
            "ca.pem",
            "client.pem",
            "client-key.pem",
            "server.pem",
            "server-key.pem",
            "guest.pem",
            "guest-key.pem",
            "jwt-key.pem",
            "jwt-public.pem",
            "jwt-kid",
        ] {
            fs::write(dir.join(name), "private-original").unwrap();
        }
        let launch = Launch::new(
            PathBuf::from("/usr/bin/sh"),
            PathBuf::from("/usr/bin/sleep"),
            dir.clone(),
        );
        fs::write(dir.join("jwt-key.pem"), "synthetic-private-key-change").unwrap();
        let error = launch.spawn().unwrap_err().to_string();
        assert!(!error.contains("synthetic-private-key-change"));
        assert!(!error.contains("private-original"));
        assert!(
            !dir.join("gateway.log").exists(),
            "changed input must refuse before launch effects"
        );
    }
}
