use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
fn run(program: &str, args: &[&str], path: &Path) -> Output {
    Command::new("timeout")
        .args(["10", program])
        .args(args)
        .arg(path)
        .output()
        .unwrap()
}
pub fn cleanup() {
    let root = Path::new("/fixture/data/state/retained/homes");
    if root.exists() {
        for entry in fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            assert_eq!(
                uuid::Uuid::parse_str(name).unwrap().simple().to_string(),
                name
            );
            let mount = path.join("mount");
            let mounted = run("findmnt", &["--mountpoint"], &mount);
            if mounted.status.success() {
                assert!(run("umount", &[], &mount).status.success());
            } else {
                assert_eq!(mounted.status.code(), Some(1));
            }
            let backing = path.join("home.ext4");
            let devices = run(
                "losetup",
                &["--list", "--noheadings", "--output", "NAME", "--associated"],
                &backing,
            );
            assert!(devices.status.success());
            for device in String::from_utf8(devices.stdout).unwrap().lines() {
                assert!(
                    device
                        .strip_prefix("/dev/loop")
                        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                );
                assert!(
                    run("losetup", &["--detach"], Path::new(device))
                        .status
                        .success()
                );
            }
            let remaining = run(
                "losetup",
                &["--list", "--noheadings", "--output", "NAME", "--associated"],
                &backing,
            );
            assert!(
                remaining.status.success() && remaining.stdout.is_empty(),
                "owned loop remains attached"
            );
        }
    }
    for name in ["data", "trust", "config", "provider", "storage"] {
        let path = Path::new("/fixture").join(name);
        if path.exists() {
            fs::remove_dir_all(path).unwrap();
        }
    }
}
