use std::{borrow::Cow, collections::BTreeSet};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use super::UriAuthority;
use super::{ResourceUri, ResourceUriError, ResourceUriParts, Url};
use crate::ResourceScheme;

// URL's path-segment setter permits these non-URL code points in custom-scheme
// paths. Our concrete profile rejects syntax violations, so encode them as well.
// The URL library has already handled delimiters, percent signs, and Unicode.
const NON_URL_PATH_CHARACTERS: &AsciiSet =
    &CONTROLS.add(b'[').add(b'\\').add(b']').add(b'^').add(b'|');

/// A decoded path component ready for library encoding. This is a serialization
/// helper, not an ID: domain constructors must require their specific ID types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriSegment(String);

impl UriSegment {
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceUriError> {
        let value = value.into();
        if value.is_empty()
            || matches!(value.as_str(), "." | "..")
            || value.chars().any(char::is_control)
        {
            return Err(ResourceUriError::InvalidPathSegment);
        }
        Ok(Self(value))
    }
}

/// Component encoding for domain-owned builders. The base is a declared route;
/// domain methods accept typed IDs and typed query arguments before reaching here.
/// ```compile_fail
/// use veoveo_types::ResourceUriBuilder;
/// ResourceUriBuilder::new("example://items").unwrap().segment("unchecked/path");
/// ```
#[derive(Debug)]
pub struct ResourceUriBuilder {
    url: Url,
    query_names: BTreeSet<String>,
}

impl ResourceUriBuilder {
    /// Build a root from typed components. The URL library owns scheme and host
    /// handling; the resulting authority must preserve its supplied spelling.
    /// ```compile_fail
    /// use veoveo_types::{ResourceScheme, ResourceUriBuilder};
    /// ResourceUriBuilder::from_components(&ResourceScheme::new("example").unwrap(), "item");
    /// ```
    pub fn from_components(
        scheme: &ResourceScheme,
        authority: UriAuthority,
    ) -> Result<Self, ResourceUriError> {
        // WHATWG forbids switching an existing URL between special and ordinary
        // schemes. Choose a declared base for its standard category before using
        // setters. This is the URL standard's scheme list, not a domain registry.
        let base = match scheme.as_str() {
            "http" => "http://authority/",
            "https" => "https://authority/",
            "ftp" => "ftp://authority/",
            "ws" => "ws://authority/",
            "wss" => "wss://authority/",
            "file" => "file://authority/",
            _ => "veoveo://authority",
        };
        let mut url = Url::parse(base).expect("declared resource base");
        if url.scheme() != scheme.as_str() {
            url.set_scheme(scheme.as_str())
                .map_err(|_| ResourceUriError::InvalidUri)?;
        }
        url.set_host(Some(authority.as_str()))
            .map_err(|_| ResourceUriError::InvalidAuthority)?;
        if url.host_str() != Some(authority.as_str()) {
            return Err(ResourceUriError::NonCanonical);
        }
        Self::new(url.as_str())
    }

    pub fn new(base: &str) -> Result<Self, ResourceUriError> {
        let base = ResourceUriParts::parse(base)?;
        if base.has_query() {
            return Err(ResourceUriError::BaseHasQuery);
        }
        Ok(Self::from_parts(base))
    }

    /// Extend an already checked address while preserving existing query pairs.
    /// Duplicate names remain invalid when adding a new pair.
    pub fn from_parts(parts: ResourceUriParts) -> Self {
        Self {
            query_names: parts.query.keys().cloned().collect(),
            url: parts.url,
        }
    }

    pub fn segment(mut self, segment: UriSegment) -> Self {
        self.url
            .path_segments_mut()
            .expect("validated hierarchical URI")
            .pop_if_empty()
            .push(&segment.0);
        self
    }

    /// The owner supplies only its declared query names. Values arrive decoded;
    /// the URL library escapes delimiters, spaces, percent signs, and Unicode.
    pub fn query_pair(mut self, name: &str, value: &str) -> Result<Self, ResourceUriError> {
        if name.is_empty()
            || name.chars().chain(value.chars()).any(char::is_control)
            || !self.query_names.insert(name.to_owned())
        {
            return Err(ResourceUriError::InvalidQuery);
        }
        self.url.query_pairs_mut().append_pair(name, value);
        Ok(self)
    }

    pub fn build(mut self) -> Result<ResourceUri, ResourceUriError> {
        let encoded: Cow<'_, str> =
            utf8_percent_encode(self.url.path(), NON_URL_PATH_CHARACTERS).into();
        if let Cow::Owned(path) = encoded {
            self.url.set_path(&path);
        }
        ResourceUriParts::parse(self.url.as_str()).map(ResourceUriParts::into_uri)
    }
}
