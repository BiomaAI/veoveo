use super::{ResourceUriError, ResourceUriParts, Url};

/// An unescaped resource authority without credentials or a port. Domain builders
/// convert their specific ID to this component only when serializing an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UriAuthority(String);

impl UriAuthority {
    pub fn new(value: impl Into<String>) -> Result<Self, ResourceUriError> {
        let value = value.into();
        let mut url = Url::parse("veoveo://authority").expect("declared resource base");
        url.set_host(Some(&value))
            .map_err(|_| ResourceUriError::InvalidAuthority)?;
        if url.host_str() != Some(value.as_str()) {
            return Err(ResourceUriError::InvalidAuthority);
        }
        ResourceUriParts::parse(url.as_str())?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
