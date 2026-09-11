use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::fs::MetadataExt,
    },
    path::Path,
};

use nix::{
    errno::Errno,
    fcntl::{AtFlags, OFlag, OpenHow, ResolveFlag, open, openat2},
    sys::stat::{Mode, fstat},
    unistd::linkat,
};
use sha2::{Digest, Sha256};

use crate::{FileFailure, FileReceipt, FileRequest, RETAINED_HOME, files::FileOperation};

const RESOLVE: ResolveFlag = ResolveFlag::RESOLVE_BENEATH
    .union(ResolveFlag::RESOLVE_NO_SYMLINKS)
    .union(ResolveFlag::RESOLVE_NO_XDEV);

/// Applies irreversible restrictions to this thread. Only the standalone helper
/// may call it; a provider/service process must invoke the packaged executable.
pub fn transfer_file(
    request: FileRequest,
    input: &mut impl Read,
    output: &mut impl Write,
) -> Result<FileReceipt, FileFailure> {
    crate::launcher::check_identity().map_err(|_| FileFailure::WrongIdentity)?;
    let root = open(
        RETAINED_HOME,
        OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| FileFailure::PathUnavailable)?;
    crate::file_confinement::restrict(
        &root,
        matches!(request.0.operation, FileOperation::Import { .. }),
    )?;
    match request.0.operation {
        FileOperation::Import { bytes, sha256 } => {
            import(&root, &request.0.path, bytes, sha256, input)
        }
        FileOperation::Export { maximum_bytes } => {
            export(&root, &request.0.path, maximum_bytes, output)
        }
    }
}

fn import(
    root: &OwnedFd,
    path: &str,
    bytes: u64,
    expected: [u8; 32],
    input: &mut impl Read,
) -> Result<FileReceipt, FileFailure> {
    let path = Path::new(path);
    let parent_path = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let parent = openat2(
        root,
        parent_path,
        OpenHow::new()
            .flags(OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC)
            .resolve(RESOLVE),
    )
    .map_err(|_| FileFailure::PathUnavailable)?;
    // Anonymous retained-home allocation is charged to its filesystem quota and
    // vanishes on interruption before publication. No named temporary path can be replaced.
    let temporary = openat2(
        &parent,
        ".",
        OpenHow::new()
            .flags(OFlag::O_TMPFILE | OFlag::O_RDWR | OFlag::O_CLOEXEC)
            .mode(Mode::S_IRUSR | Mode::S_IWUSR)
            .resolve(RESOLVE),
    )
    .map_err(|_| FileFailure::Storage)?;
    let mut temporary = File::from(temporary);
    let actual = copy_exact(input, &mut temporary, bytes, FileFailure::Storage)?;
    if actual != expected {
        return Err(FileFailure::Integrity);
    }
    temporary.sync_all().map_err(|_| FileFailure::Storage)?;
    let current_parent = openat2(
        root,
        parent_path,
        OpenHow::new()
            .flags(OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC)
            .resolve(RESOLVE),
    )
    .map_err(|_| FileFailure::FileChanged)?;
    let before = fstat(&parent).map_err(|_| FileFailure::FileChanged)?;
    let current = fstat(&current_parent).map_err(|_| FileFailure::FileChanged)?;
    if (before.st_dev, before.st_ino) != (current.st_dev, current.st_ino) {
        return Err(FileFailure::FileChanged);
    }
    // This procfs path is generated from our live descriptor, never request data.
    // Unprivileged O_TMPFILE publication uses the descriptor symlink; AT_EMPTY_PATH
    // would require CAP_DAC_READ_SEARCH. linkat atomically refuses an existing name.
    linkat(
        nix::fcntl::AT_FDCWD,
        format!("/proc/self/fd/{}", temporary.as_raw_fd()).as_str(),
        root,
        path,
        AtFlags::AT_SYMLINK_FOLLOW,
    )
    .map_err(|error| {
        if error == Errno::EEXIST {
            FileFailure::DestinationExists
        } else {
            FileFailure::Storage
        }
    })?;
    // Publication has occurred. A concurrent rename now makes the requested
    // path uncertain; do not report a rollback or remove somebody else's file.
    let published = openat2(
        root,
        path,
        OpenHow::new()
            .flags(OFlag::O_PATH | OFlag::O_CLOEXEC)
            .resolve(RESOLVE),
    )
    .map_err(|_| FileFailure::CommitUnknown)?;
    let published = fstat(&published).map_err(|_| FileFailure::CommitUnknown)?;
    let temporary_stat = fstat(&temporary).map_err(|_| FileFailure::CommitUnknown)?;
    if (published.st_dev, published.st_ino) != (temporary_stat.st_dev, temporary_stat.st_ino) {
        return Err(FileFailure::CommitUnknown);
    }
    File::from(parent)
        .sync_all()
        .map_err(|_| FileFailure::CommitUnknown)?;
    Ok(FileReceipt {
        bytes,
        sha256: actual,
    })
}

fn export(
    root: &OwnedFd,
    path: &str,
    limit: u64,
    output: &mut impl Write,
) -> Result<FileReceipt, FileFailure> {
    let fd = openat2(
        root,
        path,
        OpenHow::new()
            .flags(OFlag::O_RDONLY | OFlag::O_NONBLOCK | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC)
            .resolve(RESOLVE),
    )
    .map_err(|_| FileFailure::PathUnavailable)?;
    let mut file = File::from(fd);
    let before = file.metadata().map_err(|_| FileFailure::PathUnavailable)?;
    if !before.is_file() || before.nlink() != 1 {
        return Err(FileFailure::UnsupportedFile);
    }
    if before.len() > limit {
        return Err(FileFailure::TooLarge);
    }
    let hash = copy_exact(
        &mut file,
        output,
        before.len(),
        FileFailure::OutputUnavailable,
    )?;
    let after = file.metadata().map_err(|_| FileFailure::FileChanged)?;
    if (
        before.len(),
        before.mtime(),
        before.mtime_nsec(),
        before.ctime(),
        before.ctime_nsec(),
        before.nlink(),
    ) != (
        after.len(),
        after.mtime(),
        after.mtime_nsec(),
        after.ctime(),
        after.ctime_nsec(),
        after.nlink(),
    ) {
        return Err(FileFailure::FileChanged);
    }
    Ok(FileReceipt {
        bytes: before.len(),
        sha256: hash,
    })
}

fn copy_exact(
    input: &mut impl Read,
    output: &mut impl Write,
    mut remaining: u64,
    write_error: FileFailure,
) -> Result<[u8; 32], FileFailure> {
    let mut buffer = zeroize::Zeroizing::new([0u8; 64 * 1024]);
    let mut digest = Sha256::new();
    while remaining != 0 {
        let size = buffer.len().min(remaining as usize);
        input
            .read_exact(&mut buffer[..size])
            .map_err(|_| FileFailure::InputIncomplete)?;
        output.write_all(&buffer[..size]).map_err(|_| write_error)?;
        digest.update(&buffer[..size]);
        remaining -= size as u64;
    }
    output.flush().map_err(|_| write_error)?;
    Ok(digest.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_export_mutation_invalidates_the_result() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("source");
        std::fs::write(&path, [7; 1024]).unwrap();
        struct MutatingOutput(std::path::PathBuf);
        impl Write for MutatingOutput {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                std::fs::write(&self.0, [9; 2048])?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let root = open(
            home.path(),
            OFlag::O_PATH | OFlag::O_DIRECTORY,
            Mode::empty(),
        )
        .unwrap();
        assert_eq!(
            export(&root, "source", 1024, &mut MutatingOutput(path)),
            Err(FileFailure::FileChanged)
        );
    }
}
