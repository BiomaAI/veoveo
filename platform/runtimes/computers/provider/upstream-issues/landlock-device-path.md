# Draft: Landlock device path admission in static musl workload runtime

Status: local draft; not submitted. Qualification against the published executable is pending.

Affected release: NVIDIA OpenShell v0.1.2, commit 6648bd0c290efbc41ba131ee9831ee45cd431f94.
Relevant owner: crates/openshell-sandbox/src/sandbox/linux/landlock.rs, try_open_path.

Landlock path rules should inspect a path descriptor without opening device data.
Qualification should trace the official static sandbox executable's open flags for
/dev/tty before controlling-terminal setup and compare /dev/null and /dev/urandom.
Confirm whether the published musl executable preserves O_PATH|O_CLOEXEC or attempts
a device data open that can return ENXIO. Record exact kernel/libc and artifact hash.
Existing patched-build observations do not establish this released-binary outcome.

An upstream fix, if reproduction establishes the defect, should open an O_PATH
reference through a maintained syscall abstraction and derive classification from
the same descriptor. Preserve policy denial, symlink handling and hard Landlock
requirements; do not suppress errors or remove device rules to make launch pass.
The provider release stays unchanged in Veoveo while this issue is evaluated.
