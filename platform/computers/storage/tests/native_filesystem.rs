//! The production filesystem backend in disposable privileged containers. No
//! installation home, provider process or Docker daemon is modified.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};
use uuid::Uuid;
use veoveo_computer_storage::{
    AllocationState, Filesystem, HomeIdentity, HostIdentity, Journal, StorageError,
};

const CAPACITY: u64 = 512 * 1024 * 1024;
const TEST: &str = "native_quota_restart_and_filesystem_identity";
const FIXTURE_DOCKER_ENDPOINT: &str = "unix:///var/run/docker.sock";

fn identity() -> HomeIdentity {
    let id = std::env::var("VEOVEO_STORAGE_NATIVE_COMPUTER")
        .unwrap()
        .parse()
        .unwrap();
    HomeIdentity {
        provider_id: Uuid::from_u128(100),
        computer_id: id,
        instance_id: id,
        template_fingerprint: "a".repeat(64),
    }
}
fn root() -> PathBuf {
    PathBuf::from(std::env::var_os("VEOVEO_STORAGE_NATIVE_ROOT").unwrap())
}
fn journal() -> Journal {
    Journal::open(
        root(),
        HostIdentity {
            provider_id: Uuid::from_u128(100),
            engine_id: Uuid::from_u128(200),
            namespace: "filesystem-fixture".into(),
        },
    )
    .unwrap()
}
fn checked(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}
fn detach(backing: &Path) {
    if !backing.exists() {
        return;
    }
    let devices = checked(
        Command::new("losetup")
            .args(["--list", "--noheadings", "--output", "NAME", "--associated"])
            .arg(backing),
    );
    for device in devices.lines() {
        assert!(device.strip_prefix("/dev/loop").is_some_and(
            |number| !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        ));
        checked(Command::new("losetup").args(["--detach", device]));
    }
    assert!(
        checked(
            Command::new("losetup")
                .args(["--list", "--noheadings", "--output", "NAME", "--associated"])
                .arg(backing)
        )
        .is_empty()
    );
}

async fn child(phase: &str) {
    let identity = identity();
    if phase == "cleanup" {
        let backing = root()
            .join("homes")
            .join(identity.computer_id.simple().to_string())
            .join("home.ext4");
        detach(&backing);
        if root().exists() {
            fs::remove_dir_all(root()).unwrap();
        }
        return;
    }
    if phase == "prepare" {
        let mut constrained = Filesystem::new(journal(), u64::MAX).unwrap();
        assert!(matches!(
            constrained.prepare(identity.clone(), CAPACITY).await,
            Err(StorageError::CapacityExceeded)
        ));
        assert!(
            constrained
                .journal()
                .load(identity.computer_id)
                .unwrap()
                .is_none()
        );
    }
    let mut filesystem = Filesystem::new(journal(), CAPACITY).unwrap();
    if phase == "prepare" {
        let mount = filesystem
            .prepare(identity.clone(), CAPACITY)
            .await
            .unwrap();
        let home = mount.join("home");
        assert_eq!(fs::metadata(&home).unwrap().mode() & 0o777, 0o700);
        fs::write(home.join("retained"), b"same home after helper restart").unwrap();
        let mut filled = 0u64;
        {
            let mut file = fs::File::create(home.join("fill")).unwrap();
            let chunk = vec![0x55; 1024 * 1024];
            loop {
                match file.write(&chunk) {
                    Ok(0) => panic!("unexpected zero-byte write"),
                    Ok(bytes) => {
                        filled += bytes as u64;
                        assert!(filled < CAPACITY);
                    }
                    Err(error) => {
                        assert_eq!(error.raw_os_error(), Some(nix::libc::ENOSPC));
                        break;
                    }
                }
            }
        }
        assert!(filled > 400 * 1024 * 1024);
        fs::remove_file(home.join("fill")).unwrap();
        assert_eq!(
            filesystem
                .prepare(identity.clone(), CAPACITY)
                .await
                .unwrap(),
            mount
        );
        assert_eq!(
            fs::read(home.join("retained")).unwrap(),
            b"same home after helper restart"
        );
        assert!(matches!(
            filesystem
                .journal()
                .load(identity.computer_id)
                .unwrap()
                .unwrap()
                .state(),
            AllocationState::Ready { .. }
        ));
        fs::File::open(&home).unwrap().sync_all().unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o755)).unwrap();
        nix::unistd::syncfs(fs::File::open(&mount).unwrap()).unwrap();
        // Namespace exit removes this mount. The exact backing file and loop
        // mapping survive for a different helper container to restore.
    } else if phase == "restore" {
        let mount = filesystem.restore(&identity).await.unwrap();
        assert_eq!(
            fs::metadata(mount.join("home")).unwrap().mode() & 0o777,
            0o755
        );
        assert_eq!(
            fs::read(mount.join("home/retained")).unwrap(),
            b"same home after helper restart"
        );
        let mut foreign = identity.clone();
        foreign.instance_id = Uuid::now_v7();
        assert!(matches!(
            filesystem.restore(&foreign).await,
            Err(StorageError::IdentityMismatch)
        ));
        nix::mount::umount(&mount).unwrap();
        let backing = filesystem
            .journal()
            .directory(identity.computer_id)
            .unwrap()
            .join("home.ext4");
        detach(&backing);
        checked(
            Command::new("tune2fs")
                .args(["-U", &Uuid::now_v7().to_string()])
                .arg(&backing),
        );
        assert!(matches!(
            filesystem.restore(&identity).await,
            Err(StorageError::RecoveryRequired)
        ));
        // The failed restore leaves the original admitted identity intact.
        assert_eq!(
            filesystem
                .journal()
                .load(identity.computer_id)
                .unwrap()
                .unwrap()
                .identity(),
            &identity
        );
    } else {
        panic!("unknown native fixture phase");
    }
}

struct Fixture {
    directory: PathBuf,
    computer: Uuid,
    name: String,
    image: String,
    executable: PathBuf,
    finished: bool,
}
impl Fixture {
    fn new() -> Self {
        let computer = Uuid::now_v7();
        let directory = PathBuf::from(
            std::env::var_os("VEOVEO_COMPUTERS_NATIVE_OUTPUT").expect("owned fixture output root"),
        )
        .join(format!("storage-filesystem-{}", computer.simple()));
        fs::create_dir(&directory).unwrap();
        let image = std::env::var("VEOVEO_COMPUTERS_NATIVE_IMAGE")
            .expect("exact locally available Computer image");
        assert!(image.contains("@sha256:"));
        Self {
            directory,
            computer,
            name: format!("veoveo-storage-native-{}", computer.simple()),
            image,
            executable: std::env::current_exe().unwrap(),
            finished: false,
        }
    }
    fn command(&self, phase: &str) -> Command {
        let mut command = Command::new("docker");
        command.args(["--host", FIXTURE_DOCKER_ENDPOINT]);
        command
            .args([
                "run",
                "--rm",
                "--pull",
                "never",
                "--name",
                &self.name,
                "--privileged",
                "--user",
                "0:0",
                "--network",
                "none",
                "--memory",
                "1g",
                "--cpus",
                "2",
                "--pids-limit",
                "128",
                "--mount",
                "type=bind,source=/dev,target=/dev",
                "--mount",
            ])
            .arg(format!(
                "type=bind,source={},target={}",
                self.directory.display(),
                self.directory.display()
            ))
            .arg("--mount")
            .arg(format!(
                "type=bind,source={},target=/storage-test,readonly",
                self.executable.display()
            ))
            .arg("--env")
            .arg(format!("VEOVEO_STORAGE_NATIVE_COMPUTER={}", self.computer))
            .arg("--env")
            .arg(format!(
                "VEOVEO_STORAGE_NATIVE_ROOT={}",
                self.directory.join("retained").display()
            ))
            .arg("--env")
            .arg(format!("VEOVEO_STORAGE_NATIVE_PHASE={phase}"))
            .args([
                "--entrypoint",
                "/storage-test",
                &self.image,
                "--exact",
                TEST,
                "--ignored",
                "--nocapture",
            ]);
        command
    }
    async fn run(&self, phase: &str) {
        let mut command = tokio::process::Command::from(self.command(phase));
        command.kill_on_drop(true);
        let result: Output = tokio::time::timeout(Duration::from_secs(45), command.output())
            .await
            .expect("native filesystem fixture deadline")
            .unwrap();
        fs::write(
            self.directory.join(format!("{phase}.stdout")),
            &result.stdout,
        )
        .unwrap();
        fs::write(
            self.directory.join(format!("{phase}.stderr")),
            &result.stderr,
        )
        .unwrap();
        assert!(
            result.status.success(),
            "{phase}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    async fn finish(mut self) {
        self.run("cleanup").await;
        assert!(!self.directory.join("retained").exists());
        self.finished = true;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let _ = Command::new("timeout")
            .args([
                "10",
                "docker",
                "--host",
                FIXTURE_DOCKER_ENDPOINT,
                "rm",
                "--force",
                &self.name,
            ])
            .output();
        let remaining = Command::new("timeout")
            .args([
                "5",
                "docker",
                "--host",
                FIXTURE_DOCKER_ENDPOINT,
                "ps",
                "--all",
                "--quiet",
                "--filter",
                &format!("name=^/{}$", self.name),
            ])
            .output();
        if remaining.is_ok_and(|output| output.status.success() && output.stdout.is_empty()) {
            let command = self.command("cleanup");
            let _ = Command::new("timeout")
                .arg("30")
                .arg(command.get_program())
                .args(command.get_args())
                .output();
        }
    }
}

#[tokio::test]
#[ignore = "requires the pinned Computer image and Docker; owns isolated privileged 512 MiB ext4 fixtures and loop mappings"]
async fn native_quota_restart_and_filesystem_identity() {
    if let Ok(phase) = std::env::var("VEOVEO_STORAGE_NATIVE_PHASE") {
        child(&phase).await;
        return;
    }
    let fixture = Fixture::new();
    println!(
        "Storage filesystem diagnostics: {}",
        fixture.directory.display()
    );
    fixture.run("prepare").await;
    fixture.run("restore").await;
    fixture.finish().await;
}
