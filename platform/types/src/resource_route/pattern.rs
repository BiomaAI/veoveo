use super::{
    ResourceComponentEncoding, ResourceEncodedPattern, ResourcePatternContext,
    ResourcePatternSpelling, ResourceRoute, ResourceRouteError, RoutePath,
};
use crate::{ResourceUriBuilder, UriSegment};

// These structural grammars admit more than a domain codec. Percent-encoded
// delimiters occupy one component; no raw domain-ID regex is pasted here.
const SEGMENT: &str = r"(?:[^\x00-\x20/%?#{}]|%[0-9A-Fa-f]{2})+";
const QUERY: &str = r"(?:[^\x00-\x20%&#{}]|%[0-9A-Fa-f]{2})*";

pub(super) fn wire_pattern(
    route: &ResourceRoute,
    spelling: ResourcePatternSpelling,
    fragment: impl Fn(&str, ResourcePatternContext) -> Option<ResourceEncodedPattern>,
) -> Result<String, ResourceRouteError> {
    let canonical = spelling == ResourcePatternSpelling::Canonical;
    let component = |variable: &str, encoding, default: &str, default_empty: bool| {
        fragment(variable, ResourcePatternContext { encoding, spelling })
            .map(|value| (format!("(?:{})", value.pattern), value.allows_empty))
            .unwrap_or_else(|| (default.to_owned(), default_empty))
    };
    route.validate()?;
    let mut result = format!("^{}", escape(route.root));
    for part in route.path {
        if !matches!(part, RoutePath::Tail(_)) {
            result.push('/');
        }
        match part {
            RoutePath::Literal(value) => {
                let uri = ResourceUriBuilder::new(route.root)?
                    .segment(UriSegment::new(*value)?)
                    .build()?;
                let parts =
                    url::Url::parse(uri.as_str()).map_err(|_| ResourceRouteError::Declaration)?;
                let encoded = parts.path().trim_start_matches('/');
                result.push_str(&if canonical {
                    escape(encoded)
                } else {
                    encoded_aliases(encoded)
                });
            }
            RoutePath::Scalar(variable) => result.push_str(
                &component(
                    variable,
                    ResourceComponentEncoding::PathSegment,
                    SEGMENT,
                    false,
                )
                .0,
            ),
            RoutePath::Tail(variable) => {
                let (tail, allows_empty) = component(
                    variable,
                    ResourceComponentEncoding::PathTail,
                    &format!("{SEGMENT}(?:/{SEGMENT})*"),
                    true,
                );
                result.push_str(&if allows_empty {
                    format!("(?:/{tail})?")
                } else {
                    format!("/{tail}")
                });
            }
        }
    }
    if route.trailing_slash {
        result.push('/');
    }
    let mut names = Vec::new();
    for query in route.query {
        let uri = ResourceUriBuilder::new(route.root)?
            .query_pair(query.name, "")?
            .build()?;
        let url = url::Url::parse(uri.as_str()).map_err(|_| ResourceRouteError::Declaration)?;
        let name = url
            .query()
            .ok_or(ResourceRouteError::Declaration)?
            .strip_suffix('=')
            .ok_or(ResourceRouteError::Declaration)?;
        let name = if canonical {
            escape(name)
        } else {
            encoded_aliases(name)
        };
        let (value, allows_empty) = component(
            query.variable,
            ResourceComponentEncoding::QueryValue,
            QUERY,
            true,
        );
        names.push(if canonical || !allows_empty {
            format!("{name}={value}")
        } else {
            format!("{name}(?:={value})?")
        });
    }
    let mut alternatives = Vec::new();
    if canonical || route.canonical_query_order {
        // Each alternative fixes the first present query; later queries are optional.
        // Output grows quadratically rather than enumerating every subset.
        for (index, first) in names.iter().enumerate() {
            let mut query = format!(r"\?{first}");
            for name in &names[index + 1..] {
                query.push_str(&format!("(?:&{name})?"));
            }
            alternatives.push(query);
        }
    } else if !names.is_empty() {
        // Duplicate names are rejected by the component parser. The schema stays
        // permissive for reordered pairs, bare names and encoded-name aliases.
        let name = format!("(?:{})", names.join("|"));
        alternatives.push(format!(r"\?{name}(?:&{name})*"));
    }
    if route.allow_empty_query && !canonical {
        alternatives.push(r"\?".into());
    }
    if !alternatives.is_empty() {
        result.push_str(&format!("(?:{})?", alternatives.join("|")));
    }
    result.push_str(r"(?![\s\S])");
    Ok(result)
}

// The URL builder has already encoded delimiters and UTF-8. Give each encoded
// octet its hexadecimal-case aliases; unescaped ASCII may also be percent encoded.
fn encoded_aliases(encoded: &str) -> String {
    let bytes = encoded.as_bytes();
    let mut result = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            result.push('%');
            for byte in &bytes[index + 1..index + 3] {
                let character = char::from(*byte);
                if character.is_ascii_alphabetic() {
                    result.push_str(&format!(
                        "[{}{}]",
                        character.to_ascii_uppercase(),
                        character.to_ascii_lowercase()
                    ));
                } else {
                    result.push(character);
                }
            }
            index += 3;
        } else {
            let byte = bytes[index];
            let hex = format!("{byte:02X}");
            let mut octet = String::from("%");
            for character in hex.chars() {
                if character.is_ascii_alphabetic() {
                    octet.push_str(&format!(
                        "[{}{}]",
                        character,
                        character.to_ascii_lowercase()
                    ));
                } else {
                    octet.push(character);
                }
            }
            result.push_str(&format!(
                "(?:{}|{octet})",
                escape(&char::from(byte).to_string())
            ));
            index += 1;
        }
    }
    result
}
fn escape(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\'
        ) {
            output.push('\\');
        }
        output.push(character);
    }
    output
}
