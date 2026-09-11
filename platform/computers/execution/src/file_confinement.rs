//! Additional Landlock layer for the single-threaded file helper, never its caller.
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use nix::libc;

use crate::FileFailure;

#[repr(C)]
struct Ruleset {
    handled_access_fs: u64,
}

#[repr(C, packed)]
struct PathRule {
    allowed_access: u64,
    parent_fd: i32,
}

// Linux Landlock UAPI, filesystem access bits 0..14 (ABI 3).
const HANDLED: u64 = (1 << 15) - 1;
const READ: u64 = (1 << 2) | (1 << 3);
const WRITE: u64 = (1 << 1) | (1 << 5) | (1 << 8) | (1 << 13) | (1 << 14);

pub(crate) fn restrict(root: &OwnedFd, writing: bool) -> Result<(), FileFailure> {
    // SAFETY: ABI query takes no pointer; the version flag selects a read-only query.
    let abi = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0,
            1,
        )
    };
    if abi < 3 {
        return Err(FileFailure::ConfinementUnavailable);
    }
    let ruleset = Ruleset {
        handled_access_fs: HANDLED,
    };
    // SAFETY: ruleset is the version-1, eight-byte UAPI layout, live for the call.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &ruleset,
            std::mem::size_of::<Ruleset>(),
            0,
        )
    };
    if fd < 0 {
        return Err(FileFailure::ConfinementUnavailable);
    }
    // SAFETY: the successful syscall returned a fresh owned descriptor.
    let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
    let rule = PathRule {
        allowed_access: READ | if writing { WRITE } else { 0 },
        parent_fd: root.as_raw_fd(),
    };
    // SAFETY: both descriptors are live, rule is the packed path-beneath UAPI layout.
    let added = unsafe { libc::syscall(libc::SYS_landlock_add_rule, fd.as_raw_fd(), 1, &rule, 0) };
    if added != 0 {
        return Err(FileFailure::ConfinementUnavailable);
    }
    // SAFETY: this only narrows the helper's authority; no pointer arguments are used.
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        return Err(FileFailure::ConfinementUnavailable);
    }
    // SAFETY: main invokes this on its single thread; the ruleset descriptor is live.
    if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, fd.as_raw_fd(), 0) } != 0 {
        return Err(FileFailure::ConfinementUnavailable);
    }
    Ok(())
}
