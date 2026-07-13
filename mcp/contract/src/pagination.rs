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

fn encode_cursor(offset: usize) -> String {
    format!("{CURSOR_PREFIX}{offset}")
}

fn decode_cursor(cursor: &str) -> Result<usize, PaginationError> {
    let Some(offset) = cursor.strip_prefix(CURSOR_PREFIX) else {
        return Err(PaginationError::InvalidCursor(cursor.to_string()));
    };
    offset
        .parse::<usize>()
        .map_err(|_| PaginationError::InvalidCursor(cursor.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
