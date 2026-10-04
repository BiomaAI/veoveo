fn make<T>(stable: bool, generate: fn() -> T, from_stable: impl Fn(&[u8]) -> T) -> T {
    if stable {
        from_stable(b"product-fixture")
    } else {
        generate()
    }
}
fn single<I, U>(id: I, build: fn(I) -> U, get: fn(&U) -> &I, template: &str, key: &str)
where
    I: Clone + PartialEq + std::fmt::Debug + std::fmt::Display,
    U: ResourceAddress
        + Serialize
        + DeserializeOwned
        + schemars::JsonSchema
        + PartialEq
        + std::fmt::Debug,
    U::Error: std::fmt::Debug,
{
    let uri = build(id.clone());
    assert_eq!(get(&uri), &id);
    check(uri, template, &[(key, id.to_string())]);
}
fn rejects<T: veoveo_types::Identity>(prefix: &str) {
    for raw in [
        "019f7122-3d89-7d21-0312-8940d1e0f510",
        "019F7122-3D89-7D21-8312-8940D1E0F510",
        "019f71223d897d2183128940d1e0f510",
        "019f7122-3d89-4d21-8312-8940d1e0f510",
    ] {
        assert!(T::parse_identity(&format!("{prefix}{raw}")).is_err());
    }
}
use iri_string::template::simple_context::SimpleContext;
use serde::{Serialize, de::DeserializeOwned};
use veoveo_map_mcp::contract::*;
use veoveo_types::{ResourceAddress, ResourceTemplateUri};

fn check<T>(address: T, template: &str, values: &[(&str, String)])
where
    T: ResourceAddress
        + Serialize
        + DeserializeOwned
        + schemars::JsonSchema
        + PartialEq
        + std::fmt::Debug,
    T::Error: std::fmt::Debug,
{
    let uri = address.to_uri().unwrap();
    assert_eq!(MapAddress::parse(uri.as_str()).unwrap().to_uri(), uri);
    assert_eq!(T::parse(&uri).unwrap(), address);
    let json = serde_json::to_value(&address).unwrap();
    assert_eq!(json, uri.as_str());
    assert_eq!(serde_json::from_value::<T>(json).unwrap(), address);
    assert_eq!(schemars::schema_for!(T).as_value()["type"], "string");
    let mut context = SimpleContext::new();
    for (name, value) in values {
        context.insert(*name, value.clone());
    }
    assert_eq!(
        ResourceTemplateUri::new(template)
            .unwrap()
            .expand(&context)
            .unwrap(),
        uri
    );
    for invalid in [
        format!("{uri}/extra"),
        format!("{uri}?cursor=secret"),
        format!("{uri}?"),
        format!("{uri}#secret"),
        uri.as_str().replace("map://", "map://secret@"),
        uri.as_str().replace("map://", "other://"),
        uri.as_str().replace("map://", "MAP://"),
        uri.as_str().replacen("-", "%2D", 1),
        uri.as_str().replace("/release/", "/wrong-parent/"),
    ] {
        if invalid == uri.as_str() {
            continue;
        }
        let error = serde_json::from_value::<T>(invalid.into()).unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
}

#[test]
fn builders_and_templates_share_exact_owner_ids() {
    for stable in [false, true] {
        let dataset = make(stable, MapDatasetId::new, |key| {
            MapDatasetId::from_stable_key(key)
        });
        let release = make(stable, DatasetReleaseId::new, |key| {
            DatasetReleaseId::from_stable_key(key)
        });
        let feature = make(stable, SourceFeatureId::new, |key| {
            SourceFeatureId::from_stable_key(key)
        });
        check(
            MapReleaseUri::new(dataset.clone(), release.clone()),
            MapReleaseUri::TEMPLATE,
            &[
                ("dataset_id", dataset.to_string()),
                ("release_id", release.to_string()),
            ],
        );
        check(
            MapSourceFeatureUri::new(release.clone(), feature.clone()),
            MapSourceFeatureUri::TEMPLATE,
            &[
                ("release_id", release.to_string()),
                ("source_feature_id", feature.to_string()),
            ],
        );

        single(
            make(stable, RouteId::new, RouteId::from_stable_key),
            MapRouteUri::new,
            MapRouteUri::id,
            MapRouteUri::TEMPLATE,
            "route_id",
        );
        single(
            make(stable, RasterProductId::new, |key| {
                RasterProductId::from_stable_key(key)
            }),
            MapRasterUri::new,
            MapRasterUri::id,
            MapRasterUri::TEMPLATE,
            "raster_id",
        );
        single(
            make(stable, RasterDerivationId::new, |key| {
                RasterDerivationId::from_stable_key(key)
            }),
            MapRasterDerivationUri::new,
            MapRasterDerivationUri::id,
            MapRasterDerivationUri::TEMPLATE,
            "raster_derivation_id",
        );
        single(
            make(stable, SpatialDerivationId::new, |key| {
                SpatialDerivationId::from_stable_key(key)
            }),
            MapSpatialDerivationUri::new,
            MapSpatialDerivationUri::id,
            MapSpatialDerivationUri::TEMPLATE,
            "spatial_derivation_id",
        );
    }
}

#[test]
fn ids_reject_foreign_prefixes_noncanonical_spelling_and_non_rfc_uuids() {
    rejects::<MapDatasetId>(MapDatasetId::PREFIX);
    rejects::<DatasetReleaseId>(DatasetReleaseId::PREFIX);
    rejects::<SourceFeatureId>(SourceFeatureId::PREFIX);
    rejects::<RasterProductId>(RasterProductId::PREFIX);
    rejects::<RasterDerivationId>(RasterDerivationId::PREFIX);
    rejects::<SpatialDerivationId>(SpatialDerivationId::PREFIX);
    rejects::<RouteId>(RouteId::PREFIX);
    let uri = MapRouteUri::new(RouteId::new());
    assert!(MapRouteUri::parse(uri.as_str().replace("/route-", "/raster-")).is_err());
    assert!(MapSourceFeatureUri::parse("map://source-feature/arbitrary/secret").is_err());
}

#[test]
fn shared_product_routes_preserve_public_templates_and_complex_field_mapping() {
    for (routes, template) in [
        (MapReleaseUri::RESOURCE_ROUTES, MapReleaseUri::TEMPLATE),
        (
            MapSourceFeatureUri::RESOURCE_ROUTES,
            MapSourceFeatureUri::TEMPLATE,
        ),
        (MapRouteUri::RESOURCE_ROUTES, MapRouteUri::TEMPLATE),
        (MapRasterUri::RESOURCE_ROUTES, MapRasterUri::TEMPLATE),
        (
            MapRasterDerivationUri::RESOURCE_ROUTES,
            MapRasterDerivationUri::TEMPLATE,
        ),
        (
            MapSpatialDerivationUri::RESOURCE_ROUTES,
            MapSpatialDerivationUri::TEMPLATE,
        ),
    ] {
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].discovery_template().unwrap(), template);
    }
    let release = DatasetReleaseId::new();
    let feature = SourceFeatureId::new();
    let address = MapSourceFeatureUri::new(release.clone(), feature.clone());
    assert_eq!(address.release_id(), &release);
    assert_eq!(address.feature_id(), &feature);
    assert_eq!(
        address.resource_components_uri().unwrap(),
        address.to_uri().unwrap()
    );
    assert_eq!(
        MapSourceFeatureUri::parse(address.as_str()).unwrap(),
        address
    );
}

#[test]
fn product_route_patterns_validate_builders_and_reject_wrong_structure() {
    let address = MapSourceFeatureUri::new(DatasetReleaseId::new(), SourceFeatureId::new());
    let schema = serde_json::json!({"type": "string", "pattern": MapSourceFeatureUri::RESOURCE_ROUTES[0].wire_pattern().unwrap()});
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&serde_json::json!(address.as_str())));
    for wire in [
        address.as_str().replace("map://", "other://"),
        format!("{address}/extra"),
        "map://source-feature/one".into(),
    ] {
        assert!(!validator.is_valid(&serde_json::json!(wire)), "{wire}");
    }
    // Structural schemas deliberately leave identity versions and parents to admission.
    assert!(validator.is_valid(&serde_json::json!(
        "map://source-feature/not-an-id/not-an-id"
    )));
    assert!(MapSourceFeatureUri::parse("map://source-feature/not-an-id/not-an-id").is_err());
}
