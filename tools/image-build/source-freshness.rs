//! Bridge content-addressed BuildKit inputs to Cargo's timestamp freshness checks.
//!
//! Compile with the family's existing rustc. The writable /src bind is disposable;
//! the byte-for-byte input mirror lives beside its locked Cargo target cache.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Error, ErrorKind},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(PartialEq, Eq)]
enum Content {
    Directory(u32),
    File(Vec<u8>, u32),
    Link(PathBuf),
}

struct Entry {
    content: Content,
    modified: SystemTime,
}

struct FreshnessChanges {
    paths: usize,
    refreshed_files: usize,
    removed_paths: usize,
}

fn entries(root: &Path) -> io::Result<BTreeMap<PathBuf, Entry>> {
    fn visit(
        root: &Path,
        directory: &Path,
        result: &mut BTreeMap<PathBuf, Entry>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            let relative = path.strip_prefix(root).map_err(Error::other)?.to_owned();
            let content = if metadata.is_symlink() {
                Content::Link(fs::read_link(&path)?)
            } else if metadata.is_dir() {
                visit(root, &path, result)?;
                Content::Directory(metadata.permissions().mode())
            } else if metadata.is_file() {
                Content::File(fs::read(&path)?, metadata.permissions().mode())
            } else {
                return Err(Error::other(format!(
                    "unsupported compiler input: {}",
                    path.display()
                )));
            };
            result.insert(
                relative,
                Entry {
                    content,
                    modified: metadata.modified()?,
                },
            );
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result)?;
    Ok(result)
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn stamp(path: &Path, modified: SystemTime) -> io::Result<()> {
    fs::File::open(path)?.set_times(fs::FileTimes::new().set_modified(modified))
}

fn synchronize(source: &Path, state: &Path) -> io::Result<FreshnessChanges> {
    fs::create_dir_all(state)?;
    let source = source.canonicalize()?;
    let state = state.canonicalize()?;
    if source.starts_with(&state) || state.starts_with(&source) {
        return Err(Error::other(
            "compiler input and freshness state must be separate trees",
        ));
    }
    let current = entries(&source)?;
    let previous = entries(&state)?;
    let previous_root_modified = fs::metadata(&state)?.modified()?;
    // Symlinks can hide the path Cargo actually recorded. Keep conservative
    // invalidation for link changes. Ordinary removal needs only its directory
    // watchers: a recorded dependency that disappeared is already stale to Cargo.
    let changed_link = previous.iter().any(|(path, entry)| {
        matches!(entry.content, Content::Link(_))
            && current
                .get(path)
                .is_none_or(|new| new.content != entry.content)
    }) || current.iter().any(|(path, entry)| {
        matches!(entry.content, Content::Link(_))
            && previous
                .get(path)
                .is_none_or(|old| old.content != entry.content)
    });
    let now = SystemTime::now();
    if previous.values().any(|entry| entry.modified >= now)
        || (!previous.is_empty() && previous_root_modified >= now)
    {
        return Err(Error::other(
            "compiler freshness clock did not advance; refusing stale cache reuse",
        ));
    }
    let mut changed = previous
        .keys()
        .filter(|path| !current.contains_key(*path))
        .cloned()
        .collect::<BTreeSet<_>>();
    for (path, entry) in &current {
        if changed_link
            || previous
                .get(path)
                .is_none_or(|old| old.content != entry.content)
        {
            changed.insert(path.clone());
        }
    }
    // Parent directory times matter to build scripts with rerun-if-changed.
    for path in changed.clone() {
        for parent in path
            .ancestors()
            .skip(1)
            .filter(|path| !path.as_os_str().is_empty())
        {
            changed.insert(parent.to_owned());
        }
    }
    for (path, old) in previous.iter().rev() {
        let compatible = current.get(path).is_some_and(|entry| {
            std::mem::discriminant(&entry.content) == std::mem::discriminant(&old.content)
        });
        if !compatible {
            remove(&state.join(path))?;
        }
    }
    for (path, entry) in &current {
        let destination = state.join(path);
        match &entry.content {
            Content::Directory(mode) => {
                fs::create_dir_all(&destination)?;
                fs::set_permissions(&destination, fs::Permissions::from_mode(*mode))?;
            }
            Content::File(bytes, mode) => {
                if changed.contains(path) {
                    fs::write(&destination, bytes)?;
                    fs::set_permissions(&destination, fs::Permissions::from_mode(*mode))?;
                }
            }
            Content::Link(target) => {
                let resolved = source.join(path).canonicalize()?;
                if target.is_absolute()
                    || !resolved.starts_with(&source)
                    || !current.contains_key(resolved.strip_prefix(&source).map_err(Error::other)?)
                {
                    return Err(Error::other(format!(
                        "compiler input link escapes its declared closure: {}",
                        path.display()
                    )));
                }
                if changed.contains(path) {
                    remove(&destination)?;
                    symlink(target, &destination)?;
                }
            }
        }
    }
    // Stamp after all writes, because inserting children also changes a directory.
    for (path, entry) in &current {
        if !matches!(entry.content, Content::Link(_)) {
            let modified = if changed.contains(path) {
                now
            } else {
                previous[path].modified
            };
            stamp(&state.join(path), modified)?;
            stamp(&source.join(path), modified)?;
        }
    }
    // A build script can watch the workspace root itself. This also covers a
    // removed top-level input whose ancestors contain no nonempty relative path.
    let root_modified = if changed.is_empty() {
        previous_root_modified
    } else {
        now
    };
    stamp(&state, root_modified)?;
    stamp(&source, root_modified)?;
    Ok(FreshnessChanges {
        paths: changed.len(),
        refreshed_files: current
            .iter()
            .filter(|(path, entry)| {
                changed.contains(*path) && matches!(entry.content, Content::File(..))
            })
            .count(),
        removed_paths: previous
            .keys()
            .filter(|path| !current.contains_key(*path))
            .count(),
    })
}

#[cfg(not(test))]
fn main() -> io::Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 2 {
        return Err(Error::other(
            "usage: veoveo-source-freshness <writable-source> <locked-target-input-state>",
        ));
    }
    let changed = synchronize(Path::new(&arguments[0]), Path::new(&arguments[1]))?;
    println!(
        "Cargo input freshness: {} changed paths ({} files refreshed, {} paths removed)",
        changed.paths, changed.refreshed_files, changed.removed_paths
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
        time::{Duration, UNIX_EPOCH},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "veoveo-source-freshness-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::create_dir(path.join("source")).unwrap();
            Self(path)
        }
        fn source(&self) -> PathBuf {
            self.0.join("source")
        }
        fn state(&self) -> PathBuf {
            self.0.join("state")
        }
        fn sync(&self) -> usize {
            synchronize(&self.source(), &self.state()).unwrap().paths
        }
        fn write(&self, relative: &str, bytes: &str) {
            let path = self.source().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            stamp(&path, UNIX_EPOCH + Duration::from_secs(1)).unwrap();
        }
        fn project(&self, source: &str) {
            self.write("Cargo.toml", "[package]\nname='freshness-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n");
            self.write("src/main.rs", source);
        }
        fn build(&self) -> std::process::Output {
            self.sync();
            Command::new("cargo")
                .current_dir(self.source())
                .args(["build", "--offline", "--verbose", "--target-dir"])
                .arg(self.0.join("target"))
                .output()
                .unwrap()
        }
        fn run(&self) -> String {
            let run = Command::new(self.0.join("target/debug/freshness-fixture"))
                .output()
                .unwrap();
            assert!(run.status.success());
            String::from_utf8(run.stdout).unwrap().trim().to_owned()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn old_checkout_and_same_length_revert_rebuild_the_correct_binary() {
        let fixture = Fixture::new();
        fixture.write(
            "Cargo.toml",
            "[package]\nname='freshness-fixture'\nversion='0.1.0'\nedition='2024'\n[workspace]\n",
        );
        for value in ["first", "other", "first"] {
            fixture.write(
                "src/main.rs",
                &format!("fn main() {{ println!(\"{value}\"); }}\n"),
            );
            assert!(fixture.sync() > 0);
            let build = Command::new("cargo")
                .current_dir(fixture.source())
                .args(["build", "--offline", "--quiet", "--target-dir"])
                .arg(fixture.0.join("target"))
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            let run = Command::new(fixture.0.join("target/debug/freshness-fixture"))
                .output()
                .unwrap();
            assert!(run.status.success());
            assert_eq!(String::from_utf8(run.stdout).unwrap().trim(), value);
        }
        // Cargo creates a lock file; admit it before testing the unchanged path.
        fixture.sync();
        let before = fs::metadata(fixture.source().join("src/main.rs"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(fixture.sync(), 0);
        assert_eq!(
            before,
            fs::metadata(fixture.source().join("src/main.rs"))
                .unwrap()
                .modified()
                .unwrap()
        );
    }

    #[test]
    fn deleted_inputs_and_changed_links_refresh_directory_watchers() {
        let fixture = Fixture::new();
        fixture.write("src/one", "one");
        fixture.write("src/two", "two");
        symlink("one", fixture.source().join("src/link")).unwrap();
        fixture.sync();
        let before = fs::metadata(fixture.source().join("src"))
            .unwrap()
            .modified()
            .unwrap();
        fs::remove_file(fixture.source().join("src/link")).unwrap();
        symlink("two", fixture.source().join("src/link")).unwrap();
        fixture.sync();
        assert_eq!(
            fs::read_link(fixture.state().join("src/link")).unwrap(),
            PathBuf::from("two")
        );
        assert!(
            fs::metadata(fixture.source().join("src"))
                .unwrap()
                .modified()
                .unwrap()
                > before
        );
        fs::remove_file(fixture.source().join("src/one")).unwrap();
        fixture.sync();
        assert!(!fixture.state().join("src/one").exists());
        assert_eq!(fixture.sync(), 0);
    }

    #[test]
    fn source_and_cache_state_must_not_overlap() {
        let fixture = Fixture::new();
        assert!(synchronize(&fixture.source(), &fixture.source().join("nested")).is_err());
    }

    #[test]
    fn removing_unselected_sources_preserves_an_unchanged_cargo_binary() {
        let fixture = Fixture::new();
        fixture.project("fn main() { println!(\"kept\"); }");
        fixture.write("unused/src/private.rs", "not selected\n");
        let first = fixture.build();
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        // Include Cargo's generated lockfile in the mirror before the comparison.
        fixture.sync();
        let original = fs::metadata(fixture.source().join("src/main.rs"))
            .unwrap()
            .modified()
            .unwrap();
        fs::remove_dir_all(fixture.source().join("unused")).unwrap();
        let changes = synchronize(&fixture.source(), &fixture.state()).unwrap();
        assert_eq!(changes.refreshed_files, 0);
        assert_eq!(changes.removed_paths, 3);
        assert_eq!(
            fs::metadata(fixture.source().join("src/main.rs"))
                .unwrap()
                .modified()
                .unwrap(),
            original
        );
        let next = fixture.build();
        let log = String::from_utf8_lossy(&next.stderr);
        assert!(next.status.success(), "{log}");
        assert!(
            log.contains("Fresh freshness-fixture"),
            "unchanged Cargo unit was recompiled: {log}"
        );
        assert!(!log.contains("Compiling freshness-fixture"), "{log}");
        assert_eq!(fixture.run(), "kept");
    }

    #[test]
    fn removed_directory_and_root_inputs_rerun_cargo_build_scripts() {
        for (watch, input) in [("inputs", "inputs/flag"), (".", "flag")] {
            let fixture = Fixture::new();
            fixture.project("fn main() { println!(\"{}\", env!(\"OBSERVATION\")); }");
            fixture.write("build.rs", &format!(
                "fn main() {{ println!(\"cargo:rerun-if-changed={watch}\"); println!(\"cargo:rustc-env=OBSERVATION={{}}\", if std::path::Path::new(\"{input}\").exists() {{ \"present\" }} else {{ \"absent\" }}); }}"
            ));
            fixture.write(input, "selected input");
            let first = fixture.build();
            assert!(
                first.status.success(),
                "{}",
                String::from_utf8_lossy(&first.stderr)
            );
            assert_eq!(fixture.run(), "present");
            fixture.sync();
            fs::remove_file(fixture.source().join(input)).unwrap();
            let next = fixture.build();
            assert!(
                next.status.success(),
                "{}",
                String::from_utf8_lossy(&next.stderr)
            );
            assert_eq!(
                fixture.run(),
                "absent",
                "Cargo missed removal under {watch}"
            );
        }
    }

    #[test]
    fn removing_a_used_rust_module_cannot_reuse_its_old_binary() {
        let fixture = Fixture::new();
        fixture.project("mod required; fn main() { required::run(); }");
        fixture.write("src/required.rs", "pub fn run() {}\n");
        let first = fixture.build();
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        fixture.sync();
        fs::remove_file(fixture.source().join("src/required.rs")).unwrap();
        let next = fixture.build();
        assert!(
            !next.status.success(),
            "Cargo reused a binary with a missing module"
        );
        assert!(
            String::from_utf8_lossy(&next.stderr).contains("file not found for module `required`")
        );
    }

    #[test]
    fn changed_and_removed_symlinks_do_not_reuse_an_old_embedded_input() {
        let fixture = Fixture::new();
        fixture.project("fn main() { println!(\"{}\", include_str!(\"link\")); }");
        fixture.write("src/first", "first");
        fixture.write("src/other", "other");
        symlink("first", fixture.source().join("src/link")).unwrap();
        let first = fixture.build();
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        assert_eq!(fixture.run(), "first");
        fixture.sync();
        fs::remove_file(fixture.source().join("src/link")).unwrap();
        symlink("other", fixture.source().join("src/link")).unwrap();
        let next = fixture.build();
        assert!(
            next.status.success(),
            "{}",
            String::from_utf8_lossy(&next.stderr)
        );
        assert_eq!(fixture.run(), "other");
        fs::remove_file(fixture.source().join("src/link")).unwrap();
        let removed = fixture.build();
        assert!(
            !removed.status.success(),
            "Cargo reused a removed symlink's content"
        );
    }
}
