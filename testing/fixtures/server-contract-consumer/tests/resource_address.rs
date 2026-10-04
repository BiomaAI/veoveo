// An independent owner's encoded component grammar uses the public codec API.
use std::borrow::Cow;
use veoveo_types::{
    ResourceAddress, ResourceComponentEncoding, ResourceEncodedPattern, ResourceFieldCodec,
    ResourcePatternContext, ResourcePatternSpelling, ResourceRouteError, ResourceTailCodec,
    ResourceUri,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Part(String);
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid independent fixture address")]
struct Invalid;


fn route_error(_: ResourceRouteError) -> Invalid {
    Invalid
}

struct PartCodec;
impl ResourceFieldCodec<Part> for PartCodec {
    type Error = Invalid;
    fn parse(value: &str) -> Result<Part, Invalid> {
        match value {
            "one" | "a/b%c +é" => Ok(Part(value.into())),
            _ => Err(Invalid),
        }
    }
    fn text(value: &Part) -> Cow<'_, str> {
        value.0.as_str().into()
    }
    fn encoded_pattern(context: ResourcePatternContext) -> Option<ResourceEncodedPattern> {
        use ResourceComponentEncoding::{PathSegment, QueryValue};
        use ResourcePatternSpelling::{Admitted, Canonical};
        let pattern = match (context.encoding, context.spelling) {
            (PathSegment, Canonical) => r"(?:one|a%2Fb%25c%20\+%C3%A9)",
            (PathSegment, Admitted) => {
                r"(?:(?:o|%6[fF])(?:n|%6[eE])(?:e|%65)|(?:a|%61)%2[fF](?:b|%62)%25(?:c|%63)%20(?:\+|%2[bB])%[cC]3%[aA]9)"
            }
            (QueryValue, Canonical) => r"(?:one|a%2Fb%25c\+%2B%C3%A9)",
            (QueryValue, Admitted) => {
                r"(?:(?:o|%6[fF])(?:n|%6[eE])(?:e|%65)|(?:a|%61)(?:/|%2[fF])(?:b|%62)%25(?:c|%63)(?:\+|%20)%2[bB]%[cC]3%[aA]9)"
            }
            _ => return None,
        };
        Some(ResourceEncodedPattern {
            pattern: pattern.into(),
            allows_empty: false,
        })
    }
}

#[derive(Debug, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(template = "fixture://items/by-name/{id}{?cursor}", error = Invalid,
    route_error = route_error, canonical = false)]
struct Address {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(codec = PartCodec, error = |error| error)]
    id: Part,
    #[resource(codec = PartCodec, error = |error| error)]
    cursor: Option<Part>,
}

fn validator(pattern: String) -> jsonschema::Validator {
    jsonschema::validator_for(&serde_json::json!({"type": "string", "pattern": pattern})).unwrap()
}

#[test]
fn encoded_patterns_preserve_reserved_builders_and_distinguish_admitted_aliases() {
    let canonical = validator(
        Address::resource_wire_patterns(ResourcePatternSpelling::Canonical)
            .unwrap()
            .remove(0),
    );
    let admitted = validator(
        Address::resource_wire_patterns(ResourcePatternSpelling::Admitted)
            .unwrap()
            .remove(0),
    );
    for name in ["one", "a/b%c +é"] {
        let address = Address::resource_from_parts(
            PartCodec::parse(name).unwrap(),
            Some(PartCodec::parse(name).unwrap()),
        )
        .unwrap();
        let wire = address.to_uri().unwrap();
        assert!(
            canonical.is_valid(&serde_json::json!(wire.as_str())),
            "{wire}"
        );
        assert!(
            admitted.is_valid(&serde_json::json!(wire.as_str())),
            "{wire}"
        );
        assert_eq!(Address::parse(&wire).unwrap(), address);
    }
    for alias in [
        "fixture://items/by-name/%6Fne?cursor=%6fne",
        "fixture://items/by-name/o%6Ee?cursor=%6F%6E%65",
        "fixture://items/by-name/%61%2F%62%25%63%20+%c3%a9?cursor=%61/%62%25%63+%2b%c3%a9",
        "fixture://items/%62y-name/one?cursor=one",
        "fixture://items/by-name/a%2fb%25c%20%2b%c3%a9?cursor=a%2fb%25c%20%2b%c3%a9",
    ] {
        let uri = ResourceUri::new(alias).unwrap();
        assert_eq!(Address::parse(&uri).unwrap().to_uri().unwrap(), uri);
        assert!(admitted.is_valid(&serde_json::json!(alias)), "{alias}");
        assert!(!canonical.is_valid(&serde_json::json!(alias)), "{alias}");
    }
    for invalid in [
        "fixture://other/by-name/one",
        "fixture://items/by-name/not-admitted",
        "fixture://items/by-name/one/extra",
        "fixture://items/by-name/one?cursor",
        "fixture://items/by-name/one?cursor=",
        "fixture://items/by-name/one\n",
        "fixture://items/by-name/one\r",
        "fixture://items/by-name/one\r\n",
    ] {
        assert!(
            !admitted.is_valid(&serde_json::json!(invalid)),
            "{invalid:?}"
        );
        if let Ok(uri) = ResourceUri::new(invalid) {
            assert!(Address::parse(&uri).is_err());
        }
    }
}

struct EmptyCodec;
impl ResourceFieldCodec<Part> for EmptyCodec {
    type Error = Invalid;
    fn parse(value: &str) -> Result<Part, Invalid> {
        match value {
            "" | "one" => Ok(Part(value.into())),
            _ => Err(Invalid),
        }
    }
    fn text(value: &Part) -> Cow<'_, str> {
        value.0.as_str().into()
    }
    fn encoded_pattern(context: ResourcePatternContext) -> Option<ResourceEncodedPattern> {
        (context.encoding == ResourceComponentEncoding::QueryValue).then(|| {
            ResourceEncodedPattern {
                pattern: if context.spelling == ResourcePatternSpelling::Canonical {
                    "(?:one)?".into()
                } else {
                    r"(?:(?:o|%6[fF])(?:n|%6[eE])(?:e|%65))?".into()
                },
                allows_empty: true,
            }
        })
    }
}
#[derive(veoveo_types::ResourceAddress)]
#[resource(template = "fixture://empty{?cursor}", error = Invalid, route_error = route_error, canonical = false)]
struct EmptyQuery {
    #[resource(codec = EmptyCodec, error = |error| error)]
    cursor: Option<Part>,
}
#[test]
fn owner_empty_query_policy_preserves_bare_name_admission() {
    let canonical = validator(
        EmptyQuery::resource_wire_patterns(ResourcePatternSpelling::Canonical)
            .unwrap()
            .remove(0),
    );
    assert!(canonical.is_valid(&serde_json::json!("fixture://empty?cursor=")));
    assert!(!canonical.is_valid(&serde_json::json!("fixture://empty?cursor")));
    assert!(!canonical.is_valid(&serde_json::json!("fixture://empty?cursor=%6Fne")));
    let schema = validator(
        EmptyQuery::resource_wire_patterns(ResourcePatternSpelling::Admitted)
            .unwrap()
            .remove(0),
    );
    for wire in [
        "fixture://empty?cursor",
        "fixture://empty?cursor=",
        "fixture://empty?cursor=one",
        "fixture://empty?cursor=%6F%6E%65",
    ] {
        assert!(schema.is_valid(&serde_json::json!(wire)));
        assert!(EmptyQuery::parse(&ResourceUri::new(wire).unwrap()).is_ok());
    }
}

struct Tail(Vec<Part>);
struct TailCodec;
impl ResourceTailCodec<Tail> for TailCodec {
    type Error = Invalid;
    fn parse(segments: &[Cow<'_, str>]) -> Result<Tail, Invalid> {
        if segments.is_empty() || segments.iter().any(|part| part != "one") {
            return Err(Invalid);
        }
        Ok(Tail(
            segments.iter().map(|part| Part(part.to_string())).collect(),
        ))
    }
    fn segments(value: &Tail) -> Vec<Cow<'_, str>> {
        value.0.iter().map(|part| part.0.as_str().into()).collect()
    }
    fn encoded_pattern(context: ResourcePatternContext) -> Option<ResourceEncodedPattern> {
        (context.encoding == ResourceComponentEncoding::PathTail).then(|| ResourceEncodedPattern {
            pattern: if context.spelling == ResourcePatternSpelling::Canonical {
                "one(?:/one)*".into()
            } else {
                "(?:o|%6[fF])(?:n|%6[eE])(?:e|%65)(?:/(?:o|%6[fF])(?:n|%6[eE])(?:e|%65))*".into()
            },
            allows_empty: false,
        })
    }
}
#[derive(veoveo_types::ResourceAddress)]
#[resource(template = "fixture://tail/{+path}", error = Invalid, route_error = route_error, canonical = false)]
struct TailAddress {
    #[resource(variable = "path", tail, codec = TailCodec, error = |error| error)]
    tail: Tail,
}
#[test]
fn owner_nonempty_tail_fragment_requires_a_component() {
    let schema = validator(
        TailAddress::resource_wire_patterns(ResourcePatternSpelling::Admitted)
            .unwrap()
            .remove(0),
    );
    let value =
        TailAddress::resource_from_parts(Tail(vec![Part("one".into()), Part("one".into())]))
            .unwrap();
    assert!(schema.is_valid(&serde_json::json!(value.to_uri().unwrap().as_str())));
    for alias in ["fixture://tail/o%6Ee/%6F%6E%65", "fixture://tail/%6fne"] {
        assert!(schema.is_valid(&serde_json::json!(alias)));
        assert!(TailAddress::parse(&ResourceUri::new(alias).unwrap()).is_ok());
    }
    for invalid in [
        "fixture://tail",
        "fixture://tail/",
        "fixture://tail/one\n",
        "fixture://tail/one\r",
    ] {
        assert!(!schema.is_valid(&serde_json::json!(invalid)), "{invalid:?}");
        if let Ok(uri) = ResourceUri::new(invalid) {
            assert!(TailAddress::parse(&uri).is_err());
        }
    }
}
