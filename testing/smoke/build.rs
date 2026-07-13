use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-env-changed=DUCKDB_DOWNLOAD_LIB");
    if let Err(error) = stage_duckdb_runtime() {
        println!("cargo:warning=failed to stage DuckDB runtime for smoke binary: {error}");
    }
}

fn stage_duckdb_runtime() -> io::Result<()> {
    let out_dir = PathBuf::from(
        env::var_os("OUT_DIR")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is unavailable"))?,
    );
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "target profile directory"))?;
    let target_dir = profile_dir
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "target directory"))?;
    let download_dir = target_dir.join("duckdb-download");
    let Some(library) = find_library(&download_dir)? else {
        return Ok(());
    };
    let deps = profile_dir.join("deps");
    fs::create_dir_all(&deps)?;
    let filename = library
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "DuckDB library filename"))?;
    fs::copy(&library, deps.join(filename))?;
    Ok(())
}

fn find_library(root: &Path) -> io::Result<Option<PathBuf>> {
    if !root.exists() {
        return Ok(None);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("libduckdb.dylib" | "libduckdb.so" | "duckdb.dll")
            ) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}
