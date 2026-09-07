//! Startup snapshots of image-owned MCP App HTML.

use std::{
    fs,
    io::{self, Read},
    path::Path,
    sync::Arc,
};

/// The Console's admitted size bound for a self-contained App document.
pub const MAX_APP_HTML_BYTES: u64 = 2 * 1024 * 1024;

/// Immutable HTML bytes loaded before a server accepts requests.
///
/// The caller owns the configured path and still authorizes each resource read.
/// Loading an asset confers no MCP authority and adds no HTTP file-serving route.
#[derive(Clone, Debug)]
pub struct AppHtml(Arc<str>);

impl AppHtml {
    pub fn load(path: &Path) -> io::Result<Self> {
        let load = || {
            if !fs::metadata(path)?.is_file() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "App HTML must be a regular file",
                ));
            }
            Self::read(fs::File::open(path)?)
        };
        load().map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("loading App HTML {}: {error}", path.display()),
            )
        })
    }

    fn read(reader: impl Read) -> io::Result<Self> {
        let mut html = String::new();
        reader
            .take(MAX_APP_HTML_BYTES + 1)
            .read_to_string(&mut html)?;
        if html.trim().is_empty() || html.len() as u64 > MAX_APP_HTML_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "App HTML must be nonempty UTF-8 within 2 MiB",
            ));
        }
        Ok(Self(Arc::from(html)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_exact_bytes_in_an_immutable_snapshot() {
        let mut input = b"<!doctype html>\n<title>Map</title>".to_vec();
        let app = AppHtml::read(input.as_slice()).unwrap();
        let clone = app.clone();
        input.fill(b'x');
        assert_eq!(clone.as_str(), "<!doctype html>\n<title>Map</title>");
        assert!(Arc::ptr_eq(&app.0, &clone.0));
    }

    #[test]
    fn rejects_empty_non_utf8_and_oversized_assets() {
        for bytes in [
            vec![],
            vec![b' '],
            vec![0xff],
            vec![b'x'; MAX_APP_HTML_BYTES as usize + 1],
        ] {
            assert_eq!(
                AppHtml::read(bytes.as_slice()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        assert!(AppHtml::read(vec![b'x'; MAX_APP_HTML_BYTES as usize].as_slice()).is_ok());
        assert!(AppHtml::load(Path::new(env!("CARGO_MANIFEST_DIR"))).is_err());
    }
}
