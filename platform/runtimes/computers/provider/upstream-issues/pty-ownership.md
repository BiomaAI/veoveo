# Draft: PTY ownership after numeric privilege drop

Status: local draft; not submitted. Qualification against the published executable is pending.

Affected release: NVIDIA OpenShell v0.1.2, commit 6648bd0c290efbc41ba131ee9831ee45cd431f94.
Relevant owners: crates/openshell-sandbox/src/process.rs and src/pty.rs.

The process allocates its PTY before applying the admitted numeric process identity.
The child must be able to reopen its controlling terminal after that privilege drop.
Qualification should inspect the allocated slave inode UID/GID/mode, run a guest as
UID 10001 with no supplementary groups, and prove opening /dev/tty and the selected
slave path succeeds only for the admitted child identity. Observe both direct PTY
and supervisor SSH paths. Compare stock behavior before claiming a released defect.

An upstream fix should assign the newly allocated slave descriptor to the admitted
UID/GID with mode 0600 before dropping privileges. It must not resolve a replaceable
path or add groups/capabilities to guest code. The provider release stays unchanged
in Veoveo while this issue is evaluated.
