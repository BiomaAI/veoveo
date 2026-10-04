use super::*;

const FEATURE: ResourceRoute = ResourceRoute {
    template: "map://source-feature/{release_id}/{source_feature_id}",
    root: "map://source-feature",
    scheme: "map",
    authority: "source-feature",
    path: &[
        RoutePath::Scalar("release_id"),
        RoutePath::Scalar("source_feature_id"),
    ],
    query: &[],
    allow_empty_query: false,
    trailing_slash: false,
    canonical_query_order: true,
};
const PAGE: ResourceRoute = ResourceRoute {
    template: "map://sources{?cursor,limit}",
    root: "map://sources",
    scheme: "map",
    authority: "sources",
    path: &[],
    query: &[
        RouteQuery {
            name: "cursor",
            variable: "cursor",
        },
        RouteQuery {
            name: "limit",
            variable: "limit",
        },
    ],
    allow_empty_query: false,
    trailing_slash: false,
    canonical_query_order: true,
};
const ZONE: ResourceRoute = ResourceRoute {
    template: "time://zones/{+zone}",
    root: "time://zones",
    scheme: "time",
    authority: "zones",
    path: &[RoutePath::Tail("zone")],
    query: &[],
    allow_empty_query: false,
    trailing_slash: false,
    canonical_query_order: true,
};

#[test]
fn decoded_delimiters_do_not_change_component_shape() {
    let parts = ResourceUriParts::parse("map://source-feature/release/a%2Fb%25c").unwrap();
    assert!(FEATURE.matches_shape(&parts));
    let matched = FEATURE.capture(&parts).unwrap();
    assert_eq!(matched.scalar("source_feature_id"), Some("a/b%c"));
    let uri = FEATURE
        .build(
            &[
                RouteBinding::Scalar {
                    variable: "release_id",
                    value: "release".into(),
                },
                RouteBinding::Scalar {
                    variable: "source_feature_id",
                    value: "a/b%c".into(),
                },
            ],
            &[],
        )
        .unwrap();
    assert_eq!(uri.as_str(), parts.as_str());
    assert!(
        !FEATURE
            .matches_shape(&ResourceUriParts::parse("map://source-feature/release/a/b").unwrap())
    );
}

#[test]
fn matched_query_failures_are_separate_from_route_selection() {
    for uri in [
        "map://sources?",
        "map://sources?unknown=value",
        "map://sources?limit=1&cursor=c",
    ] {
        let parts = ResourceUriParts::parse(uri).unwrap();
        assert!(PAGE.matches_shape(&parts));
        assert!(matches!(
            PAGE.capture(&parts),
            Err(ResourceRouteError::Query)
        ));
    }
    for uri in [
        "map://sources?cursor=a&cursor=b",
        "map://sources?cursor=a&%63ursor=b",
    ] {
        assert!(ResourceUriParts::parse(uri).is_err());
    }
    let uri = PAGE
        .build(&[], &[("limit", "2".into()), ("cursor", "a+b &é".into())])
        .unwrap();
    assert_eq!(uri.as_str(), "map://sources?cursor=a%2Bb+%26%C3%A9&limit=2");
    let parts = uri.components().unwrap();
    let matched = PAGE.capture(&parts).unwrap();
    assert_eq!(matched.query("cursor"), Some("a+b &é"));
}

#[test]
fn aliases_empty_query_and_trailing_slash_have_explicit_policy() {
    let relaxed = ResourceRoute {
        allow_empty_query: true,
        canonical_query_order: false,
        ..PAGE
    };
    for uri in [
        "map://sources?",
        "map://sources?limit=1&%63ursor=c",
        "map://sources?cursor",
    ] {
        let parts = ResourceUriParts::parse(uri).unwrap();
        assert!(relaxed.capture(&parts).is_ok());
    }
    let slash = ResourceRoute {
        template: "map://sources/{?cursor,limit}",
        trailing_slash: true,
        ..PAGE
    };
    let uri = slash.build(&[], &[]).unwrap();
    assert_eq!(uri.as_str(), "map://sources/");
    assert!(slash.capture(&uri.components().unwrap()).is_ok());
    assert!(!PAGE.matches_shape(&uri.components().unwrap()));
}

#[test]
fn tail_admission_sees_components_even_when_the_domain_value_is_invalid() {
    for uri in [
        "time://zones",
        "time://zones/",
        "time://zones/America/El_Salvador",
        "time://zones/America%2FEl_Salvador",
    ] {
        let parts = ResourceUriParts::parse(uri).unwrap();
        assert!(ZONE.matches_shape(&parts));
        let matched = ZONE.capture(&parts).unwrap();
        let segments = matched.tail("zone").unwrap();
        if uri == "time://zones" {
            assert!(segments.is_empty());
        }
        if uri.contains("%2F") {
            assert_eq!(segments.len(), 1);
        }
    }
    let uri = ZONE
        .build(
            &[RouteBinding::Tail {
                variable: "zone",
                segments: vec!["America".into(), "El_Salvador".into()],
            }],
            &[],
        )
        .unwrap();
    assert_eq!(uri.as_str(), "time://zones/America/El_Salvador");
}

#[test]
fn discovery_must_agree_with_direct_descriptors() {
    let wrong = ResourceRoute::declare(
        "map://source/{id}",
        "map://source-feature",
        "map",
        "source-feature",
        &[
            RoutePath::Scalar("release_id"),
            RoutePath::Scalar("source_feature_id"),
        ],
        &[],
        ResourceRoutePolicy {
            allow_empty_query: false,
            trailing_slash: false,
            canonical_query_order: true,
        },
    );
    let parts = ResourceUriParts::parse("map://source-feature/release/feature").unwrap();
    assert!(matches!(
        wrong.capture(&parts),
        Err(ResourceRouteError::Declaration)
    ));
    assert!(matches!(
        wrong.discovery_template(),
        Err(ResourceRouteError::Declaration)
    ));
    assert!(matches!(
        wrong.wire_pattern(),
        Err(ResourceRouteError::Declaration)
    ));
    let wrong = ResourceRoute {
        query: &[RouteQuery {
            name: "after",
            variable: "cursor",
        }],
        ..PAGE
    };
    assert!(matches!(
        wrong.build(&[], &[]),
        Err(ResourceRouteError::Declaration)
    ));
}

#[test]
fn query_pattern_growth_is_polynomial_and_fields_use_encoded_grammars() {
    let pattern = FEATURE.wire_pattern().unwrap();
    assert!(pattern.contains("%[0-9A-Fa-f]{2}"));
    assert!(!pattern.contains("[0-9a-f]{8}"));
    let query = PAGE.wire_pattern().unwrap();
    assert!(query.starts_with("^map://sources"));
    assert!(query.ends_with(r"(?![\s\S])"));
    let relaxed = ResourceRoute {
        canonical_query_order: false,
        allow_empty_query: true,
        ..PAGE
    };
    assert!(relaxed.wire_pattern().unwrap().contains(r"\?"));
    assert!(ZONE.wire_pattern().unwrap().contains("(?:/"));
}

#[test]
fn ordered_query_patterns_support_many_fields_without_subset_enumeration() {
    static ALL: [RouteQuery; 16] = [
        RouteQuery {
            name: "q0",
            variable: "q0",
        },
        RouteQuery {
            name: "q1",
            variable: "q1",
        },
        RouteQuery {
            name: "q2",
            variable: "q2",
        },
        RouteQuery {
            name: "q3",
            variable: "q3",
        },
        RouteQuery {
            name: "q4",
            variable: "q4",
        },
        RouteQuery {
            name: "q5",
            variable: "q5",
        },
        RouteQuery {
            name: "q6",
            variable: "q6",
        },
        RouteQuery {
            name: "q7",
            variable: "q7",
        },
        RouteQuery {
            name: "q8",
            variable: "q8",
        },
        RouteQuery {
            name: "q9",
            variable: "q9",
        },
        RouteQuery {
            name: "q10",
            variable: "q10",
        },
        RouteQuery {
            name: "q11",
            variable: "q11",
        },
        RouteQuery {
            name: "q12",
            variable: "q12",
        },
        RouteQuery {
            name: "q13",
            variable: "q13",
        },
        RouteQuery {
            name: "q14",
            variable: "q14",
        },
        RouteQuery {
            name: "q15",
            variable: "q15",
        },
    ];
    let policy = ResourceRoutePolicy {
        allow_empty_query: false,
        trailing_slash: false,
        canonical_query_order: true,
    };
    let medium = ResourceRoute::declare(
        "example://pages{?q0,q1,q2,q3,q4,q5,q6,q7}",
        "example://pages",
        "example",
        "pages",
        &[],
        &ALL[..8],
        policy,
    );
    let large = ResourceRoute::declare(
        "example://pages{?q0,q1,q2,q3,q4,q5,q6,q7,q8,q9,q10,q11,q12,q13,q14,q15}",
        "example://pages",
        "example",
        "pages",
        &[],
        &ALL,
        policy,
    );
    let medium = medium.wire_pattern().unwrap();
    let large = large.wire_pattern().unwrap();
    assert!(large.len() > medium.len());
    assert!(large.len() < medium.len() * 5);
}

#[test]
fn binding_admission_precedes_component_encoding() {
    let scalar = |variable, value: &'static str| RouteBinding::Scalar {
        variable,
        value: value.into(),
    };
    for path in [
        vec![scalar("release_id", "")],
        vec![scalar("release_id", ""), scalar("unknown", "feature")],
        vec![scalar("release_id", ""), scalar("release_id", "again")],
        vec![
            RouteBinding::Tail {
                variable: "release_id",
                segments: vec![],
            },
            scalar("source_feature_id", "feature"),
        ],
    ] {
        assert!(matches!(
            FEATURE.build(&path, &[]),
            Err(ResourceRouteError::Bindings)
        ));
    }
    for query in [
        vec![("unknown", "value".into())],
        vec![("cursor", "one".into()), ("cursor", "two".into())],
    ] {
        assert!(matches!(
            PAGE.build(&[], &query),
            Err(ResourceRouteError::Bindings)
        ));
    }
    let uri = FEATURE
        .build(
            &[
                scalar("source_feature_id", "feature"),
                scalar("release_id", "release"),
            ],
            &[],
        )
        .unwrap();
    assert_eq!(uri.as_str(), "map://source-feature/release/feature");
}
