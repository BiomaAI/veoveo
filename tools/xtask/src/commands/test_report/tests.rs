use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Barrier},
};

use chrono::{Duration, Utc};
use uuid::Uuid;

use super::{catalog, inputs, model::*, report, storage};

fn repository() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    crate::process::output("git", ["init", "--quiet"], Some(directory.path())).unwrap();
    write(
        directory.path(),
        "service/src/lib.rs",
        "pub fn value() -> u8 { 1 }\n",
    );
    write(directory.path(), "shared/schema.json", "{\"version\":1}\n");
    write(directory.path(), "apps/console/web/main.ts", "first\n");
    write(directory.path(), "rust-toolchain.toml", "first\n");
    directory
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn source(root: &Path) -> SourceInputs {
    let roots = vec![
        "service".to_owned(),
        "shared".to_owned(),
        "rust-toolchain.toml".to_owned(),
    ];
    inputs::snapshot_with_roots(
        root,
        &InputScope::Console {
            roots: roots.clone(),
        },
        &roots,
    )
    .unwrap()
}

fn receipt(root: &Path, name: &str) -> Receipt {
    let arguments = vec!["cargo".to_owned(), "test".to_owned()];
    Receipt {
        schema_version: RECEIPT_SCHEMA.to_owned(),
        run_id: Uuid::new_v4(),
        name: name.to_owned(),
        check_id: catalog::check_id(&arguments).unwrap(),
        command: CommandIdentity::Admitted { arguments },
        provenance: SourceProvenance {
            revision: Some("a".repeat(40)),
            dirty: false,
        },
        inputs: source(root),
        environment: EnvironmentIdentity {
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            toolchains: vec![],
            runtime: RuntimeIdentity {
                class: EvidenceClass::Source,
                bindings: BTreeMap::new(),
                valid_for_seconds: None,
            },
            reusable: true,
        },
        started_at: Utc::now() - Duration::seconds(2),
        finished_at: Utc::now(),
        duration_millis: 1,
        outcome: Outcome::Passed,
        exit_code: Some(0),
        diagnostics: None,
    }
}

#[test]
fn selected_receipts_validate_superseded_history_and_reject_unindexed_attempts() {
    let repo = repository();
    let old = receipt(repo.path(), "old-pass");
    storage::publish(repo.path(), &old).unwrap();
    let mut new = receipt(repo.path(), "new-failure");
    new.outcome = Outcome::Failed;
    new.exit_code = Some(1);
    storage::publish(repo.path(), &new).unwrap();
    let selected = storage::read_latest(repo.path()).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[&new.check_id].run_id, new.run_id);
    assert_eq!(selected[&new.check_id].outcome, Outcome::Failed);
    let path = repo
        .path()
        .join(RECEIPT_DIRECTORY)
        .join(format!("{}.json", old.run_id));
    let original = fs::read(&path).unwrap();
    let mut changed = original.clone();
    changed.push(b' ');
    fs::write(&path, changed).unwrap();
    assert!(
        storage::read_latest(repo.path()).is_err(),
        "superseded receipt integrity still matters"
    );
    fs::write(&path, original).unwrap();
    let missing = repo
        .path()
        .join(RECEIPT_DIRECTORY)
        .join(format!("{}.json", Uuid::new_v4()));
    fs::copy(path, missing).unwrap();
    assert!(
        storage::read_latest(repo.path()).is_err(),
        "unindexed publication must be recovered first"
    );
}

#[test]
fn unrelated_console_edit_retains_service_but_shared_contract_and_toolchain_invalidate() {
    let repo = repository();
    let before = source(repo.path());
    write(repo.path(), "apps/console/web/main.ts", "other\n");
    assert_eq!(source(repo.path()), before);
    write(repo.path(), "shared/schema.json", "{\"version\":2}\n");
    assert_ne!(source(repo.path()), before);
    let shared = source(repo.path());
    write(repo.path(), "rust-toolchain.toml", "other\n");
    assert_ne!(source(repo.path()), shared);
}

#[test]
fn additions_deletions_and_equal_length_edits_change_inputs() {
    let repo = repository();
    let before = source(repo.path());
    write(repo.path(), "service/src/new.rs", "new\n");
    let added = source(repo.path());
    assert_ne!(added, before);
    fs::remove_file(repo.path().join("service/src/new.rs")).unwrap();
    assert_eq!(source(repo.path()), before);
    write(
        repo.path(),
        "service/src/lib.rs",
        "pub fn value() -> u8 { 2 }\n",
    );
    assert_ne!(source(repo.path()), before);
    crate::process::output("git", ["add", "service"], Some(repo.path())).unwrap();
    fs::remove_file(repo.path().join("service/src/lib.rs")).unwrap();
    let removed = source(repo.path());
    assert!(
        !removed
            .files
            .iter()
            .any(|input| input.path == "service/src/lib.rs")
    );
}

#[test]
fn materialized_bytes_are_not_hidden_by_git_clean_filters() {
    let repo = repository();
    write(repo.path(), ".gitattributes", "*.rs filter=constant\n");
    crate::process::output(
        "git",
        ["config", "filter.constant.clean", "printf constant"],
        Some(repo.path()),
    )
    .unwrap();
    let before = source(repo.path());
    write(repo.path(), "service/src/lib.rs", "changed input\n");
    assert_ne!(source(repo.path()), before);
}

#[test]
fn identical_materialized_inputs_share_identity_across_worktrees() {
    let first = repository();
    let second = repository();
    assert_eq!(source(first.path()), source(second.path()));
}

#[cfg(unix)]
#[test]
fn symlink_targets_participate_and_escape_or_ignored_secrets_are_rejected() {
    use std::os::unix::fs::symlink;
    let repo = repository();
    symlink(
        "../../shared/schema.json",
        repo.path().join("service/src/schema.json"),
    )
    .unwrap();
    let first = source(repo.path());
    write(repo.path(), "shared/next.json", "next\n");
    fs::remove_file(repo.path().join("service/src/schema.json")).unwrap();
    symlink(
        "../../shared/next.json",
        repo.path().join("service/src/schema.json"),
    )
    .unwrap();
    assert_ne!(source(repo.path()), first);
    fs::remove_file(repo.path().join("service/src/schema.json")).unwrap();
    symlink("/etc/hostname", repo.path().join("service/src/schema.json")).unwrap();
    let roots = vec!["service".to_owned()];
    assert!(inputs::snapshot_with_roots(repo.path(), &InputScope::Repository, &roots).is_err());
    fs::remove_file(repo.path().join("service/src/schema.json")).unwrap();
    write(repo.path(), ".gitignore", "secret\n");
    write(repo.path(), "secret", "must not hash this\n");
    symlink("../../secret", repo.path().join("service/src/schema.json")).unwrap();
    assert!(inputs::snapshot_with_roots(repo.path(), &InputScope::Repository, &roots).is_err());
}

#[test]
fn evidence_outputs_never_invalidate_their_own_receipts() {
    let repo = repository();
    let before = inputs::snapshot(repo.path(), &InputScope::Repository).unwrap();
    storage::publish(repo.path(), &receipt(repo.path(), "first")).unwrap();
    assert_eq!(
        inputs::snapshot(repo.path(), &InputScope::Repository).unwrap(),
        before
    );
}

#[test]
fn concurrent_publication_retains_every_attempt_and_create_only_rejects_rewrites() {
    let repo = repository();
    let barrier = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        for number in 0..8 {
            let barrier = barrier.clone();
            let mut attempt = receipt(repo.path(), &format!("check-{number}"));
            let arguments = vec![
                "cargo".to_owned(),
                "test".to_owned(),
                format!("test-{number}"),
            ];
            attempt.check_id = catalog::check_id(&arguments).unwrap();
            attempt.command = CommandIdentity::Admitted { arguments };
            let root = repo.path();
            scope.spawn(move || {
                barrier.wait();
                storage::publish(root, &attempt).unwrap();
            });
        }
    });
    let index = storage::read_index(repo.path()).unwrap();
    assert_eq!(index.receipts.len(), 8);
    assert_eq!(index.latest.len(), 8);
    let attempt = storage::read_receipt(repo.path(), &index.receipts[0]).unwrap();
    assert!(storage::publish(repo.path(), &attempt).is_err());
    assert_eq!(storage::read_index(repo.path()).unwrap().receipts.len(), 8);
}

#[test]
fn renamed_failure_is_selected_and_late_changed_input_run_cannot_replace_it() {
    let repo = repository();
    let mut pass = receipt(repo.path(), "first");
    pass.finished_at -= Duration::seconds(1);
    storage::publish(repo.path(), &pass).unwrap();
    let mut failed = receipt(repo.path(), "renamed");
    failed.outcome = Outcome::Failed;
    failed.exit_code = Some(1);
    storage::publish(repo.path(), &failed).unwrap();
    let mut changed = receipt(repo.path(), "late");
    changed.outcome = Outcome::InputsChanged;
    storage::publish(repo.path(), &changed).unwrap();
    let index = storage::read_index(repo.path()).unwrap();
    assert_eq!(index.receipts.len(), 3);
    assert_eq!(index.latest.get(&failed.check_id), Some(&failed.run_id));
}

#[test]
fn modified_receipt_is_rejected_instead_of_blessed_during_rebuild() {
    let repo = repository();
    let attempt = receipt(repo.path(), "first");
    let reference = storage::publish(repo.path(), &attempt).unwrap();
    let path = repo
        .path()
        .join(RECEIPT_DIRECTORY)
        .join(format!("{}.json", attempt.run_id));
    let mut content = fs::read(&path).unwrap();
    content.push(b' ');
    fs::write(path, content).unwrap();
    assert!(storage::read_receipt(repo.path(), &reference).is_err());
    assert!(storage::publish(repo.path(), &receipt(repo.path(), "second")).is_err());
}

#[test]
fn complete_unindexed_receipt_is_recovered_by_next_publication() {
    let repo = repository();
    let orphan = receipt(repo.path(), "orphan");
    write(
        repo.path(),
        &format!("{RECEIPT_DIRECTORY}/{}.json", orphan.run_id),
        &serde_json::to_string(&orphan).unwrap(),
    );
    storage::publish(repo.path(), &receipt(repo.path(), "next")).unwrap();
    assert_eq!(storage::read_index(repo.path()).unwrap().receipts.len(), 2);
}

#[test]
fn coverage_rejects_gaps_failures_expiry_and_changed_configuration() {
    let repo = repository();
    let mut attempt = receipt(repo.path(), "installed-fixture");
    attempt.environment.runtime.class = EvidenceClass::Installed;
    attempt
        .environment
        .runtime
        .bindings
        .insert("configuration".to_owned(), "sha256:fixture-one".to_owned());
    attempt.environment.runtime.valid_for_seconds = Some(300);
    let profile = CoverageProfile {
        schema_version: PROFILE_SCHEMA.to_owned(),
        name: "fixture".to_owned(),
        requirements: vec![CoverageRequirement {
            check_id: attempt.check_id.clone(),
            class: EvidenceClass::Installed,
            max_age_seconds: Some(60),
            bindings: attempt.environment.runtime.bindings.clone(),
        }],
    };
    assert!(report::validate_coverage(&profile, &BTreeMap::new(), Utc::now()).is_err());
    let mut selected = BTreeMap::from([(attempt.check_id.clone(), attempt.clone())]);
    assert!(report::validate_coverage(&profile, &selected, Utc::now()).is_ok());
    assert!(
        report::validate_coverage(&profile, &selected, Utc::now() + Duration::seconds(61)).is_err()
    );
    selected
        .get_mut(&attempt.check_id)
        .unwrap()
        .environment
        .runtime
        .bindings
        .insert("configuration".to_owned(), "sha256:fixture-two".to_owned());
    assert!(report::validate_coverage(&profile, &selected, Utc::now()).is_err());
    selected.insert(attempt.check_id.clone(), attempt.clone());
    selected.get_mut(&attempt.check_id).unwrap().outcome = Outcome::Failed;
    assert!(report::validate_coverage(&profile, &selected, Utc::now()).is_err());
}

#[test]
fn selectors_define_identity_and_opaque_commands_do_not_persist_arguments() {
    assert_ne!(
        catalog::check_id(&["cargo".to_owned(), "test".to_owned(), "one".to_owned()]).unwrap(),
        catalog::check_id(&["cargo".to_owned(), "test".to_owned(), "two".to_owned()]).unwrap()
    );
    let repo = repository();
    assert!(
        catalog::lookup(
            repo.path(),
            &["env".into(), "TOKEN=secret".into(), "cargo".into()]
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn committing_a_deletion_does_not_change_materialized_identity() {
    let repo = repository();
    crate::process::output("git", ["add", "."], Some(repo.path())).unwrap();
    fs::remove_file(repo.path().join("service/src/lib.rs")).unwrap();
    let before = source(repo.path());
    crate::process::output("git", ["add", "-u"], Some(repo.path())).unwrap();
    assert_eq!(source(repo.path()), before);
}

#[test]
fn recorder_detects_inputs_changed_by_the_actual_command() {
    let repo = repository();
    let context = crate::context::RepositoryContext::discover(repo.path()).unwrap();
    let result = super::run(
        &context,
        "mutating-command",
        &[
            "sh".into(),
            "-c".into(),
            "printf changed > service/src/lib.rs".into(),
        ],
    );
    assert!(result.is_err());
    let index = storage::read_index(repo.path()).unwrap();
    assert!(index.latest.is_empty());
    let attempt = storage::read_receipt(repo.path(), &index.receipts[0]).unwrap();
    assert_eq!(attempt.outcome, Outcome::InputsChanged);
}

#[test]
fn an_unindexed_completed_failure_prevents_presenting_an_old_pass() {
    let repo = repository();
    storage::publish(repo.path(), &receipt(repo.path(), "first")).unwrap();
    let mut failed = receipt(repo.path(), "failed");
    failed.outcome = Outcome::Failed;
    failed.exit_code = Some(1);
    write(
        repo.path(),
        &format!("{RECEIPT_DIRECTORY}/{}.json", failed.run_id),
        &serde_json::to_string(&failed).unwrap(),
    );
    let index = storage::read_index(repo.path()).unwrap();
    assert!(storage::ensure_index_complete(repo.path(), &index).is_err());
    storage::publish(repo.path(), &receipt(repo.path(), "next")).unwrap();
    storage::ensure_index_complete(repo.path(), &storage::read_index(repo.path()).unwrap())
        .unwrap();
}

#[test]
fn adding_another_owners_checks_preserves_the_selected_declaration() {
    let repo = repository();
    let args = ["cargo".to_owned(), "test".to_owned()];
    let own = format!("{CATALOG_DIRECTORY}/service.json");
    let definition = CheckDefinition {
        arguments: args.to_vec(),
        inputs: InputScope::Console {
            roots: vec!["service".to_owned(), "testing".to_owned()],
        },
        toolchains: vec![],
    };
    let mut catalog = CheckCatalog {
        schema_version: CATALOG_SCHEMA.to_owned(),
        checks: vec![definition],
    };
    write(repo.path(), &own, &serde_json::to_string(&catalog).unwrap());
    let selected = catalog::lookup(
        repo.path(),
        &args.iter().map(Into::into).collect::<Vec<_>>(),
    )
    .unwrap()
    .unwrap();
    let before = inputs::snapshot(repo.path(), &selected.inputs).unwrap();
    assert!(before.files.iter().any(|input| input.path == own));
    catalog.checks[0].arguments.push("another".to_owned());
    write(
        repo.path(),
        &format!("{CATALOG_DIRECTORY}/another.json"),
        &serde_json::to_string(&catalog).unwrap(),
    );
    assert_eq!(
        inputs::snapshot(repo.path(), &selected.inputs).unwrap(),
        before
    );
    catalog.checks[0].arguments.pop();
    catalog.checks[0].toolchains.push(Toolchain::Rust);
    write(repo.path(), &own, &serde_json::to_string(&catalog).unwrap());
    assert_ne!(
        inputs::snapshot(repo.path(), &selected.inputs).unwrap(),
        before
    );
}
