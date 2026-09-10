//! Isolated physical storage evidence. This is not the production allocator.
use std::{fs, path::PathBuf, process::Command};
use uuid::Uuid;
use veoveo_computers_runtime::PersistentHome;

pub struct BlockHome {
    dir: PathBuf,
    image: String,
    pub volume: String,
    device: Option<String>,
}
fn checked(mut command: Command) -> String {
    let output = command.output().expect("start fixture command");
    assert!(
        output.status.success(),
        "fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
impl BlockHome {
    pub fn create(dir: PathBuf, image: String, computer: Uuid) -> Self {
        let mut home = Self {
            dir,
            image,
            volume: PersistentHome::volume_name(computer).unwrap(),
            device: None,
        };
        fs::File::create_new(home.dir.join("home.ext4")).unwrap();
        checked(home.helper(&[
            "/usr/bin/fallocate",
            "--length",
            "536870912",
            "/fixture/home.ext4",
        ]));
        checked(home.helper(&[
            "/usr/sbin/mkfs.ext4",
            "-F",
            "-q",
            "-m",
            "0",
            "/fixture/home.ext4",
        ]));
        home.attach();
        let mut seed = home.container();
        seed.args(["/bin/sh", "-c", "mkdir /home-volume/home && chown 10001:10001 /home-volume/home && chmod 700 /home-volume/home"]);
        checked(seed);
        home
    }
    fn helper(&self, args: &[&str]) -> Command {
        let mut command = Command::new("docker");
        command
            .args([
                "run",
                "--rm",
                "--network",
                "none",
                "--privileged",
                "--mount",
                "type=bind,source=/dev,target=/dev",
                "--mount",
            ])
            .arg(format!(
                "type=bind,source={},target=/fixture",
                self.dir.display()
            ))
            .arg(&self.image)
            .args(args);
        command
    }
    fn container(&self) -> Command {
        let mut command = Command::new("docker");
        command
            .args(["run", "--rm", "--network", "none", "--mount"])
            .arg(format!(
                "type=volume,source={},target=/home-volume",
                self.volume
            ))
            .arg(&self.image);
        command
    }
    fn attach(&mut self) {
        let device = checked(self.helper(&[
            "/usr/sbin/losetup",
            "--find",
            "--show",
            "--nooverlap",
            "/fixture/home.ext4",
        ]));
        assert!(
            device
                .strip_prefix("/dev/loop")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        );
        self.device = Some(device.clone());
        let mut command = Command::new("docker");
        command
            .args([
                "volume",
                "create",
                "--driver",
                "local",
                "--opt",
                "type=ext4",
                "--opt",
                "o=rw,nodev,nosuid",
                "--opt",
            ])
            .arg(format!("device={device}"))
            .arg(&self.volume);
        assert_eq!(checked(command), self.volume);
    }
    /// Native deletion of every fixture-owned container fences its physical writer.
    /// Production replacement must obtain equivalent evidence through its service.
    pub fn remove_consumers(&self) {
        let mut list = Command::new("docker");
        list.args(["ps", "--all", "--quiet", "--filter"])
            .arg(format!("volume={}", self.volume));
        for id in checked(list).split_whitespace() {
            assert!((12..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_hexdigit()));
            let mut remove = Command::new("docker");
            remove.args(["rm", "--force", id]);
            checked(remove);
        }
    }
    fn detach(&mut self) {
        self.remove_consumers();
        let mut remove = Command::new("docker");
        remove.args(["volume", "rm", &self.volume]);
        checked(remove);
        let device = self.device.take().unwrap();
        checked(self.helper(&["/usr/sbin/losetup", "--detach", &device]));
        // A busy loop is only marked autoclear. Retain the source on that outcome.
        let remaining = checked(self.helper(&[
            "/usr/sbin/losetup",
            "--list",
            "--noheadings",
            "--output",
            "NAME",
            "--associated",
            "/fixture/home.ext4",
        ]));
        assert!(remaining.is_empty(), "physical writer remains attached");
    }
    pub fn backup_restore(&mut self) {
        self.detach();
        let source = self.dir.join("home.ext4");
        let backup = self.dir.join("backup.ext4");
        assert_eq!(fs::copy(&source, &backup).unwrap(), 536870912);
        fs::File::open(&backup).unwrap().sync_all().unwrap();
        fs::remove_file(&source).unwrap();
        assert_eq!(fs::copy(&backup, &source).unwrap(), 536870912);
        fs::File::open(&source).unwrap().sync_all().unwrap();
        self.attach();
    }
    pub fn finish(mut self) {
        self.detach();
        fs::remove_file(self.dir.join("home.ext4")).unwrap();
        fs::remove_file(self.dir.join("backup.ext4")).unwrap();
    }
}
impl Drop for BlockHome {
    fn drop(&mut self) {
        if self.device.is_some() {
            // Best effort on assertion failure. Preserve backing files on cleanup
            // failure; deleting a possibly mounted home is never acceptable.
            let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.detach()));
            if cleanup.is_ok() {
                let _ = fs::remove_file(self.dir.join("home.ext4"));
                let _ = fs::remove_file(self.dir.join("backup.ext4"));
            }
        }
    }
}
