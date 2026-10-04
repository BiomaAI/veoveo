//! Independent route owners exercise the shared derive through public interfaces.
use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use veoveo_types::{
    Identity, ResourceAddress, ResourceFieldCodec, ResourceRouteError, ResourceTailCodec,
    ResourceUri,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Component(String);
impl Identity for Component {
    type Error = Error;
    fn parse_identity(value: &str) -> Result<Self, Error> {
        if value.is_empty() || value.starts_with("bad") || value.chars().any(char::is_control) {
            return Err(Error::Field);
        }
        Ok(Self(value.into()))
    }
    fn identity_text(&self) -> Cow<'_, str> {
        self.0.as_str().into()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Error {
    Route,
    Field,
    Tail,
    Relationship,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid fixture resource")
    }
}
impl std::error::Error for Error {}
fn route_error(_: ResourceRouteError) -> Error {
    Error::Route
}
fn relationship(value: &Feature) -> Result<(), Error> {
    if value.release_id == value.feature_id {
        Err(Error::Relationship)
    } else {
        Ok(())
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    veoveo_types::ResourceAddress,
)]
#[serde(try_from = "String", into = "String")]
#[schemars(with = "String")]
#[resource(template="map://source-feature/{release_id}/{source_feature_id}", error=Error, route_error=route_error, validate=relationship, wire)]
struct Feature {
    #[resource(error=|_| Error::Field)]
    #[resource(accessor=release_id)]
    release_id: Component,
    #[resource(variable="source_feature_id", error=|_| Error::Field)]
    #[resource(accessor=feature_id)]
    feature_id: Component,
    #[resource(cache)]
    wire: ResourceUri,
}

#[test]
fn cached_map_shape_preserves_typed_mapping_wire_and_owner_validation() {
    let uri = ResourceUri::new("map://source-feature/release/a%2Fb%25c").unwrap();
    let value = Feature::parse(&uri).unwrap();
    assert_eq!(value.feature_id.0, "a/b%c");
    assert_eq!(value.feature_id().0, "a/b%c");
    let built = Feature::resource_build_uri(&value.release_id, &value.feature_id).unwrap();
    assert_eq!(built, uri);
    assert_eq!(
        Feature::resource_from_parts(value.release_id.clone(), value.feature_id.clone()).unwrap(),
        value
    );
    assert_eq!(
        Feature::resource_from_parts(Component("same".into()), Component("same".into())),
        Err(Error::Relationship)
    );
    assert_eq!(
        Feature::resource_from_parts(Component("release".into()), Component("bad".into())),
        Err(Error::Field)
    );
    assert_eq!(value.to_uri().unwrap(), uri);
    assert_eq!(serde_json::to_value(&value).unwrap(), uri.as_str());
    assert_eq!(
        serde_json::from_value::<Feature>(serde_json::json!(uri.as_str())).unwrap(),
        value
    );
    assert_eq!(
        Feature::RESOURCE_ROUTES[0].discovery_template().unwrap(),
        "map://source-feature/{release_id}/{source_feature_id}"
    );
    let schema = serde_json::to_value(schemars::schema_for!(Feature)).unwrap();
    assert_eq!(schema["type"], "string");
    assert_eq!(schema["title"], "string");
    assert_eq!(
        Feature::parse(&ResourceUri::new("map://source-feature/same/same").unwrap()),
        Err(Error::Relationship)
    );
    assert_eq!(
        Feature::parse(&ResourceUri::new("map://source-feature/release/bad-id").unwrap()),
        Err(Error::Field)
    );
    assert!(
        Feature::parse(&ResourceUri::new("map://source-feature/%72elease/feature").unwrap())
            .is_err()
    );
}

struct CursorCodec;
impl ResourceFieldCodec<Component> for CursorCodec {
    type Error = Error;
    fn parse(value: &str) -> Result<Component, Error> {
        Component::parse_identity(value)
    }
    fn text(value: &Component) -> Cow<'_, str> {
        value.identity_text()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(template="example://pairs/{id}{?cursor,limit}", error=Error, route_error=route_error, canonical=false, allow_empty_query)]
struct AliasAddress {
    id: Component,
    #[resource(codec=CursorCodec)]
    cursor: Option<Component>,
    limit: Option<Component>,
    #[resource(cache)]
    wire: String,
}

#[test]
fn noncanonical_owner_preserves_cached_aliases_and_can_build_canonical_components() {
    let uri = ResourceUri::new("example://pairs/%69d?limit=2&%63ursor=page").unwrap();
    let address = AliasAddress::parse(&uri).unwrap();
    assert_eq!(address.to_uri().unwrap(), uri);
    assert_eq!(
        address.resource_components_uri().unwrap().as_str(),
        "example://pairs/id?cursor=page&limit=2"
    );
    let empty = ResourceUri::new("example://pairs/id?").unwrap();
    assert_eq!(
        AliasAddress::parse(&empty).unwrap().to_uri().unwrap(),
        empty
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Zone(Vec<String>);
struct ZoneCodec;
impl ResourceTailCodec<Zone> for ZoneCodec {
    type Error = Error;
    fn parse(segments: &[Cow<'_, str>]) -> Result<Zone, Error> {
        if segments.is_empty()
            || segments
                .iter()
                .any(|segment| segment.is_empty() || segment.contains('/'))
        {
            return Err(Error::Tail);
        }
        Ok(Zone(
            segments.iter().map(|value| value.to_string()).collect(),
        ))
    }
    fn segments(value: &Zone) -> Vec<Cow<'_, str>> {
        value.0.iter().map(|part| part.as_str().into()).collect()
    }
}
#[derive(Debug, Clone, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(error=Error, route_error=route_error)]
enum TimeShape {
    #[resource(template = "time://docs")]
    Docs,
    #[resource(template = "time://zones/{+zone}")]
    Zone(#[resource(variable="zone", tail, codec=ZoneCodec, error=|_| Error::Tail)] Zone),
    #[resource(template = "time://events{?cursor}")]
    Events {
        #[resource(codec=CursorCodec, error=|_| Error::Field)]
        cursor: Option<Component>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(error=Error, route_error=route_error)]
enum Overlap {
    #[resource(template = "example://item/{id}")]
    Strict(#[resource(variable="id", error=|_| Error::Field)] Component),
    #[resource(template = "example://other/{+zone}")]
    Tail(#[resource(variable="zone", tail, codec=ZoneCodec)] Zone),
}

#[test]
fn time_tails_optional_queries_and_fixed_routes_keep_their_shapes() {
    let zone =
        TimeShape::parse(&ResourceUri::new("time://zones/America/El_Salvador").unwrap()).unwrap();
    assert_eq!(
        zone,
        TimeShape::Zone(Zone(vec!["America".into(), "El_Salvador".into()]))
    );
    assert_eq!(
        zone.to_uri().unwrap().as_str(),
        "time://zones/America/El_Salvador"
    );
    for uri in [
        "time://zones",
        "time://zones/",
        "time://zones/America%2FEl_Salvador",
    ] {
        assert_eq!(
            TimeShape::parse(&ResourceUri::new(uri).unwrap()),
            Err(Error::Tail)
        );
    }
    assert_eq!(
        TimeShape::parse(&ResourceUri::new("time://docs").unwrap()),
        Ok(TimeShape::Docs)
    );
    assert!(TimeShape::parse(&ResourceUri::new("time://docs/").unwrap()).is_err());
    assert_eq!(
        TimeShape::parse(&ResourceUri::new("time://events?cursor=bad").unwrap()),
        Err(Error::Field)
    );
    assert!(TimeShape::parse(&ResourceUri::new("time://events?unknown=page").unwrap()).is_err());
}

#[test]
fn invalid_matched_route_cannot_fall_through_to_another_codec() {
    assert_eq!(
        Overlap::parse(&ResourceUri::new("example://item/bad-id").unwrap()),
        Err(Error::Field)
    );
    assert!(Overlap::parse(&ResourceUri::new("example://item/ok").unwrap()).is_ok());
}

#[derive(Debug, Clone, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(template="example://raw/{type}", error=Error, route_error=route_error)]
struct RawField {
    r#type: Component,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Eq, veoveo_types::ResourceAddress)]
#[resource(error=Error, route_error=route_error)]
enum RawVariant {
    #[resource(template = "example://type")]
    r#type,
}

#[test]
fn raw_field_and_variant_names_preserve_wire_variables_and_constants() {
    let uri = ResourceUri::new("example://raw/value").unwrap();
    assert_eq!(RawField::parse(&uri).unwrap().r#type.0, "value");
    assert_eq!(RawVariant::RESOURCE_TEMPLATE_TYPE, "example://type");
    assert_eq!(
        RawVariant::r#type.to_uri().unwrap().as_str(),
        "example://type"
    );
}

fn reject_escaped_input(parts: &veoveo_types::ResourceUriParts) -> Result<(), Error> {
    if parts.as_str().contains('%') {
        Err(Error::Route)
    } else {
        Ok(())
    }
}

#[derive(Debug, veoveo_types::ResourceAddress)]
#[resource(template="example://checked/{id}", error=Error, route_error=route_error, input=reject_escaped_input)]
struct InputChecked {
    #[resource(error=|_| Error::Field)]
    id: Component,
}

#[test]
fn input_admission_runs_after_shape_and_before_field_codec() {
    for value in ["example://checked/b%61d", "example://wrong/bad"] {
        assert!(matches!(
            InputChecked::parse(&ResourceUri::new(value).unwrap()),
            Err(Error::Route)
        ));
    }
    assert!(matches!(
        InputChecked::parse(&ResourceUri::new("example://checked/bad").unwrap()),
        Err(Error::Field)
    ));
    assert!(matches!(
        InputChecked::resource_from_parts(Component("bad".into())),
        Err(Error::Field)
    ));
}

struct PreciseCodec;
impl ResourceFieldCodec<Component> for PreciseCodec {
    type Error = Error;
    fn parse(value: &str) -> Result<Component, Error> {
        let digits = value
            .strip_prefix("item-")
            .or_else(|| value.strip_prefix("q/"));
        if !digits.is_some_and(|digits| {
            digits.len() == 2 && digits.bytes().all(|byte| byte.is_ascii_digit())
        }) {
            return Err(Error::Field);
        }
        Component::parse_identity(value)
    }
    fn text(value: &Component) -> Cow<'_, str> {
        value.identity_text()
    }
    fn encoded_pattern(
        context: veoveo_types::ResourcePatternContext,
    ) -> Option<veoveo_types::ResourceEncodedPattern> {
        use veoveo_types::{
            ResourceComponentEncoding, ResourceEncodedPattern, ResourcePatternSpelling,
        };
        if context.spelling != ResourcePatternSpelling::Canonical {
            return None;
        }
        Some(ResourceEncodedPattern {
            pattern: match context.encoding {
                ResourceComponentEncoding::PathSegment => r"(?:item-|q%2F)[0-9]{2}",
                ResourceComponentEncoding::QueryValue => r"(?:item-|q%2F)[0-9]{2}",
                ResourceComponentEncoding::PathTail => return None,
            }
            .into(),
            allows_empty: false,
        })
    }
}

#[derive(Debug, veoveo_types::ResourceAddress)]
#[resource(template="example://precise/{id}{?cursor}", error=Error, route_error=route_error)]
struct PreciseRoute {
    #[resource(codec=PreciseCodec)]
    id: Component,
    #[resource(codec=PreciseCodec)]
    cursor: Option<Component>,
}

#[test]
fn encoded_constraints_are_opt_in_and_receive_component_context() {
    use veoveo_types::ResourcePatternSpelling;
    let patterns =
        PreciseRoute::resource_wire_patterns(ResourcePatternSpelling::Canonical).unwrap();
    assert!(patterns[0].contains("(?:item-|q%2F)[0-9]{2}"));
    assert!(patterns[0].contains(r"cursor=(?:(?:item-|q%2F)[0-9]{2})"));
    assert!(patterns[0].ends_with(r"(?![\s\S])"));
    let admitted = PreciseRoute::resource_wire_patterns(ResourcePatternSpelling::Admitted).unwrap();
    assert!(!admitted[0].contains("(?:item-|q%2F)[0-9]{2}"));
    let value = PreciseRoute::resource_from_parts(
        Component("item-12".into()),
        Some(Component("q/34".into())),
    )
    .unwrap();
    assert_eq!(
        value.to_uri().unwrap().as_str(),
        "example://precise/item-12?cursor=q%2F34"
    );
}
