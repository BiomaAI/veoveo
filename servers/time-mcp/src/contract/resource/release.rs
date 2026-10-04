use std::fmt;

use super::{AuthorityReleaseId, TimeResource, TimeResourceError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri};

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct TimeAuthorityReleaseUri {
    wire: String,
    release_id: AuthorityReleaseId,
    bootstrap: bool,
}

impl TimeAuthorityReleaseUri {
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::{CalendarId, TimeAuthorityReleaseUri};
    /// TimeAuthorityReleaseUri::new(&CalendarId::parse("calendar-example").unwrap());
    /// ```
    pub fn new(release_id: &AuthorityReleaseId) -> Self {
        let uri = TimeResource::AuthorityRelease(release_id.clone())
            .to_uri()
            .expect("validated Time release address");
        Self {
            wire: uri.into(),
            release_id: release_id.clone(),
            bootstrap: false,
        }
    }

    pub fn bootstrap(release_id: &AuthorityReleaseId) -> Self {
        Self {
            wire: TimeResource::BootstrapAuthority(release_id.clone()).to_string(),
            release_id: release_id.clone(),
            bootstrap: true,
        }
    }

    pub fn is_bootstrap(&self) -> bool {
        self.bootstrap
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, TimeResourceError> {
        let value = value.into();
        let (id, bootstrap) = match TimeResource::parse(&value)? {
            TimeResource::AuthorityRelease(id) => (id, false),
            TimeResource::BootstrapAuthority(id) => (id, true),
            _ => return Err(TimeResourceError::UnknownResource),
        };
        Ok(Self {
            wire: value,
            release_id: id,
            bootstrap,
        })
    }

    pub fn release_id(&self) -> &AuthorityReleaseId {
        &self.release_id
    }

    pub fn as_str(&self) -> &str {
        &self.wire
    }
}

impl ResourceAddress for TimeAuthorityReleaseUri {
    type Error = TimeResourceError;

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
    type Error = TimeResourceError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<TimeAuthorityReleaseUri> for String {
    fn from(value: TimeAuthorityReleaseUri) -> Self {
        value.wire
    }
}
