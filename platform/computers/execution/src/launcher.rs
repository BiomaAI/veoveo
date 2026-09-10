use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    os::{fd::AsRawFd, unix::process::CommandExt},
    process::{Command, Stdio},
};

use nix::{
    fcntl::{OFlag, OpenHow, ResolveFlag, open, openat2},
    sys::stat::Mode,
    unistd::{fchdir, getegid, geteuid, getgid, getgroups, getuid},
};

use crate::{ExecutionRequest, LaunchError, RETAINED_HOME};

fn directory(relative: &str) -> Result<std::os::fd::OwnedFd, LaunchError> {
    let root = open(
        RETAINED_HOME,
        OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| LaunchError::DirectoryUnavailable)?;
    resolve_directory(&root, relative)
}

fn resolve_directory(
    root: &impl std::os::fd::AsFd,
    relative: &str,
) -> Result<std::os::fd::OwnedFd, LaunchError> {
    openat2(
        root,
        relative,
        OpenHow::new()
            .flags(OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC)
            .resolve(ResolveFlag::RESOLVE_BENEATH | ResolveFlag::RESOLVE_NO_MAGICLINKS),
    )
    .map_err(|_| LaunchError::DirectoryUnavailable)
}

fn program_input(bytes: &[u8]) -> Result<File, LaunchError> {
    // The qualified supervisor denies memfd_create to prevent fileless binary
    // execution. O_TMPFILE has no directory entry, including after a crash. There
    // is deliberately no named-file or weaker confinement fallback.
    let fd = open(
        "/tmp",
        OFlag::O_TMPFILE | OFlag::O_EXCL | OFlag::O_RDWR | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    )
    .map_err(|_| LaunchError::InputUnavailable)?;
    let mut file = File::from(fd);
    file.write_all(bytes)
        .map_err(|_| LaunchError::InputUnavailable)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| LaunchError::InputUnavailable)?;
    // Reopen our live descriptor read-only before dropping the writable handle.
    // This is a kernel-owned path derived from a live fd, never a request path.
    let reader = open(
        format!("/proc/self/fd/{}", file.as_raw_fd()).as_str(),
        OFlag::O_RDONLY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| LaunchError::InputUnavailable)?;
    Ok(File::from(reader))
}

/// Replaces the current process; call only from the single-threaded guest binary.
pub fn launch(request: ExecutionRequest) -> Result<std::convert::Infallible, LaunchError> {
    if getuid().as_raw() != 10001
        || geteuid().as_raw() != 10001
        || getgid().as_raw() != 10001
        || getegid().as_raw() != 10001
        || getgroups()
            .map_err(|_| LaunchError::WrongIdentity)?
            .iter()
            .any(|group| group.as_raw() != 10001)
    {
        return Err(LaunchError::WrongIdentity);
    }
    let working_directory = directory(&request.0.directory)?;
    let input = program_input(&request.0.stdin)?;
    let mut command = Command::new(&request.0.arguments[0]);
    command
        .args(&request.0.arguments[1..])
        .envs(&request.0.environment)
        .stdin(Stdio::from(input))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    fchdir(&working_directory).map_err(|_| LaunchError::DirectoryUnavailable)?;
    drop(working_directory);
    let _ = command.exec();
    Err(LaunchError::ProgramUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        io::Read,
        os::unix::{
            fs::{MetadataExt, symlink},
            io::AsRawFd,
        },
    };

    #[test]
    fn internal_symlinks_work_and_external_or_magic_links_fail() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("project")).unwrap();
        symlink("project", root.path().join("current")).unwrap();
        symlink("/etc", root.path().join("outside")).unwrap();
        let base = File::open(root.path()).unwrap();
        let current = File::from(resolve_directory(&base, "current").unwrap());
        assert_eq!(
            current.metadata().unwrap().ino(),
            fs::metadata(root.path().join("project")).unwrap().ino()
        );
        assert!(resolve_directory(&base, "outside").is_err());
        assert!(resolve_directory(&base, "..").is_err());
        let proc = File::open("/proc/self").unwrap();
        assert!(resolve_directory(&proc, &format!("fd/{}", base.as_raw_fd())).is_err());
    }

    #[test]
    fn selected_directory_keeps_its_inode_when_the_name_is_replaced() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("project")).unwrap();
        let base = File::open(root.path()).unwrap();
        let selected = File::from(resolve_directory(&base, "project").unwrap());
        let inode = selected.metadata().unwrap().ino();
        fs::rename(root.path().join("project"), root.path().join("old-project")).unwrap();
        fs::create_dir(root.path().join("project")).unwrap();
        assert_eq!(selected.metadata().unwrap().ino(), inode);
        assert_ne!(
            selected.metadata().unwrap().ino(),
            fs::metadata(root.path().join("project")).unwrap().ino()
        );
    }

    #[test]
    fn program_stdin_is_binary_exact_read_only_unnamed_and_has_eof() {
        let mut input = program_input(&[0, 255, 13, 10]).unwrap();
        assert_eq!(input.metadata().unwrap().nlink(), 0);
        let target = tempfile::tempdir().unwrap();
        assert!(
            nix::unistd::linkat(
                nix::fcntl::AT_FDCWD,
                format!("/proc/self/fd/{}", input.as_raw_fd()).as_str(),
                nix::fcntl::AT_FDCWD,
                &target.path().join("linked-input"),
                nix::fcntl::AtFlags::AT_SYMLINK_FOLLOW,
            )
            .is_err()
        );
        assert!(!target.path().join("linked-input").exists());
        let mut data = Vec::new();
        input.read_to_end(&mut data).unwrap();
        assert_eq!(data, [0, 255, 13, 10]);
        assert_eq!(input.read(&mut [0]).unwrap(), 0);
        assert!(input.write_all(b"changed").is_err());
        assert!(input.set_len(0).is_err());
    }
}
