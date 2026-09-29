//! Filesystem selection inside an already-authorized owner directory.
use crate::contract::{
    DUCKDB_DATABASE_PAGE_SIZE, DuckDbDatabaseCursor, DuckDbDatabaseId, DuckDbDatabasePage,
};
use anyhow::{Context, Result};
use std::{collections::BTreeSet, path::Path};

/// The hosted adapter derives `directory` from verified identity; it is never caller input.
/// Directory order is unspecified, so retain only the smallest 101 eligible names.
pub fn database_page(
    directory: &Path,
    cursor: Option<&DuckDbDatabaseCursor>,
) -> Result<DuckDbDatabasePage> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(DuckDbDatabasePage::from_ids(Vec::new(), false)?);
        }
        Err(error) => return Err(error).context("reading owner database directory"),
    };
    let mut candidates = BTreeSet::new();
    for entry in entries {
        let entry = entry.context("reading owner database entry")?;
        if !entry
            .file_type()
            .context("reading database entry type")?
            .is_file()
        {
            continue;
        }
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|s| s.strip_suffix(".duckdb")) else {
            continue;
        };
        let Ok(id) = DuckDbDatabaseId::new(stem) else {
            continue;
        };
        if cursor.is_some_and(|cursor| &id <= cursor.after()) {
            continue;
        }
        candidates.insert(id);
        if candidates.len() > DUCKDB_DATABASE_PAGE_SIZE + 1 {
            candidates.pop_last();
        }
    }
    let has_more = candidates.len() > DUCKDB_DATABASE_PAGE_SIZE;
    if has_more {
        candidates.pop_last();
    }
    Ok(DuckDbDatabasePage::from_ids(
        candidates.into_iter().collect(),
        has_more,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_select_only_regular_databases_and_traverse_owner_names() {
        let root = tempfile::tempdir().unwrap();
        let owner = root.path().join("owner");
        std::fs::create_dir(&owner).unwrap();
        // Deliberately invalid database contents prove listing never opens the engine.
        for n in (0..251).rev() {
            std::fs::write(owner.join(format!("db_{n:03}.duckdb")), b"metadata only").unwrap();
        }
        for name in ["private.duckdb.wal", "invalid-name.duckdb", "UPPER.duckdb"] {
            std::fs::write(owner.join(name), []).unwrap();
        }
        std::fs::create_dir(owner.join("directory.duckdb")).unwrap();
        std::fs::write(root.path().join("foreign.duckdb"), []).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            root.path().join("foreign.duckdb"),
            owner.join("link.duckdb"),
        )
        .unwrap();
        let mut cursor = None;
        let mut found = Vec::new();
        for expected_len in [100, 100, 51] {
            let page = database_page(&owner, cursor.as_ref()).unwrap();
            assert_eq!(page.items().len(), expected_len);
            found.extend(page.items().iter().map(|e| e.id().to_string()));
            cursor = page.next_cursor().cloned();
        }
        assert!(cursor.is_none());
        assert_eq!(
            found,
            (0..251).map(|n| format!("db_{n:03}")).collect::<Vec<_>>()
        );
        assert!(
            database_page(&root.path().join("new-owner"), None)
                .unwrap()
                .items()
                .is_empty()
        );
    }
    #[test]
    fn each_page_reopens_the_current_directory_without_a_retained_inventory() {
        let root = tempfile::tempdir().unwrap();
        for n in 0..101 {
            std::fs::write(root.path().join(format!("db_{n:03}.duckdb")), []).unwrap();
        }
        let page = database_page(root.path(), None).unwrap();
        std::fs::remove_file(root.path().join("db_100.duckdb")).unwrap();
        std::fs::write(root.path().join("db_101.duckdb"), []).unwrap();
        let next = database_page(root.path(), page.next_cursor()).unwrap();
        assert_eq!(next.items()[0].id().as_str(), "db_101");
        assert!(next.next_cursor().is_none());
    }
}
