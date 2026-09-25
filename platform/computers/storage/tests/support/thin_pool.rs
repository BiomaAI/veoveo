//! A disposable 768 MiB ext4 pool contains the 512 MiB test home. Exhaustion
//! affects only this file-backed filesystem, never the installation's disk.
use super::*;

pub fn mount() {
    let pool = root().parent().unwrap().to_path_buf();
    let backing = pool.with_extension("ext4");
    if !backing.exists() {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&backing)
            .unwrap();
        file.set_len(768 * 1024 * 1024).unwrap();
        checked(
            Command::new("mkfs.ext4")
                .args(["-F", "-q", "-m", "0"])
                .arg(&backing),
        );
    }
    fs::create_dir_all(&pool).unwrap();
    let device = checked(
        Command::new("losetup")
            .args(["--find", "--show", "--nooverlap"])
            .arg(&backing),
    );
    checked(Command::new("mount").arg(device).arg(pool));
}

pub async fn pressure(filesystem: &mut Filesystem, identity: &HomeIdentity) {
    let space = nix::sys::statvfs::statvfs(root().as_path()).unwrap();
    assert!(space.blocks_available() * space.fragment_size() < CAPACITY);
    let mut rejected = identity.clone();
    rejected.computer_id = Uuid::now_v7();
    assert!(matches!(
        filesystem.prepare(rejected.clone(), CAPACITY).await,
        Err(StorageError::CapacityExceeded)
    ));
    assert!(
        filesystem
            .journal()
            .load(rejected.computer_id)
            .unwrap()
            .is_none()
    );
    // Exhaust the outer pool, without sending writes to a loop-backed home when
    // its backing filesystem is full. Admission must fail without changing it.
    let fill = root().parent().unwrap().join("pool-fill");
    let mut file = fs::File::create(&fill).unwrap();
    let chunk = vec![0x33; 1024 * 1024];
    loop {
        match file.write(&chunk) {
            Ok(0) => panic!("zero-byte pool write"),
            Ok(_) => (),
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(nix::libc::ENOSPC));
                break;
            }
        }
    }
    assert!(matches!(
        filesystem.prepare(rejected, CAPACITY).await,
        Err(StorageError::CapacityExceeded)
    ));
    drop(file);
    fs::remove_file(fill).unwrap();
}

pub fn cleanup() {
    let pool = root().parent().unwrap().to_path_buf();
    let backing = pool.with_extension("ext4");
    checked(Command::new("umount").arg(&pool));
    detach(&backing);
    fs::remove_file(backing).unwrap();
    fs::remove_dir(pool).unwrap();
}
