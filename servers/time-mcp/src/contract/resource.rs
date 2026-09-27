use std::{error::Error, fmt};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceAddress, ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts,
    UriSegment,
};

use super::AuthorityReleaseId;

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct TimeAuthorityReleaseUri {
    wire: String,
    release_id: AuthorityReleaseId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeAuthorityReleaseUriError {
    Uri(ResourceUriError),
    InvalidReleaseAddress,
}

impl fmt::Display for TimeAuthorityReleaseUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(error) => error.fmt(f),
            Self::InvalidReleaseAddress => {
                f.write_str("expected canonical time authority release URI")
            }
        }
    }
}

impl Error for TimeAuthorityReleaseUriError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Uri(error) => Some(error),
            Self::InvalidReleaseAddress => None,
        }
    }
}

impl From<ResourceUriError> for TimeAuthorityReleaseUriError {
    fn from(error: ResourceUriError) -> Self {
        Self::Uri(error)
    }
}

impl TimeAuthorityReleaseUri {
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::{CalendarId, TimeAuthorityReleaseUri};
    /// TimeAuthorityReleaseUri::new(&CalendarId::new("calendar-example").unwrap());
    /// ```
    pub fn new(release_id: &AuthorityReleaseId) -> Self {
        let uri = ResourceUriBuilder::new("time://authorities/releases")
            .expect("declared Time release route")
            .segment(UriSegment::new(release_id.as_str()).expect("validated authority release ID"))
            .build()
            .expect("validated Time release components");
        Self {
            wire: uri.into(),
            release_id: release_id.clone(),
        }
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, TimeAuthorityReleaseUriError> {
        let value = value.into();
        let parts = ResourceUriParts::parse(&value)?;
        let segments: Vec<_> = parts.path_segments().collect();
        if parts.scheme() != "time" || parts.authority() != "authorities" || parts.has_query() {
            return Err(TimeAuthorityReleaseUriError::InvalidReleaseAddress);
        }
        let [family, id] = segments.as_slice() else {
            return Err(TimeAuthorityReleaseUriError::InvalidReleaseAddress);
        };
        if family != "releases" {
            return Err(TimeAuthorityReleaseUriError::InvalidReleaseAddress);
        }
        let release_id = AuthorityReleaseId::new(id.as_ref())
            .map_err(|_| TimeAuthorityReleaseUriError::InvalidReleaseAddress)?;
        let address = Self::new(&release_id);
        // The published identity has one spelling. Encoded aliases were never
        // valid IDs and must not create alternative policy or cache keys.
        if address.as_str() != value {
            return Err(TimeAuthorityReleaseUriError::InvalidReleaseAddress);
        }
        Ok(address)
    }

    pub fn release_id(&self) -> AuthorityReleaseId {
        self.release_id.clone()
    }

    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

impl ResourceAddress for TimeAuthorityReleaseUri {
    type Error = TimeAuthorityReleaseUriError;

    fn parse(uri: &ResourceUri) -> Result<Self, Self::Error> {
        Self::parse(uri.as_str())
    }

    fn to_uri(&self) -> Result<ResourceUri, Self::Error> {
        Ok(ResourceUri::new(self.wire.clone()).expect("validated Time release URI"))
    }
}

impl fmt::Display for TimeAuthorityReleaseUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl TryFrom<String> for TimeAuthorityReleaseUri {
    type Error = TimeAuthorityReleaseUriError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<TimeAuthorityReleaseUri> for String {
    fn from(value: TimeAuthorityReleaseUri) -> Self {
        value.wire
    }
}
