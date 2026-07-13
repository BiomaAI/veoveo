pub const CATALOG_URI: &str = "recording://catalog";
pub const RECORDING_TEMPLATE: &str = "recording://recordings/{recording_id}";
pub const SEGMENTS_TEMPLATE: &str = "recording://recordings/{recording_id}/segments";

pub fn recording_uri(recording_id: &str) -> String {
    format!("recording://recordings/{recording_id}")
}

pub fn segments_uri(recording_id: &str) -> String {
    format!("recording://recordings/{recording_id}/segments")
}

pub fn parse_recording_uri(uri: &str) -> Option<&str> {
    let value = uri.strip_prefix("recording://recordings/")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

pub fn parse_segments_uri(uri: &str) -> Option<&str> {
    let value = uri
        .strip_prefix("recording://recordings/")?
        .strip_suffix("/segments")?;
    (!value.is_empty() && !value.contains('/')).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_shapes_do_not_overlap() {
        assert_eq!(
            parse_recording_uri("recording://recordings/abc"),
            Some("abc")
        );
        assert_eq!(
            parse_recording_uri("recording://recordings/abc/segments"),
            None
        );
        assert_eq!(
            parse_segments_uri("recording://recordings/abc/segments"),
            Some("abc")
        );
    }
}
