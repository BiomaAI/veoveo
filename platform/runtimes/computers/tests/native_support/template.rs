//! Canonical retained native fixture template. No network access is granted.
use veoveo_computers_runtime::{protocol::sandbox::v1 as policy, *};

pub fn retained_template(image: String) -> DevelopmentTemplate {
    DevelopmentTemplate::new(
        image,
        2,
        2048,
        policy::SandboxPolicy {
            version: 1,
            filesystem: Some(policy::FilesystemPolicy {
                include_workdir: false,
                read_only: [
                    "/usr",
                    "/lib",
                    "/lib64",
                    "/bin",
                    "/etc/passwd",
                    "/etc/group",
                    "/etc/profile",
                    "/etc/profile.d",
                    "/etc/bash.bashrc",
                    "/etc/nsswitch.conf",
                    "/etc/resolv.conf",
                    "/etc/ssl/certs",
                    "/proc",
                    "/sandbox",
                ]
                .map(str::to_owned)
                .into(),
                read_write: [PERSISTENT_HOME, "/tmp", "/dev/null", "/dev/tty", "/dev/pts"]
                    .map(str::to_owned)
                    .into(),
            }),
            landlock: Some(policy::LandlockPolicy {
                compatibility: "hard_requirement".into(),
            }),
            process: Some(policy::ProcessPolicy {
                run_as_user: "10001".into(),
                run_as_group: "10001".into(),
            }),
            ..Default::default()
        },
        PERSISTENT_COMMAND.map(str::to_owned).into(),
        Some(PersistentHome::new(512, 32).unwrap()),
    )
    .unwrap()
}
