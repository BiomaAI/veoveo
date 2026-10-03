use veoveo_modular_fixture_mcp::contract::{
    ObservatoryDocument, ObservatoryResource, ObservatoryScope, ReadingId,
};
use veoveo_types::{ResourceAddress, ResourceUri, ScopeDefinition, ScopeName};

#[test]
fn independent_owned_addresses_and_scope_names_round_trip() {
    for resource in [
        ObservatoryResource::Docs,
        ObservatoryResource::Document(ObservatoryDocument::Design),
        ObservatoryResource::Contract,
        ObservatoryResource::Readings,
        ObservatoryResource::Reading(ReadingId::new("sensor-a").unwrap()),
    ] {
        assert_eq!(
            ObservatoryResource::parse(&resource.to_uri().unwrap()).unwrap(),
            resource
        );
    }
    assert_eq!(ObservatoryScope::Read.name().as_str(), "observatory:read");
    assert!(ObservatoryScope::try_from(&ScopeName::new("unrelated:read").unwrap()).is_err());
}

#[test]
fn owned_parser_rejects_wrong_routes_ids_and_parameters() {
    for value in [
        "other://readings",
        "observatory://reading",
        "observatory://reading/a/b",
        "observatory://reading/%61",
        "observatory://reading/A",
        "observatory://readings?cursor=bad",
        "observatory://reading/sensor-a#bad",
        "observatory://reading/{reading_id}",
    ] {
        // The URI profile rejects some spellings before the owner parser sees them.
        assert!(
            ResourceUri::new(value).map_or(true, |uri| ObservatoryResource::parse(&uri).is_err()),
            "{value}"
        );
    }
}
