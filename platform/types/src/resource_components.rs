//! Domain resource address parsing. Obtaining these components establishes the
//! stricter profile for server-owned routes, separate from generic network URIs.
use std::{borrow::Cow, cell::Cell, collections::BTreeMap, error::Error, fmt};

use percent_encoding::percent_decode_str;
use url::Url;

use crate::ResourceUri;

mod authority;
mod builder;
pub use authority::UriAuthority;
pub use builder::{ResourceUriBuilder, UriSegment};

/// Validation failures contain no URI, query value, credential, or other input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceUriError {
    InvalidUri,
    NonCanonical,
    NotHierarchical,
    InvalidAuthority,
    DisallowedComponent,
    InvalidEncoding,
    InvalidPathSegment,
    InvalidQuery,
    BaseHasQuery,
}

impl fmt::Display for ResourceUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidUri => "expected an absolute resource URI",
            Self::NonCanonical => "resource URI must be escaped and must not require normalization",
            Self::NotHierarchical => "resource URI requires an authority and hierarchical path",
            Self::InvalidAuthority => {
                "resource authority must be unescaped and exclude credentials and ports"
            }
            Self::DisallowedComponent => {
                "resource URI cannot contain fragments or unexpanded templates"
            }
            Self::InvalidEncoding => {
                "resource components must encode UTF-8 without control characters"
            }
            Self::InvalidPathSegment => {
                "path segment must be nonempty, nonrelative, and free of control characters"
            }
            Self::InvalidQuery => {
                "query parameters must have unique nonempty names and valid values"
            }
            Self::BaseHasQuery => "resource builder base must not contain a query",
        })
    }
}

impl Error for ResourceUriError {}

/// Parsed concrete components, with the supplied spelling preserved. This is a
/// validation result, not a domain resource: owners still validate routes and IDs.
/// URL parsing and percent decoding are delegated to the pinned URI libraries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceUriParts {
    url: Url,
    query: BTreeMap<String, String>,
}

impl ResourceUriParts {
    pub fn parse(value: &str) -> Result<Self, ResourceUriError> {
        let violation = Cell::new(false);
        let url = Url::options()
            .syntax_violation_callback(Some(&|_| violation.set(true)))
            .parse(value)
            .map_err(|_| ResourceUriError::InvalidUri)?;
        if violation.get() || url.as_str() != value {
            return Err(ResourceUriError::NonCanonical);
        }
        if !url.has_host() || url.cannot_be_a_base() {
            return Err(ResourceUriError::NotHierarchical);
        }
        let authority = url.host_str().expect("validated resource authority");
        // Resource authorities are declared names or unescaped IDs. WHATWG's
        // opaque-host parser does not report malformed percent escapes here.
        if url.authority() != authority || authority.contains('%') {
            return Err(ResourceUriError::InvalidAuthority);
        }
        // WHATWG allows braces in some non-special URL components. They are
        // template syntax at our protocol boundary, never concrete route text.
        if url.fragment().is_some() || value.contains(['{', '}']) {
            return Err(ResourceUriError::DisallowedComponent);
        }
        for component in [authority, url.path(), url.query().unwrap_or_default()] {
            let decoded = percent_decode_str(component)
                .decode_utf8()
                .map_err(|_| ResourceUriError::InvalidEncoding)?;
            if decoded.chars().any(char::is_control) {
                return Err(ResourceUriError::InvalidEncoding);
            }
        }
        let mut query = BTreeMap::new();
        for (name, value) in url.query_pairs() {
            if name.is_empty()
                || query
                    .insert(name.into_owned(), value.into_owned())
                    .is_some()
            {
                return Err(ResourceUriError::InvalidQuery);
            }
        }
        Ok(Self { url, query })
    }

    pub fn as_str(&self) -> &str {
        self.url.as_str()
    }

    pub fn scheme(&self) -> &str {
        self.url.scheme()
    }

    pub fn authority(&self) -> &str {
        self.url.host_str().expect("validated resource authority")
    }

    /// An escaped slash stays inside its decoded segment. Owners decide whether
    /// their domain ID permits it; it cannot create another path component.
    pub fn path_segments(&self) -> impl Iterator<Item = Cow<'_, str>> {
        self.url
            .path_segments()
            .into_iter()
            .flatten()
            .map(|segment| {
                percent_decode_str(segment)
                    .decode_utf8()
                    .expect("validated UTF-8 path")
            })
    }

    pub fn has_query(&self) -> bool {
        self.url.query().is_some()
    }

    /// Query decoding uses form semantics: `+` means space, `%2B` means plus.
    /// Names are decoded before duplicate detection. Owners reject unknown names.
    pub fn query_parameters(&self) -> &BTreeMap<String, String> {
        &self.query
    }

    pub fn into_uri(self) -> ResourceUri {
        ResourceUri::new(String::from(self.url)).expect("validated absolute resource address")
    }
}
