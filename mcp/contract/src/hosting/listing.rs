//! What a domain's list hook returns.

use rmcp::{
    ErrorData,
    model::{CacheScope, PaginatedRequestParams},
};

use super::ReadCache;
use crate::{PRIVATE_CATALOG_TTL_MS, paginate};

/// Items per discovery page the host serves.
pub const CATALOG_PAGE_SIZE: usize = 100;

/// Who pages a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Paging {
    /// The host sorts the complete list and pages it with its own cursor.
    Host,
    /// The domain returned one page in its own order, with its own next cursor.
    Domain { next_cursor: Option<String> },
}

/// A domain's discovery list, its paging and its cache policy.
#[derive(Debug, Clone)]
pub struct Listing<T> {
    entries: Vec<T>,
    paging: Paging,
    cache: ReadCache,
}

impl<T> Listing<T> {
    /// The complete list; the host sorts and pages it.
    pub fn all(entries: Vec<T>) -> Self {
        Self {
            entries,
            paging: Paging::Host,
            cache: ReadCache::Private,
        }
    }

    /// One page the domain selected with the request cursor, such as a page of
    /// a remote catalog, and the cursor of the next page.
    pub fn page(entries: Vec<T>, next_cursor: Option<String>) -> Self {
        Self {
            entries,
            paging: Paging::Domain { next_cursor },
            cache: ReadCache::Private,
        }
    }

    /// Marks the list as never reused, for lists whose visibility follows
    /// current authority.
    pub fn no_store(mut self) -> Self {
        self.cache = ReadCache::NoStore;
        self
    }

    /// The page to serve, its next cursor, and its cache lifetime and scope.
    pub(super) fn serve<K: Ord>(
        self,
        request: Option<&PaginatedRequestParams>,
        key: impl Fn(&T) -> K,
    ) -> Result<ServedPage<T>, ErrorData> {
        let (items, next_cursor) = match self.paging {
            Paging::Host => {
                let mut entries = self.entries;
                entries.sort_by_key(|entry| key(entry));
                let page = paginate(entries, request, CATALOG_PAGE_SIZE)
                    .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
                (page.items, page.next_cursor)
            }
            Paging::Domain { next_cursor } => (self.entries, next_cursor),
        };
        let ttl_ms = match self.cache {
            ReadCache::Private => PRIVATE_CATALOG_TTL_MS,
            ReadCache::NoStore => 0,
        };
        Ok(ServedPage {
            items,
            next_cursor,
            ttl_ms,
            cache_scope: CacheScope::Private,
        })
    }
}

pub(super) struct ServedPage<T> {
    pub(super) items: Vec<T>,
    pub(super) next_cursor: Option<String>,
    pub(super) ttl_ms: u64,
    pub(super) cache_scope: CacheScope,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_paging_sorts_and_domain_paging_passes_through() {
        let served = Listing::all(vec![3, 1, 2]).serve(None, |n| *n).unwrap();
        assert_eq!(served.items, [1, 2, 3]);
        assert_eq!(served.ttl_ms, PRIVATE_CATALOG_TTL_MS);
        let served = Listing::page(vec![3, 1], Some("after-1".to_owned()))
            .no_store()
            .serve(None, |n| *n)
            .unwrap();
        assert_eq!(served.items, [3, 1]);
        assert_eq!(served.next_cursor.as_deref(), Some("after-1"));
        assert_eq!(served.ttl_ms, 0);
    }
}
