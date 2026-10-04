//! Network URL syntax. Admission supplies no host, DNS or access authority.
use std::{cell::Cell, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;

/// Canonical HTTPS URL without credentials or a fragment. Query spelling,
/// order and repeated names are preserved, including signed URL parameters.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HttpsUrl(Url);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("expected a canonical absolute HTTPS URL without credentials or a fragment")]
pub struct HttpsUrlError;

impl HttpsUrl {
    pub fn parse(value: &str) -> Result<Self, HttpsUrlError> {
        let violation = Cell::new(false);
        let url = Url::options()
            .syntax_violation_callback(Some(&|_| violation.set(true)))
            .parse(value)
            .map_err(|_| HttpsUrlError)?;
        if violation.get()
            || url.as_str() != value
            || !value.is_ascii()
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
            || url.scheme() != "https"
            || !url.has_host()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(HttpsUrlError);
        }
        Ok(Self(url))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
    pub fn as_url(&self) -> &Url {
        &self.0
    }
}

impl fmt::Debug for HttpsUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // A syntactically valid URL can carry bearer authority in its query.
        f.debug_tuple("HttpsUrl").field(&"[redacted]").finish()
    }
}
impl fmt::Display for HttpsUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for HttpsUrl {
    type Err = HttpsUrlError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
impl TryFrom<String> for HttpsUrl {
    type Error = HttpsUrlError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}
impl From<HttpsUrl> for String {
    fn from(value: HttpsUrl) -> Self {
        value.0.into()
    }
}
impl JsonSchema for HttpsUrl {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "HttpsUrl".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type":"string", "format":"uri",
            "pattern":"^https://[^/?#@\\s]+/[^#\\s]*$",
            "description":"Canonical ASCII HTTPS URL without credentials or a fragment. The URL parser also checks host syntax, escaping and normalization; runtime policy checks access."
        })
    }
}
