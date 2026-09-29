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
        macro_rules! make {
            ($ty:ty) => {
                if stable {
                    <$ty>::from_stable_key(b"product-fixture")
                } else {
                    <$ty>::new()
                }
            };
        }
        let dataset = make!(MapDatasetId);
        let release = make!(DatasetReleaseId);
        let feature = make!(SourceFeatureId);
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
        macro_rules! single {
            ($uri:ty, $id:ty, $key:literal) => {
                let id = make!($id);
                let uri = <$uri>::new(id.clone());
                assert_eq!(uri.id(), &id);
                check(uri, <$uri>::TEMPLATE, &[($key, id.to_string())]);
            };
        }
        single!(MapRouteUri, RouteId, "route_id");
        single!(MapRasterUri, RasterProductId, "raster_id");
        single!(
            MapRasterDerivationUri,
            RasterDerivationId,
            "raster_derivation_id"
        );
        single!(
            MapSpatialDerivationUri,
            SpatialDerivationId,
            "spatial_derivation_id"
        );
    }
}

#[test]
fn ids_reject_foreign_prefixes_noncanonical_spelling_and_non_rfc_uuids() {
    macro_rules! rejects {
        ($ty:ty) => {
            for raw in [
                "019f7122-3d89-7d21-0312-8940d1e0f510",
                "019F7122-3D89-7D21-8312-8940D1E0F510",
                "019f71223d897d2183128940d1e0f510",
                "019f7122-3d89-4d21-8312-8940d1e0f510",
            ] {
                assert!(<$ty>::parse(format!("{}{raw}", <$ty>::PREFIX)).is_err());
            }
        };
    }
    rejects!(MapDatasetId);
    rejects!(DatasetReleaseId);
    rejects!(SourceFeatureId);
    rejects!(RasterProductId);
    rejects!(RasterDerivationId);
    rejects!(SpatialDerivationId);
    rejects!(RouteId);
    let uri = MapRouteUri::new(RouteId::new());
    assert!(MapRouteUri::parse(uri.as_str().replace("/route-", "/raster-")).is_err());
    assert!(MapSourceFeatureUri::parse("map://source-feature/arbitrary/secret").is_err());
}
