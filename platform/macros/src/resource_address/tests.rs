use super::*;

#[test]
fn declaration_maps_fields_to_variables_and_requires_tail_codecs() {
    let input: DeriveInput = syn::parse_quote! {
        #[resource(template="map://source-feature/{release_id}/{source_feature_id}", error=OwnerError, route_error=map_error)]
        struct Feature {
            release_id: ReleaseId,
            #[resource(variable="source_feature_id")]
            feature_id: FeatureId,
            #[resource(cache)]
            wire: ResourceUri,
        }
    };
    let generated = generate(&input).unwrap();
    // Parse the output as Rust items without loading an owner or Cargo target.
    syn::parse2::<syn::File>(generated.clone()).unwrap();
    let generated = generated.to_string();
    assert!(generated.contains("source_feature_id"));
    assert!(generated.contains("resource_components_uri"));
    assert!(!generated.contains("pub fn resource_build_uri"));
    assert!(!generated.contains("impl :: serde"));

    let invalid: DeriveInput = syn::parse_quote! {
        #[resource(template="time://zones/{+zone}", error=OwnerError, route_error=map_error)]
        struct Zone { #[resource(tail)] zone: ZoneId }
    };
    assert!(generate(&invalid).is_err());
}

#[test]
fn tuple_variants_queries_and_owner_relationship_hooks_expand() {
    let input: DeriveInput = syn::parse_quote! {
        #[resource(error=OwnerError, route_error=map_error, validate=validate, wire)]
        enum Address {
            #[resource(template="time://docs")]
            Docs,
            #[resource(template="time://zones/{+zone}")]
            Zone(#[resource(variable="zone", tail, codec=ZoneCodec, error=zone_error)] ZoneId),
            #[resource(template="time://events{?cursor}")]
            Events { #[resource(codec=CursorCodec, error=cursor_error)] cursor: Option<Cursor> },
        }
    };
    syn::parse2::<syn::File>(generate(&input).unwrap()).unwrap();
}

#[test]
fn unbound_variables_raw_tuple_fields_and_nonoptional_queries_fail() {
    for input in [
        syn::parse_quote! {
            #[resource(template="map://source/{source_id}", error=E, route_error=map)]
            struct Missing { id: SourceId }
        },
        syn::parse_quote! {
            #[resource(template="map://source/{id}", error=E, route_error=map)]
            struct Unmapped(SourceId);
        },
        syn::parse_quote! {
            #[resource(template="map://sources{?cursor}", error=E, route_error=map)]
            struct Required { cursor: Cursor }
        },
        syn::parse_quote! {
            #[resource(template="map://source/{id}/{+tail}/more", error=E, route_error=map)]
            struct BadTail { id: SourceId, #[resource(tail, codec=Tail)] tail: OwnerTail }
        },
    ] {
        assert!(generate(&input).is_err());
    }
}

#[test]
fn ambiguous_shapes_ignored_variant_hooks_and_public_caches_fail() {
    for input in [
        syn::parse_quote! {
            #[resource(error=E, route_error=map)]
            enum Ambiguous {
                #[resource(template="example://item/{id}")]
                Dynamic { id: Id },
                #[resource(template="example://item/current")]
                Literal,
            }
        },
        syn::parse_quote! {
            #[resource(error=E, route_error=map)]
            enum AmbiguousQuery {
                #[resource(template="example://pages{?cursor}")]
                Cursor { cursor: Option<Cursor> },
                #[resource(template="example://pages{?after}")]
                After { after: Option<Cursor> },
            }
        },
        syn::parse_quote! {
            #[resource(error=E, route_error=map)]
            enum Ignored {
                #[resource(template="example://docs", validate=owner)]
                Docs,
            }
        },
        syn::parse_quote! {
            #[resource(template="example://item/{id}", error=E, route_error=map)]
            struct Forged {
                id: Id,
                #[resource(cache)] pub wire: ResourceUri,
            }
        },
    ] {
        assert!(generate(&input).is_err());
    }
}

#[test]
fn variant_route_error_and_canonical_policy_are_explicit_overrides() {
    let input: DeriveInput = syn::parse_quote! {
        #[resource(error=E, route_error=map)]
        enum Address {
            #[resource(template="example://pages{?cursor}", route_error=page_error, canonical=false, allow_empty_query)]
            Page { cursor: Option<Cursor> },
            #[resource(template="example://docs")]
            Docs,
        }
    };
    let generated = generate(&input).unwrap();
    assert!(generated.to_string().contains("page_error"));
    syn::parse2::<syn::File>(generated).unwrap();
}

#[test]
fn mutable_cached_components_and_enum_caches_fail() {
    for input in [
        syn::parse_quote! {
            #[resource(template="example://item/{id}", error=E, route_error=map)]
            struct Mutable {
                pub id: Id,
                #[resource(cache)] wire: ResourceUri,
            }
        },
        syn::parse_quote! {
            #[resource(error=E, route_error=map)]
            enum Exposed {
                #[resource(template="example://item/{id}")]
                Item { id: Id, #[resource(cache)] wire: ResourceUri },
            }
        },
    ] {
        assert!(generate(&input).is_err());
    }
}

#[test]
fn input_hooks_reject_duplicates_and_variant_placement() {
    for input in [
        syn::parse_quote! {
            #[resource(template="example://item/{id}", error=E, route_error=map, input=check, input=other)]
            struct Duplicate { id: Id }
        },
        syn::parse_quote! {
            #[resource(error=E, route_error=map)]
            enum Invalid {
                #[resource(template="example://item/{id}", input=check)]
                Item { id: Id },
            }
        },
    ] {
        assert!(generate(&input).is_err());
    }
}
