//! Download presentation for one authorized Artifact occurrence.
use axum::http::HeaderValue;
use std::fmt::Write;

pub(super) fn attachment(filename: Option<&str>) -> HeaderValue {
    let Some(filename) = filename else {
        return HeaderValue::from_static("attachment");
    };
    // RFC 6266 filename* uses RFC 8187 UTF-8 extended-value encoding. The
    // occurrence owns this name; a content-addressed blob may have other names.
    let mut value = String::from("attachment; filename*=UTF-8''");
    for byte in filename.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
        {
            value.push(char::from(byte));
        } else {
            write!(&mut value, "%{byte:02X}").expect("String write");
        }
    }
    HeaderValue::from_str(&value).expect("extended filename contains only visible ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_is_utf8_and_cannot_change_header_structure() {
        assert_eq!(attachment(None), "attachment");
        assert_eq!(
            attachment(Some("café \"rapport\" 100%.txt")),
            "attachment; filename*=UTF-8''caf%C3%A9%20%22rapport%22%20100%25.txt"
        );
        assert_eq!(
            attachment(Some("x';\r\nX-Evil: yes")),
            "attachment; filename*=UTF-8''x%27%3B%0D%0AX-Evil%3A%20yes"
        );
    }
}
