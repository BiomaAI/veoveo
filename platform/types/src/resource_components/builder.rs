use std::{borrow::Cow, collections::BTreeSet};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use super::{ResourceUri, ResourceUriError, ResourceUriParts, Url};

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
    pub fn new(base: &str) -> Result<Self, ResourceUriError> {
        let base = ResourceUriParts::parse(base)?;
        if base.has_query() {
            return Err(ResourceUriError::BaseHasQuery);
        }
        Ok(Self {
            url: base.url,
            query_names: BTreeSet::new(),
        })
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
