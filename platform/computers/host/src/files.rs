use anyhow::{Context, Result, ensure};
use nix::{fcntl::OFlag, unistd::geteuid};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
};

pub fn read(path: &Path) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK).bits())
        .open(path)
        .context("open bounded host input")?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == geteuid().as_raw()
            && metadata.mode() & 0o022 == 0
            && metadata.len() <= 65536,
        "host input must be a private regular file"
    );
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= 65536,
        "host input exceeds its bound"
    );
    Ok(bytes)
}
pub fn directory(path: &Path) -> Result<()> {
    child_directory(path, true)
}
pub fn owned_directory(path: &Path) -> Result<()> {
    child_directory(path, false)
}
fn child_directory(path: &Path, private: bool) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && metadata.uid() == 0
            && if private {
                metadata.mode() & 0o777 == 0o700
            } else {
                metadata.mode() & 0o022 == 0
            }
            && fs::canonicalize(path)? == path,
        "host directory must have its required owner and permissions: {}",
        path.display()
    );
    Ok(())
}
pub fn lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(OFlag::O_NOFOLLOW.bits())
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o777 == 0o600,
        "invalid host lock"
    );
    file.try_lock()
        .context("compute host data is already owned")?;
    Ok(file)
}
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("new");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(OFlag::O_NOFOLLOW.bits())
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    File::open(path.parent().context("host file parent")?)?.sync_all()?;
    Ok(())
}
pub fn projected_file(root: &Path, name: &str) -> Result<Vec<u8>> {
    let root = fs::canonicalize(root)?;
    let path = fs::canonicalize(root.join(name))?;
    ensure!(
        path.starts_with(&root),
        "projected host input escaped its root"
    );
    read(&path)
}
pub fn trust() -> Result<()> {
    let target = Path::new(super::config::RUN).join("trust");
    directory(&target)?;
    for name in [
        "provider-ca.pem",
        "provider-server.pem",
        "provider-server-key.pem",
        "guest.pem",
        "guest-key.pem",
        "jwt-key.pem",
        "jwt-public.pem",
        "jwt-kid",
        "storage-ca.pem",
        "storage-server.pem",
        "storage-server-key.pem",
    ] {
        let bytes = projected_file(Path::new(super::config::TRUST), name)
            .with_context(|| format!("read host trust input {name}"))?;
        write(&target.join(name), &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn projected_inputs_are_bounded_and_cannot_escape_the_mounted_root() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("projection");
        fs::create_dir(&source).unwrap();
        let data = source.join("revision");
        fs::create_dir(&data).unwrap();
        fs::write(data.join("key"), b"test-only-key").unwrap();
        fs::set_permissions(data.join("key"), fs::Permissions::from_mode(0o600)).unwrap();
        symlink("revision/key", source.join("key")).unwrap();
        assert_eq!(projected_file(&source, "key").unwrap(), b"test-only-key");
        fs::write(temp.path().join("outside"), b"foreign").unwrap();
        symlink("../outside", source.join("escape")).unwrap();
        assert!(projected_file(&source, "escape").is_err());
        fs::write(data.join("key"), vec![0; 65537]).unwrap();
        assert!(projected_file(&source, "key").is_err());
    }
}
