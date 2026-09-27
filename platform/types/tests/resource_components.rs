use veoveo_types::{
    ResourceUri, ResourceUriBuilder, ResourceUriError, ResourceUriParts, UriSegment,
};

#[test]
fn authority_only_roots_have_no_path_segments() {
    for wire in ["example://items", "example://items?cursor=one"] {
        let parts = ResourceUriParts::parse(wire).unwrap();
        assert_eq!(parts.path_segments().count(), 0);
    }
    assert_eq!(
        ResourceUriParts::parse("example://items/")
            .unwrap()
            .path_segments()
            .collect::<Vec<_>>(),
        [""]
    );
}

#[test]
fn component_encoding_does_not_turn_data_into_paths_or_query_arguments() {
    let id = "north/south?key=x&other=y#fragment 50%+café";
    let cursor = "abc+def/==&cursor=injected?☕%2F";
    let uri = ResourceUriBuilder::new("example://items")
        .unwrap()
        .segment(UriSegment::new(id).unwrap())
        .query_pair("cursor", cursor)
        .unwrap()
        .build()
        .unwrap();
    let parts = uri.components().unwrap();
    assert_eq!(parts.scheme(), "example");
    assert_eq!(parts.authority(), "items");
    assert_eq!(parts.path_segments().collect::<Vec<_>>(), [id]);
    assert_eq!(parts.query_parameters().len(), 1);
    assert_eq!(parts.query_parameters()["cursor"], cursor);
    assert_eq!(parts.into_uri(), uri);
    assert!(
        uri.as_str()
            .contains("north%2Fsouth%3Fkey=x&other=y%23fragment%2050%25+caf%C3%A9")
    );
}

#[test]
fn query_names_are_decoded_before_duplicate_detection() {
    for invalid in [
        "example://items?cursor=a&cursor=b",
        "example://items?cursor=a&%63ursor=b",
        "example://items?some+name=a&some%20name=b",
        "example://items?=value",
    ] {
        assert_eq!(
            ResourceUriParts::parse(invalid).unwrap_err(),
            ResourceUriError::InvalidQuery
        );
    }
    let builder = ResourceUriBuilder::new("example://items")
        .unwrap()
        .query_pair("cursor", "one")
        .unwrap();
    assert_eq!(
        builder.query_pair("cursor", "two").unwrap_err(),
        ResourceUriError::InvalidQuery
    );
    assert!(ResourceUriBuilder::new("example://items?").is_err());
    assert!(ResourceUriBuilder::new("example://items?cursor=one").is_err());
    let uri = ResourceUriBuilder::new("example://items")
        .unwrap()
        .query_pair("cursor", "a + b")
        .unwrap()
        .build()
        .unwrap();
    assert_eq!(uri.as_str(), "example://items?cursor=a+%2B+b");
    assert_eq!(
        uri.components().unwrap().query_parameters()["cursor"],
        "a + b"
    );
}

#[test]
fn concrete_parser_rejects_templates_normalization_and_unsafe_components() {
    for invalid in [
        "relative/path",
        "example:opaque",
        "example:///items",
        "EXAMPLE://items",
        "example://items/../other",
        "example://items/%2e%2e/other",
        "example://items/a/./b",
        "example://items/path%",
        "example://items/path%2",
        "example://items/path%GG",
        "example://items/path%ff",
        "example://items?cursor=%ff",
        "example://items?%ff=value",
        "example://items/path%00",
        "example://items?cursor=%0a",
        "example://items/a b",
        "example://items/é",
        "example://items?cursor=é",
        " example://items",
        "example://items\n",
        "example://user:password@items/path",
        "example://user@items/path",
        "example://@items/path",
        "example://items%GG/path",
        "example://items%ff/path",
        "example://items:443/path",
        "example://items/path#part",
        "example://items/{item_id}",
        "example://items{?cursor}",
        "example://items?q={value}",
    ] {
        assert!(
            ResourceUriParts::parse(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn parser_preserves_spelling_and_segment_identity() {
    for wire in [
        "example://items",
        "ui://example/catalog.html",
        "artifact://0197f78e-f2f0-7a6e-8a5d-f41c691e4471",
        "example://items/a%2Fb",
        "example://items/a%2fb",
        "example://items/a%252Fb",
        "example://items/caf%C3%A9?cursor=one%2Btwo",
        "example://items?cursor=",
    ] {
        assert_eq!(
            ResourceUriParts::parse(wire).unwrap().into_uri().as_str(),
            wire
        );
    }
    let parts = ResourceUriParts::parse("example://items/a%2Fb/a%252Fb").unwrap();
    assert_eq!(parts.path_segments().collect::<Vec<_>>(), ["a/b", "a%2Fb"]);
    for invalid in ["", ".", "..", "a\0b", "a\nb"] {
        assert!(UriSegment::new(invalid).is_err());
    }
    let uri = ResourceUriBuilder::new("example://items/")
        .unwrap()
        .segment(UriSegment::new("%2e%2e").unwrap())
        .build()
        .unwrap();
    assert_eq!(uri.as_str(), "example://items/%252e%252e");
}

#[test]
fn diagnostics_do_not_echo_untrusted_input() {
    for value in [
        "example://user:URI_SECRET_CANARY@items",
        "example://items?cursor=URI_SECRET_CANARY%ff",
        "example://items/URI_SECRET_CANARY/../other",
    ] {
        let error = ResourceUriParts::parse(value).unwrap_err();
        assert!(!format!("{error} {error:?}").contains("URI_SECRET_CANARY"));
    }
}

#[test]
fn printable_component_values_round_trip_through_the_builder() {
    for value in (0x20u8..=0x7e)
        .map(|byte| format!("id{}value", char::from(byte)))
        .chain(["東京/🌍+é".to_owned()])
    {
        let uri = ResourceUriBuilder::new("example://items")
            .unwrap()
            .segment(UriSegment::new(&value).unwrap())
            .query_pair("cursor", &value)
            .unwrap()
            .build()
            .unwrap_or_else(|error| panic!("failed for {value:?}: {error}"));
        let parts = uri.components().unwrap();
        assert_eq!(parts.path_segments().collect::<Vec<_>>(), [value.as_str()]);
        assert_eq!(parts.query_parameters()["cursor"], value);
    }
}

#[test]
fn historical_reference_decoding_does_not_claim_concrete_address_validation() {
    for wire in [
        "example://items/{id}",
        "example://items{?cursor}",
        "example://items/bad%",
    ] {
        let reference: ResourceUri = serde_json::from_value(wire.into()).unwrap();
        assert_eq!(reference.as_str(), wire);
        assert!(reference.components().is_err());
        assert_eq!(serde_json::to_value(reference).unwrap(), wire);
    }
}
