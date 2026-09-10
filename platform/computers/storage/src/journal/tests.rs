//! Metadata-only tests. Sparse files here do not prove quota, ext4 or mount safety.
use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

// A concurrent fork can inherit another test's open lock until exec closes it.
// Keep these filesystem fixtures outside that transient inherited-FD window.
static CASE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn isolated_case() -> std::sync::MutexGuard<'static, ()> {
    CASE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn host() -> HostIdentity {
    HostIdentity {
        provider_id: Uuid::from_u128(100),
        engine_id: Uuid::from_u128(200),
        namespace: "provider-test".into(),
    }
}
fn home() -> HomeIdentity {
    HomeIdentity {
        provider_id: host().provider_id,
        computer_id: Uuid::from_u128(300),
        instance_id: Uuid::from_u128(300),
        template_fingerprint: "a".repeat(64),
    }
}
const CAPACITY: u64 = 512 * 1024 * 1024;
fn backing(journal: &Journal) -> PathBuf {
    journal
        .directory(home().computer_id)
        .unwrap()
        .join("home.ext4")
}
fn create_sparse(path: &Path) {
    private_options()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap()
        .set_len(CAPACITY)
        .unwrap();
}

#[test]
fn lock_and_host_identity_survive_reopening_without_adopting_another_engine() {
    if let Some(root) = std::env::var_os("VEOVEO_STORAGE_JOURNAL_CHILD_ROOT") {
        let result = Journal::open(root.into(), host());
        if std::env::var("VEOVEO_STORAGE_JOURNAL_CHILD_STATE").unwrap() == "locked" {
            assert!(matches!(result, Err(StorageError::Busy)));
        } else {
            result.unwrap();
        }
        return;
    }
    let _case = isolated_case();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("retained");
    let journal = Journal::open(root.clone(), host()).unwrap();
    assert!(matches!(
        Journal::open(root.clone(), host()),
        Err(StorageError::Busy)
    ));
    assert_child_lock(&root, "locked");
    drop(journal);
    assert_child_lock(&root, "released");
    for field in 0..3 {
        let mut foreign = host();
        match field {
            0 => foreign.engine_id = Uuid::from_u128(201),
            1 => foreign.provider_id = Uuid::from_u128(101),
            _ => foreign.namespace = "another-provider".into(),
        }
        assert!(matches!(
            Journal::open(root.clone(), foreign),
            Err(StorageError::IdentityMismatch)
        ));
    }
    Journal::open(root, host()).unwrap();
}

fn assert_child_lock(root: &Path, state: &str) {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "journal::tests::lock_and_host_identity_survive_reopening_without_adopting_another_engine", "--nocapture"])
        .env("VEOVEO_STORAGE_JOURNAL_CHILD_ROOT", root)
        .env("VEOVEO_STORAGE_JOURNAL_CHILD_STATE", state)
        .output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn incomplete_reservations_retain_identity_capacity_and_bytes_across_restart() {
    let _case = isolated_case();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("retained");
    let mut journal = Journal::open(root.clone(), host()).unwrap();
    let reservation = journal.reserve(home(), CAPACITY).unwrap();
    assert!(matches!(reservation, Reservation::Created(_)));
    let original = reservation.record().clone();
    assert_eq!(original.state(), &AllocationState::Allocating);
    let path = backing(&journal);
    private_options()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap()
        .write_all(b"incomplete bytes")
        .unwrap();
    drop(journal);
    let mut journal = Journal::open(root, host()).unwrap();
    let resumed = journal.reserve(home(), CAPACITY).unwrap();
    assert!(matches!(resumed, Reservation::Existing(_)));
    assert_eq!(resumed.record(), &original);
    assert_eq!(fs::read(&path).unwrap(), b"incomplete bytes");
    assert!(matches!(
        journal.record_ready(&home()),
        Err(StorageError::RecoveryRequired)
    ));
    for field in 0..4 {
        let mut foreign = home();
        let mut capacity = CAPACITY;
        match field {
            0 => foreign.instance_id = Uuid::from_u128(400),
            1 => foreign.template_fingerprint = "b".repeat(64),
            2 => foreign.provider_id = Uuid::from_u128(101),
            _ => capacity *= 2,
        }
        assert!(matches!(
            journal.reserve(foreign, capacity),
            Err(StorageError::IdentityMismatch)
        ));
    }
    assert_eq!(journal.load(home().computer_id).unwrap().unwrap(), original);
}

#[test]
fn ready_record_is_idempotent_and_rejects_backing_file_substitution() {
    let _case = isolated_case();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("retained");
    let mut journal = Journal::open(root.clone(), host()).unwrap();
    journal.reserve(home(), CAPACITY).unwrap();
    let path = backing(&journal);
    create_sparse(&path);
    let ready = journal.record_ready(&home()).unwrap();
    assert!(matches!(ready.state(), AllocationState::Ready { .. }));
    assert_eq!(journal.record_ready(&home()).unwrap(), ready);
    drop(journal);
    let mut journal = Journal::open(root, host()).unwrap();
    assert_eq!(journal.load(home().computer_id).unwrap().unwrap(), ready);
    fs::rename(&path, path.with_extension("original")).unwrap();
    create_sparse(&path);
    assert!(matches!(
        journal.record_ready(&home()),
        Err(StorageError::RecoveryRequired)
    ));
    assert_eq!(journal.load(home().computer_id).unwrap().unwrap(), ready);
}

#[test]
fn unknown_initial_state_missing_records_and_metadata_links_fail_closed() {
    let _case = isolated_case();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("retained");
    let mut journal = Journal::open(root.clone(), host()).unwrap();
    let mut replacement = home();
    replacement.instance_id = Uuid::from_u128(400);
    assert!(matches!(
        journal.reserve(replacement, CAPACITY),
        Err(StorageError::IdentityMismatch)
    ));
    let directory = journal.directory(home().computer_id).unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    assert!(matches!(
        journal.reserve(home(), CAPACITY),
        Err(StorageError::RecoveryRequired)
    ));
    let outside = temporary.path().join("outside");
    private_options()
        .write(true)
        .create_new(true)
        .open(&outside)
        .unwrap()
        .write_all(b"preserve")
        .unwrap();
    symlink(&outside, directory.join("record.json")).unwrap();
    assert!(journal.load(home().computer_id).is_err());
    assert_eq!(fs::read(&outside).unwrap(), b"preserve");
    drop(journal);
    fs::remove_file(root.join("host.json")).unwrap();
    assert!(matches!(
        Journal::open(root, host()),
        Err(StorageError::RecoveryRequired)
    ));
}

#[test]
fn malformed_private_records_and_non_private_paths_are_not_repaired_implicitly() {
    let _case = isolated_case();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("retained");
    let mut journal = Journal::open(root.clone(), host()).unwrap();
    journal.reserve(home(), CAPACITY).unwrap();
    let record = journal
        .directory(home().computer_id)
        .unwrap()
        .join("record.json");
    let original = fs::read(&record).unwrap();
    for bytes in [
        b"{}".to_vec(),
        b"null".to_vec(),
        b"{".to_vec(),
        vec![b' '; 8193],
        {
            let mut duplicate = original[..original.len() - 1].to_vec();
            duplicate.extend_from_slice(b",\"schema\":\"veoveo.io/retained-home/v1\"}");
            duplicate
        },
    ] {
        fs::write(&record, &bytes).unwrap();
        assert!(matches!(
            journal.load(home().computer_id),
            Err(StorageError::RecoveryRequired)
        ));
        assert_eq!(fs::read(&record).unwrap(), bytes);
    }
    fs::write(&record, original).unwrap();
    fs::hard_link(&record, temporary.path().join("linked-record")).unwrap();
    assert!(matches!(
        journal.load(home().computer_id),
        Err(StorageError::RecoveryRequired)
    ));
    drop(journal);
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        Journal::open(root, host()),
        Err(StorageError::InvalidIdentity)
    ));
}
