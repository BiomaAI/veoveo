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
}

impl TimeAuthorityReleaseUri {
    /// ```compile_fail
    /// use veoveo_time_mcp::contract::{CalendarId, TimeAuthorityReleaseUri};
    /// TimeAuthorityReleaseUri::new(&CalendarId::new("calendar-example").unwrap());
    /// ```
    pub fn new(release_id: &AuthorityReleaseId) -> Self {
        let uri = TimeResource::AuthorityRelease(release_id.clone())
            .to_uri()
            .expect("validated Time release address");
        Self {
            wire: uri.into(),
            release_id: release_id.clone(),
        }
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, TimeResourceError> {
        let value = value.into();
        let TimeResource::AuthorityRelease(id) = TimeResource::parse(&value)? else {
            return Err(TimeResourceError::UnknownResource);
        };
        Ok(Self {
            wire: value,
            release_id: id,
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
