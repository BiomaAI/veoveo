//! Canonical recording analysis resource identity.
pub fn parse_recording_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("recording://recordings/")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}
