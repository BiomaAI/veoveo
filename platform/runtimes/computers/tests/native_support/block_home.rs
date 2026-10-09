//! Isolated physical storage evidence. This is not the production allocator.
use std::{fs, path::PathBuf, process::Command};
use uuid::Uuid;
use veoveo_computers_runtime::{PersistentHome, RetainedVolumeAdmission};

pub struct BlockHome {
    dir: PathBuf,
    socket: PathBuf,
    image: String,
    pub volume: String,
    device: Option<String>,
    backup_created: bool,
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
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Mount {
    source: Option<String>,
    target: String,
    volume_options: Option<VolumeOptions>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct VolumeOptions {
    subpath: Option<String>,
}
fn admit_retained_mount(body: &str, volume: &str) -> Result<(), String> {
    let mounts: Vec<Mount> = serde_json::from_str(body)
        .map_err(|error| format!("decode inspected Docker mounts: {error}"))?;
    let home = mounts
        .iter()
        .find(|mount| mount.source.as_deref() == Some(volume))
        .ok_or("registered retained-home mount absent")?;
    if home.target != "/sandbox/persistent" {
        return Err("retained-home mount target differs".into());
    }
    let options = home
        .volume_options
        .as_ref()
        .ok_or("retained-home VolumeOptions absent")?;
    if options.subpath.as_deref() != Some("home") {
        return Err("retained-home mount requires Subpath home".into());
    }
    Ok(())
}
// Inspect only mounts, and omit arbitrary driver configuration from failure output.
fn mount_excerpt(body: &str) -> String {
    let Ok(serde_json::Value::Array(mounts)) = serde_json::from_str(body) else {
        return format!("unavailable: invalid mount JSON ({} bytes)", body.len());
    };
    let diagnostic: Vec<_> = mounts
        .iter()
        .map(|mount| {
            serde_json::json!({
                "Source": mount.get("Source"), "Target": mount.get("Target"),
                "VolumeOptions": {
                    "Subpath": mount.pointer("/VolumeOptions/Subpath")
                }
            })
        })
        .collect();
    let mut excerpt = serde_json::to_string(&diagnostic).expect("serialize mount diagnostic");
    if excerpt.len() > 2048 {
        let mut end = 2048;
        while !excerpt.is_char_boundary(end) {
            end -= 1;
        }
        excerpt.truncate(end);
        excerpt.push_str(" [truncated]");
    }
    excerpt
}
impl BlockHome {
    pub fn create(dir: PathBuf, image: String, computer: Uuid, socket: PathBuf) -> Self {
        assert!(
            socket.is_absolute()
                && socket.exists()
                && socket != std::path::Path::new("/var/run/docker.sock"),
            "isolated retained-home daemon required"
        );
        let mut home = Self {
            dir,
            socket,
            image,
            volume: PersistentHome::volume_name(computer).unwrap(),
            device: None,
            backup_created: false,
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
    fn docker(&self) -> Command {
        let mut command = Command::new("docker");
        command.args(["--host", &format!("unix://{}", self.socket.display())]);
        command
    }
    fn helper(&self, args: &[&str]) -> Command {
        let mut command = self.docker();
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
        let mut command = self.docker();
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
        let mut command = self.docker();
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
            .arg(format!("device={device}"));
        for (key, value) in RetainedVolumeAdmission::DEFAULT.labels() {
            command.arg("--label").arg(format!("{key}={value}"));
        }
        command.arg(&self.volume);
        assert_eq!(checked(command), self.volume);
        let mut inspect = self.docker();
        inspect.args([
            "volume",
            "inspect",
            "--format",
            "{{json .Labels}}",
            &self.volume,
        ]);
        let labels: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&checked(inspect)).expect("native volume approval labels");
        assert!(
            RetainedVolumeAdmission::DEFAULT.matches(&labels),
            "retained volume approval or workspace differs"
        );
    }
    /// Native deletion of every fixture-owned container fences its physical writer.
    /// Production replacement must obtain equivalent evidence through its service.
    pub fn remove_consumers(&self) {
        let mut list = self.docker();
        list.args(["ps", "--all", "--quiet", "--filter"])
            .arg(format!("volume={}", self.volume));
        for id in checked(list).split_whitespace() {
            assert!((12..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_hexdigit()));
            let mut remove = self.docker();
            remove.args(["rm", "--force", id]);
            checked(remove);
        }
    }
    pub fn assert_registered_retained_mount(&self) {
        let mut list = self.docker();
        list.args(["ps", "--all", "--quiet", "--filter"])
            .arg(format!("volume={}", self.volume));
        let consumers = checked(list);
        let ids = consumers.split_whitespace().collect::<Vec<_>>();
        assert_eq!(ids.len(), 1, "expected one registered provider container");
        let mut inspect = self.docker();
        inspect.args(["inspect", "--format", "{{json .HostConfig.Mounts}}", ids[0]]);
        let body = checked(inspect);
        admit_retained_mount(&body, &self.volume)
            .unwrap_or_else(|error| panic!("{error}; inspected mounts: {}", mount_excerpt(&body)));
    }
    fn detach(&mut self) {
        self.remove_consumers();
        let mut remove = self.docker();
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
        self.backup_created = true;
        fs::File::open(&backup).unwrap().sync_all().unwrap();
        fs::remove_file(&source).unwrap();
        assert_eq!(fs::copy(&backup, &source).unwrap(), 536870912);
        fs::File::open(&source).unwrap().sync_all().unwrap();
        self.attach();
    }
    pub fn finish(mut self) {
        self.detach();
        fs::remove_file(self.dir.join("home.ext4")).unwrap();
        if self.backup_created {
            fs::remove_file(self.dir.join("backup.ext4")).unwrap();
        }
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

#[cfg(test)]
mod mount_tests {
    use super::*;
    use serde_json::{Value, json};

    fn mounts() -> Value {
        json!([
            {"Source":"retained-home", "Target":"/sandbox/persistent",
             "VolumeOptions":{"Subpath":"home"}},
            {"Source":"owned-channel", "Target":"/.openshell/channel",
             "VolumeOptions":{"NoCopy":true}},
            {"Type":"tmpfs", "Target":"/tmp"}
        ])
    }
    #[test]
    fn channel_without_subpath_preserves_retained_home_admission() {
        admit_retained_mount(&mounts().to_string(), "retained-home").unwrap();
    }
    #[test]
    fn retained_home_requires_present_exact_subpath() {
        for subpath in [None, Some(Value::Null), Some(json!("other"))] {
            let mut body = mounts();
            body[0]["VolumeOptions"]
                .as_object_mut()
                .unwrap()
                .remove("Subpath");
            if let Some(value) = subpath {
                body[0]["VolumeOptions"]["Subpath"] = value;
            }
            assert!(
                admit_retained_mount(&body.to_string(), "retained-home")
                    .unwrap_err()
                    .contains("Subpath home")
            );
        }
        assert!(admit_retained_mount(&mounts().to_string(), "absent-home").is_err());
    }
    #[test]
    fn retained_home_target_and_volume_options_remain_mandatory() {
        for (field, value) in [("Target", json!("/wrong")), ("VolumeOptions", Value::Null)] {
            let mut body = mounts();
            body[0][field] = value;
            assert!(admit_retained_mount(&body.to_string(), "retained-home").is_err());
        }
    }
    #[test]
    fn decode_failure_retains_bounded_mount_fields_without_driver_secrets() {
        let mut body = mounts();
        body[0]["VolumeOptions"]["Subpath"] = json!(42);
        body[0]["VolumeOptions"]["DriverConfig"] =
            json!({"Options":{"password":"private-driver-secret"}});
        let raw = body.to_string();
        assert!(
            admit_retained_mount(&raw, "retained-home")
                .unwrap_err()
                .contains("decode inspected Docker mounts")
        );
        let excerpt = mount_excerpt(&raw);
        assert!(excerpt.contains("retained-home") && excerpt.contains("42"));
        assert!(!excerpt.contains("private-driver-secret"));
        body[1]["Target"] = json!("é".repeat(4096));
        let excerpt = mount_excerpt(&body.to_string());
        assert!(excerpt.len() <= 2048 + " [truncated]".len());
        assert!(excerpt.ends_with(" [truncated]"));
    }
}
