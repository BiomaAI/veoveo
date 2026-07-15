use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tileset {
    pub asset: Asset,
    pub geometric_error: f64,
    pub root: Tile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tileset_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub bounding_volume: BoundingVolume,
    pub geometric_error: f64,
    #[serde(default)]
    pub refine: Option<Refine>,
    #[serde(default)]
    pub transform: Option<[f64; 16]>,
    #[serde(default)]
    pub content: Option<Content>,
    #[serde(default)]
    pub contents: Vec<Content>,
    #[serde(default)]
    pub children: Vec<Tile>,
    #[serde(default)]
    pub implicit_tiling: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Refine {
    #[serde(rename = "REPLACE")]
    Replace,
    #[serde(rename = "ADD")]
    Add,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Content {
    #[serde(alias = "url")]
    pub uri: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundingVolume {
    #[serde(rename = "box", default)]
    pub obb: Option<[f64; 12]>,
    #[serde(default)]
    pub sphere: Option<[f64; 4]>,
    #[serde(default)]
    pub region: Option<[f64; 6]>,
}

#[derive(Debug, Clone, Copy)]
pub enum VolumeKind {
    Box([f64; 12]),
    Sphere([f64; 4]),
    Region([f64; 6]),
}

impl BoundingVolume {
    pub fn kind(&self) -> Option<VolumeKind> {
        self.obb
            .map(VolumeKind::Box)
            .or_else(|| self.sphere.map(VolumeKind::Sphere))
            .or_else(|| self.region.map(VolumeKind::Region))
    }
}

pub fn parse(bytes: &[u8]) -> Result<Tileset, SchemaError> {
    let tileset: Tileset = serde_json::from_slice(bytes)?;
    if tileset.asset.version != "1.0" && tileset.asset.version != "1.1" {
        return Err(SchemaError::UnsupportedVersion(tileset.asset.version));
    }
    validate_tile(&tileset.root)?;
    Ok(tileset)
}

fn validate_tile(tile: &Tile) -> Result<(), SchemaError> {
    if tile.implicit_tiling.is_some() {
        return Err(SchemaError::ImplicitTilingUnsupported);
    }
    if tile.content.is_some() && !tile.contents.is_empty() {
        return Err(SchemaError::AmbiguousContent);
    }
    if tile.contents.len() > 1 {
        return Err(SchemaError::MultipleContentsUnsupported);
    }
    if tile.bounding_volume.kind().is_none() {
        return Err(SchemaError::MissingBoundingVolume);
    }
    for child in &tile.children {
        validate_tile(child)?;
    }
    Ok(())
}

impl Tile {
    pub fn content_uri(&self) -> Option<&str> {
        self.content
            .as_ref()
            .or_else(|| self.contents.first())
            .map(|content| content.uri.as_str())
    }

    pub fn content_uri_mut(&mut self) -> Option<&mut String> {
        self.content
            .as_mut()
            .or_else(|| self.contents.first_mut())
            .map(|content| &mut content.uri)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("tileset JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("3D Tiles version `{0}` is unsupported")]
    UnsupportedVersion(String),
    #[error("implicit tiling is not supported")]
    ImplicitTilingUnsupported,
    #[error("a tile cannot contain both content and contents")]
    AmbiguousContent,
    #[error("multiple contents per tile are not supported")]
    MultipleContentsUnsupported,
    #[error("tile bounding volume is missing")]
    MissingBoundingVolume,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_tree() {
        let bytes = br#"{
          "asset":{"version":"1.1"},
          "geometricError":100,
          "root":{"boundingVolume":{"sphere":[0,0,0,10]},"geometricError":10,"refine":"REPLACE","content":{"uri":"root.glb"}}
        }"#;
        let tileset = parse(bytes).unwrap();
        assert_eq!(tileset.root.content_uri(), Some("root.glb"));
    }

    #[test]
    fn rejects_implicit_tiling_explicitly() {
        let bytes = br#"{
          "asset":{"version":"1.1"},
          "geometricError":100,
          "root":{"boundingVolume":{"sphere":[0,0,0,10]},"geometricError":10,"implicitTiling":{}}
        }"#;
        assert!(matches!(
            parse(bytes),
            Err(SchemaError::ImplicitTilingUnsupported)
        ));
    }
}
