//! Terminal retained Map products derive their public address from owner metadata.
use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceAddress, ResourceUri};

pub trait MapTaskProductValue {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError>;
}
impl<T: MapTaskProductValue> MapTaskProductValue for &T {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        (*self).product_uri()
    }
}
#[derive(Debug, Clone, JsonSchema)]
pub struct MapTaskProduct<T: MapTaskProductValue>(veoveo_types::Checked<MapTaskProductWire<T>>);
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct MapTaskProductWire<T: MapTaskProductValue> {
    result_uri: ResourceUri,
    #[serde(flatten)]
    output: T,
}
impl<T: MapTaskProductValue> veoveo_types::Check for MapTaskProductWire<T> {
    type Error = MapResourceError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.result_uri != self.output.product_uri()? {
            return Err(MapResourceError);
        }
        Ok(())
    }
}
impl<T: MapTaskProductValue> MapTaskProduct<T> {
    pub fn new(output: T) -> Result<Self, MapResourceError> {
        veoveo_types::Checked::new(MapTaskProductWire {
            result_uri: output.product_uri()?,
            output,
        })
        .map(Self)
    }
    pub fn result_uri(&self) -> &ResourceUri {
        &self.0.result_uri
    }
    pub fn into_output(self) -> T {
        self.0.into_inner().output
    }
}
impl<T: Serialize + MapTaskProductValue> Serialize for MapTaskProduct<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
impl<'de, T: Deserialize<'de> + MapTaskProductValue> Deserialize<'de> for MapTaskProduct<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = MapTaskProductWire::<T>::deserialize(deserializer)?;
        veoveo_types::Checked::new(wire)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
impl MapTaskProductValue for RoutePlan {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        let uri = MapRouteUri::new(self.route_id.clone());
        if uri != self.route_uri {
            return Err(MapResourceError);
        }
        ResourceAddress::to_uri(&uri).map_err(|_| MapResourceError)
    }
}
impl MapTaskProductValue for RouteMatrix {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        Ok(MapResource::Matrix {
            id: self.matrix_id.clone(),
        }
        .to_uri())
    }
}
impl MapTaskProductValue for TravelModelRecord {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        self.validate_identity().map_err(|_| MapResourceError)?;
        ResourceAddress::to_uri(&MapTravelModelUri::new(self.travel_model_id.clone()))
            .map_err(|_| MapResourceError)
    }
}
impl MapTaskProductValue for RasterDerivation {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        ResourceAddress::to_uri(&MapRasterDerivationUri::new(self.derivation_id.clone()))
            .map_err(|_| MapResourceError)
    }
}
impl MapTaskProductValue for ImportFeatureLayerOutput {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        Ok(MapResource::Changeset {
            layer: self.changeset.layer_id.clone(),
            changeset: self.changeset.changeset_id.clone(),
        }
        .to_uri())
    }
}
impl MapTaskProductValue for ExportFeatureLayerOutput {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        Ok(MapResource::Product {
            layer: self.product.layer_id.clone(),
            publication: self.product.publication_id.clone(),
            product: self.product.product_id.clone(),
        }
        .to_uri())
    }
}
impl MapTaskProductValue for BuildVectorTilesOutput {
    fn product_uri(&self) -> Result<ResourceUri, MapResourceError> {
        Ok(MapResource::Product {
            layer: self.product.layer_id.clone(),
            publication: self.product.publication_id.clone(),
            product: self.product.product_id.clone(),
        }
        .to_uri())
    }
}
