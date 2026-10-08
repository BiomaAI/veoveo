use std::{error::Error, fmt};

use rmcp::model::PaginatedRequestParams;

const CURSOR_PREFIX: &str = "v1:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaginationError {
    EmptyPage,
    InvalidCursor(String),
}

impl fmt::Display for PaginationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPage => f.write_str("page size must be greater than zero"),
            Self::InvalidCursor(cursor) => write!(f, "invalid pagination cursor {cursor:?}"),
        }
    }
}

impl Error for PaginationError {}

pub fn paginate<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
    page_size: usize,
) -> Result<Page<T>, PaginationError> {
    if page_size == 0 {
        return Err(PaginationError::EmptyPage);
    }

    let start = request
        .and_then(|request| request.cursor.as_deref())
        .map(decode_cursor)
        .transpose()?
        .unwrap_or_default();
    let total = items.len();
    let next_offset = start.saturating_add(page_size);
    let next_cursor = (next_offset < total).then(|| encode_cursor(next_offset));
    let page_items = items.into_iter().skip(start).take(page_size).collect();

    Ok(Page {
        items: page_items,
        next_cursor,
    })
}

struct OffsetCursorCodec;
impl veoveo_types::CursorCodec for OffsetCursorCodec {
    type Position = usize;
    type Error = PaginationError;
    fn check(&self, _position: &usize) -> Result<(), Self::Error> {
        Ok(())
    }
    fn encode(&self, position: &usize) -> Result<String, Self::Error> {
        Ok(format!("{CURSOR_PREFIX}{position}"))
    }
    fn decode(&self, wire: &str) -> Result<usize, Self::Error> {
        wire.strip_prefix(CURSOR_PREFIX)
            .and_then(|offset| offset.parse().ok())
            .ok_or_else(|| PaginationError::InvalidCursor(wire.to_owned()))
    }
}
fn encode_cursor(offset: usize) -> String {
    veoveo_types::OpaqueCursor::try_new(OffsetCursorCodec, offset)
        .expect("integer offset serialization")
        .into_wire()
}
fn decode_cursor(wire: &str) -> Result<usize, PaginationError> {
    veoveo_types::OpaqueCursor::parse(OffsetCursorCodec, wire).map(|cursor| *cursor.position())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_cursor_preserves_admitted_numeric_aliases() {
        assert_eq!(decode_cursor("v1:0002").unwrap(), 2);
        assert_eq!(encode_cursor(2), "v1:2");
        assert!(decode_cursor("v2:2").is_err());
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn offset_cursor_usize64_matches_python_admission_corpus() {
        for (wire, expected) in [
            ("v1:0", 0),
            ("v1:+0", 0),
            ("v1:0002", 2),
            ("v1:+0002", 2),
            ("v1:18446744073709551615", usize::MAX),
        ] {
            assert_eq!(decode_cursor(wire).unwrap(), expected);
        }
        assert_eq!(
            decode_cursor(&format!("v1:{}2", "0".repeat(5000))).unwrap(),
            2
        );
        for wire in [
            "v1:",
            "v1:+",
            "v1:++2",
            "v1:-2",
            "v1: 2",
            "v1:2 ",
            "v1:٢",
            "v1:²",
            "v1:2.0",
            "v2:2",
            "v1:18446744073709551616",
        ] {
            assert!(decode_cursor(wire).is_err(), "{wire:?}");
        }
        assert!(decode_cursor(&format!("v1:{}", "9".repeat(5000))).is_err());
    }

    #[test]
    fn paginate_returns_next_cursor() {
        let page = paginate(vec![1, 2, 3], None, 2).unwrap();
        assert_eq!(page.items, vec![1, 2]);
        assert_eq!(page.next_cursor.as_deref(), Some("v1:2"));
    }

    #[test]
    fn paginate_uses_cursor() {
        let request = PaginatedRequestParams::default().with_cursor(Some("v1:2".into()));
        let page = paginate(vec![1, 2, 3], Some(&request), 2).unwrap();
        assert_eq!(page.items, vec![3]);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn paginate_rejects_unknown_cursor_shape() {
        let request = PaginatedRequestParams::default().with_cursor(Some("2".into()));
        assert_eq!(
            paginate(vec![1, 2, 3], Some(&request), 2).unwrap_err(),
            PaginationError::InvalidCursor("2".into())
        );
    }
}
