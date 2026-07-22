use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::Path,
};

use anyhow::{Context, Result, bail};
use duckdb::params;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};
use veoveo_duckdb_runtime::quote_sql_literal;
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::{
    catalog::MapScope,
    contract::{
        BuildVectorTilesRequest, CommitFeatureChangesRequest, FeatureExportFormat, FeatureGeometry,
        FeatureImportFormat, FeatureInput, FeatureMutation, FeatureTime, GeoJsonFeatureType,
        ImportFeatureLayerOutput, ImportFeatureLayerRequest, JSON_FG_CORE_CONFORMANCE,
        JSON_FG_TYPES_SCHEMAS_CONFORMANCE, LayerProductFormat, LayerStyle, MAX_IMPORT_FEATURES,
        MAX_VECTOR_TILES, MapFeatureId, QueryFeaturesRequest, StyleRule, TileCoordinate,
    },
};

use super::{AuthoringService, service::decode};

const GEOJSON_SEQ_MIME: &str = "application/geo+json-seq";
const GEOPARQUET_MIME: &str = "application/vnd.apache.parquet";
const MVT_BUNDLE_MIME: &str = "application/vnd.veoveo.mvt-bundle+tar";

#[derive(Debug)]
pub struct GeneratedLayerProduct {
    pub bytes: Vec<u8>,
    pub format: LayerProductFormat,
    pub mime_type: &'static str,
    pub filename: String,
    pub feature_count: u64,
    pub digest_sha256: String,
    pub tile_count: Option<u64>,
}

impl AuthoringService {
    pub async fn import_features(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: ImportFeatureLayerRequest,
        bytes: &[u8],
    ) -> Result<ImportFeatureLayerOutput> {
        validate_semantic_type(&request.default_semantic_type)?;
        let inputs = parse_import(bytes, request.format, &request.default_semantic_type)?;
        let imported_feature_count = u64::try_from(inputs.len())?;
        let mutations = inputs
            .into_iter()
            .map(|feature| FeatureMutation::Create { feature })
            .collect();
        let commit = self
            .commit_import_changes(
                identity,
                scope,
                CommitFeatureChangesRequest {
                    layer_id: request.layer_id,
                    expected_layer_revision: request.expected_layer_revision,
                    idempotency_key: request.idempotency_key,
                    mutations,
                },
            )
            .await?;
        Ok(ImportFeatureLayerOutput {
            imported_feature_count,
            changeset: commit.changeset,
            projection_state: commit.projection_state,
        })
    }

    pub async fn generate_export(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: &crate::contract::ExportFeatureLayerRequest,
        task_directory: &Path,
        maximum_bytes: u64,
    ) -> Result<GeneratedLayerProduct> {
        let publication = self
            .product_publication(identity, scope, &request.layer_id, &request.publication_id)
            .await?;
        self.reconcile_projection().await?;
        match request.format {
            FeatureExportFormat::GeoJsonSeq => {
                let (bytes, feature_count) = self
                    .encode_publication_geojson_sequence(
                        identity,
                        scope,
                        &request.layer_id,
                        &request.publication_id,
                        maximum_bytes,
                    )
                    .await?;
                Ok(generated(
                    bytes,
                    LayerProductFormat::GeoJsonSeq,
                    GEOJSON_SEQ_MIME,
                    format!("{}.geojsons", request.publication_id),
                    feature_count,
                    None,
                ))
            }
            FeatureExportFormat::GeoParquet => {
                let path = task_directory.join("layer.parquet");
                let connection = self.analytics.task_connection(task_directory)?;
                write_geoparquet(
                    &connection,
                    &path,
                    &scope.identity.tenant_key,
                    identity.authority.work_context.as_str(),
                    request.layer_id.as_str(),
                    publication.layer_revision,
                )?;
                let feature_count = publication_feature_count(
                    &connection,
                    &scope.identity.tenant_key,
                    identity.authority.work_context.as_str(),
                    request.layer_id.as_str(),
                    publication.layer_revision,
                )?;
                let metadata = std::fs::metadata(&path)?;
                if metadata.len() > maximum_bytes {
                    bail!("GeoParquet export exceeds the configured artifact byte limit");
                }
                let bytes = std::fs::read(&path)?;
                Ok(generated(
                    bytes,
                    LayerProductFormat::GeoParquet,
                    GEOPARQUET_MIME,
                    format!("{}.parquet", request.publication_id),
                    feature_count,
                    None,
                ))
            }
        }
    }

    pub async fn generate_vector_tiles(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        request: &BuildVectorTilesRequest,
        task_directory: &Path,
        maximum_bytes: u64,
    ) -> Result<GeneratedLayerProduct> {
        validate_tiles(&request.tiles)?;
        let publication = self
            .product_publication(identity, scope, &request.layer_id, &request.publication_id)
            .await?;
        self.reconcile_projection().await?;
        let style = if let Some(style_id) = &publication.style_revision_id {
            self.store()
                .map_style_revision_by_key(
                    &scope.identity.tenant_key,
                    identity.authority.work_context.as_str(),
                    style_id.as_str(),
                )
                .await?
                .map(|record| decode::<LayerStyle>(&record.style_json, "publication style"))
                .transpose()?
        } else {
            None
        };
        let connection = self.analytics.task_connection(task_directory)?;
        let label_properties = style
            .iter()
            .flat_map(|style| &style.rules)
            .filter_map(|rule| rule.label_property.clone())
            .collect::<BTreeSet<_>>();
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = Builder::new(&mut output);
            let manifest = serde_json::to_vec_pretty(&serde_json::json!({
                "format": "Mapbox Vector Tile",
                "version": "2.1",
                "layer": "features",
                "publication_id": request.publication_id,
                "layer_id": request.layer_id,
                "layer_revision": publication.layer_revision,
                "extent": 4096,
                "tiles": request.tiles,
            }))?;
            append_tar(&mut archive, "manifest.json", &manifest)?;
            let style_json = maplibre_style(&request.layer_id.to_string(), style.as_ref());
            append_tar(
                &mut archive,
                "style.json",
                &serde_json::to_vec_pretty(&style_json)?,
            )?;
            for tile in &request.tiles {
                let bytes = vector_tile(
                    &connection,
                    &scope.identity.tenant_key,
                    identity.authority.work_context.as_str(),
                    request.layer_id.as_str(),
                    publication.layer_revision,
                    tile,
                    &label_properties,
                )?;
                append_tar(
                    &mut archive,
                    &format!("{}/{}/{}.mvt", tile.z, tile.x, tile.y),
                    &bytes,
                )?;
                if archive.get_ref().get_ref().len() as u64 > maximum_bytes {
                    bail!("vector tile bundle exceeds the configured artifact byte limit");
                }
            }
            archive.finish()?;
        }
        let bytes = output.into_inner();
        if bytes.len() as u64 > maximum_bytes {
            bail!("vector tile bundle exceeds the configured artifact byte limit");
        }
        let feature_count = publication_feature_count(
            &connection,
            &scope.identity.tenant_key,
            identity.authority.work_context.as_str(),
            request.layer_id.as_str(),
            publication.layer_revision,
        )?;
        Ok(generated(
            bytes,
            LayerProductFormat::MvtBundle,
            MVT_BUNDLE_MIME,
            format!("{}-mvt.tar", request.publication_id),
            feature_count,
            Some(u64::try_from(request.tiles.len())?),
        ))
    }

    async fn encode_publication_geojson_sequence(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapScope,
        layer_id: &crate::contract::FeatureLayerId,
        publication_id: &crate::contract::LayerPublicationId,
        maximum_bytes: u64,
    ) -> Result<(Vec<u8>, u64)> {
        let mut output = Vec::new();
        let mut feature_count = 0_u64;
        let mut cursor = None;
        loop {
            let page = self
                .query_features(
                    identity,
                    scope,
                    QueryFeaturesRequest {
                        layer_id: layer_id.clone(),
                        publication_id: Some(publication_id.clone()),
                        bbox: None,
                        datetime: None,
                        geometry_type: None,
                        filter: None,
                        limit: 1_000,
                        cursor,
                        minimum_commit_sequence: None,
                    },
                )
                .await?;
            for feature in page.features {
                output.push(0x1e);
                serde_json::to_writer(&mut output, &feature)?;
                output.push(b'\n');
                if output.len() as u64 > maximum_bytes {
                    bail!(
                        "GeoJSON text sequence export exceeds the configured artifact byte limit"
                    );
                }
                feature_count = feature_count
                    .checked_add(1)
                    .context("GeoJSON text sequence feature count overflow")?;
            }
            let Some(next) = page.next_cursor else {
                break;
            };
            cursor = Some(next);
        }
        Ok((output, feature_count))
    }
}

#[derive(Debug, Deserialize)]
struct ImportFeatureCollection {
    #[serde(rename = "type")]
    feature_type: String,
    features: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ImportGeoJsonFeature {
    #[serde(rename = "type")]
    feature_type: GeoJsonFeatureType,
    #[serde(default, rename = "conformsTo")]
    conforms_to: Vec<String>,
    #[serde(default)]
    id: Option<serde_json::Value>,
    geometry: FeatureGeometry,
    #[serde(default)]
    properties: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "featureType")]
    semantic_type: Option<String>,
    #[serde(default)]
    time: Option<FeatureTime>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    related_resources: Vec<String>,
    #[serde(default)]
    evidence_resources: Vec<String>,
}

fn parse_import(
    bytes: &[u8],
    format: FeatureImportFormat,
    default_semantic_type: &str,
) -> Result<Vec<FeatureInput>> {
    let values = match format {
        FeatureImportFormat::GeoJsonFeatureCollection => {
            let collection: ImportFeatureCollection = serde_json::from_slice(bytes)
                .context("decoding GeoJSON FeatureCollection import")?;
            if collection.feature_type != "FeatureCollection" {
                bail!("GeoJSON import root type must be FeatureCollection");
            }
            collection.features
        }
        FeatureImportFormat::GeoJsonTextSequence => parse_geojson_sequence(bytes)?,
    };
    if values.is_empty() || values.len() > MAX_IMPORT_FEATURES {
        bail!("a feature import must contain between one and {MAX_IMPORT_FEATURES} features");
    }
    values
        .into_iter()
        .map(|value| imported_feature(value, default_semantic_type))
        .collect()
}

fn parse_geojson_sequence(bytes: &[u8]) -> Result<Vec<serde_json::Value>> {
    if bytes.first() != Some(&0x1e) {
        bail!("GeoJSON text sequence must begin each record with ASCII RS (0x1E)");
    }
    let mut values = Vec::new();
    for record in bytes.split(|byte| *byte == 0x1e).skip(1) {
        if !record.ends_with(b"\n") {
            bail!("each GeoJSON text sequence record must end with LF");
        }
        let payload = &record[..record.len() - 1];
        if payload.is_empty() || payload.contains(&0x1e) {
            bail!("GeoJSON text sequence contains an empty record");
        }
        values.push(serde_json::from_slice(payload).context("decoding GeoJSON sequence record")?);
    }
    Ok(values)
}

fn imported_feature(value: serde_json::Value, default_semantic_type: &str) -> Result<FeatureInput> {
    let feature: ImportGeoJsonFeature =
        serde_json::from_value(value).context("decoding imported GeoJSON Feature")?;
    let _ = feature.feature_type;
    if feature.semantic_type.is_some()
        && (!feature
            .conforms_to
            .iter()
            .any(|value| value == JSON_FG_CORE_CONFORMANCE)
            || !feature
                .conforms_to
                .iter()
                .any(|value| value == JSON_FG_TYPES_SCHEMAS_CONFORMANCE))
    {
        bail!("JSON-FG input must declare the core and types-schemas conformance classes");
    }
    let semantic_type = feature
        .semantic_type
        .unwrap_or_else(|| default_semantic_type.to_owned());
    validate_semantic_type(&semantic_type)?;
    let feature_id = feature.id.map(import_feature_id).transpose()?;
    Ok(FeatureInput {
        feature_id,
        geometry: feature.geometry,
        properties: feature.properties,
        semantic_type,
        time: feature.time,
        title: feature.title,
        related_resources: feature.related_resources,
        evidence_resources: feature.evidence_resources,
    })
}

fn import_feature_id(value: serde_json::Value) -> Result<MapFeatureId> {
    let external = match value {
        serde_json::Value::String(value) => value,
        serde_json::Value::Number(value) => value.to_string(),
        _ => bail!("GeoJSON feature id must be a string or number"),
    };
    Ok(external
        .parse()
        .unwrap_or_else(|_| MapFeatureId::from_stable_key(external.as_bytes())))
}

fn validate_semantic_type(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        bail!("semantic feature type must be 1..=256 bytes without control characters");
    }
    Ok(())
}

fn publication_source_sql() -> &'static str {
    "SELECT * EXCLUDE (version_rank) FROM (
       SELECT *, row_number() OVER (PARTITION BY feature_key ORDER BY feature_revision DESC) AS version_rank
       FROM map_authored_feature_revision
       WHERE tenant_key = ? AND work_context_key = ? AND layer_key = ? AND layer_revision <= ?
     ) WHERE version_rank = 1"
}

fn verify_geoparquet(connection: &duckdb::Connection, path: &Path) -> Result<()> {
    let path = path.to_str().context("GeoParquet path is not UTF-8")?;
    let geo_metadata: String = connection.query_row(
        "SELECT decode(value) FROM parquet_kv_metadata(?) WHERE decode(key) = 'geo'",
        params![path],
        |row| row.get(0),
    )?;
    let geo: serde_json::Value =
        serde_json::from_str(&geo_metadata).context("decoding GeoParquet geo metadata")?;
    if geo.pointer("/version").and_then(serde_json::Value::as_str) != Some("1.0.0")
        || geo
            .pointer("/primary_column")
            .and_then(serde_json::Value::as_str)
            != Some("geometry")
        || geo
            .pointer("/columns/geometry/encoding")
            .and_then(serde_json::Value::as_str)
            != Some("WKB")
    {
        bail!("DuckDB output did not satisfy the GeoParquet 1.0 WKB metadata contract");
    }
    Ok(())
}

fn write_geoparquet(
    connection: &duckdb::Connection,
    path: &Path,
    tenant_key: &str,
    context_key: &str,
    layer_key: &str,
    layer_revision: u64,
) -> Result<()> {
    let path_literal =
        quote_sql_literal(path.to_str().context("GeoParquet task path is not UTF-8")?);
    let source = publication_source_sql();
    let sql = format!(
        "COPY (SELECT feature_key AS id, semantic_type AS feature_type, title, properties_json AS properties, valid_from, valid_until, geometry FROM ({source}) AS authored WHERE deleted = false ORDER BY feature_key) TO {path_literal} (FORMAT PARQUET, COMPRESSION ZSTD);"
    );
    connection.execute(
        &sql,
        params![
            tenant_key,
            context_key,
            layer_key,
            i64::try_from(layer_revision)?,
        ],
    )?;
    verify_geoparquet(connection, path)
}

fn validate_tiles(tiles: &[TileCoordinate]) -> Result<()> {
    if tiles.is_empty() || tiles.len() > MAX_VECTOR_TILES {
        bail!("a vector tile task must contain between one and {MAX_VECTOR_TILES} tiles");
    }
    let mut previous = None;
    for tile in tiles {
        tile.validate().map_err(anyhow::Error::msg)?;
        if previous.is_some_and(|value| value >= *tile) {
            bail!("vector tile coordinates must be unique and sorted by z, x, then y");
        }
        previous = Some(*tile);
    }
    Ok(())
}

fn vector_tile(
    connection: &duckdb::Connection,
    tenant_key: &str,
    context_key: &str,
    layer_key: &str,
    layer_revision: u64,
    tile: &TileCoordinate,
    label_properties: &BTreeSet<String>,
) -> Result<Vec<u8>> {
    let source = publication_source_sql();
    let label_fields = label_properties
        .iter()
        .filter(|property| !matches!(property.as_str(), "feature_id" | "feature_type" | "title"))
        .map(|property| {
            let field = quote_sql_literal(property);
            let pointer = quote_sql_literal(&format!(
                "/{}",
                property.replace('~', "~0").replace('/', "~1")
            ));
            format!(", {field}: json_extract_string(properties_json, {pointer})")
        })
        .collect::<String>();
    let sql = format!(
        "WITH authored AS ({source}),
         bounds AS (SELECT ST_TileEnvelope(?, ?, ?) AS geometry),
         tile_rows AS (
           SELECT feature_key, semantic_type, title, properties_json,
                  ST_AsMVTGeom(ST_Transform(authored.geometry, 'EPSG:4326', 'EPSG:3857', true), ST_Extent(bounds.geometry), 4096, 256, true) AS geometry
           FROM authored, bounds
           WHERE authored.deleted = false
             AND ST_Intersects(ST_Transform(authored.geometry, 'EPSG:4326', 'EPSG:3857', true), bounds.geometry)
         )
         SELECT ST_AsMVT({{'feature_id': feature_key, 'feature_type': semantic_type, 'title': title{label_fields}, 'geometry': geometry}}, 'features', 4096, 'geometry') FROM tile_rows"
    );
    connection
        .query_row(
            &sql,
            params![
                tenant_key,
                context_key,
                layer_key,
                i64::try_from(layer_revision)?,
                i32::from(tile.z),
                i32::try_from(tile.x)?,
                i32::try_from(tile.y)?,
            ],
            |row| row.get(0),
        )
        .context("building Mapbox Vector Tile")
}

fn publication_feature_count(
    connection: &duckdb::Connection,
    tenant_key: &str,
    context_key: &str,
    layer_key: &str,
    layer_revision: u64,
) -> Result<u64> {
    let source = publication_source_sql();
    let count: i64 = connection.query_row(
        &format!("SELECT count(*) FROM ({source}) AS authored WHERE deleted = false"),
        params![
            tenant_key,
            context_key,
            layer_key,
            i64::try_from(layer_revision)?
        ],
        |row| row.get(0),
    )?;
    u64::try_from(count).context("publication feature count is negative")
}

fn append_tar(builder: &mut Builder<&mut Cursor<Vec<u8>>>, path: &str, bytes: &[u8]) -> Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(u64::try_from(bytes.len())?);
    header.set_mode(0o644);
    header.set_mtime(0);
    header.set_cksum();
    builder.append_data(&mut header, path, bytes)?;
    Ok(())
}

fn maplibre_style(layer_id: &str, style: Option<&LayerStyle>) -> serde_json::Value {
    let rules = style.map_or_else(default_style_rules, |style| style.rules.clone());
    let layers = rules
        .iter()
        .enumerate()
        .flat_map(|(index, rule)| maplibre_layers(layer_id, index, rule))
        .collect::<Vec<_>>();
    serde_json::json!({
        "version": 8,
        "name": layer_id,
        "sources": {
            "veoveo": {
                "type": "vector",
                "tiles": ["{z}/{x}/{y}.mvt"],
                "minzoom": 0,
                "maxzoom": 22
            }
        },
        "layers": layers
    })
}

fn default_style_rules() -> Vec<StyleRule> {
    use crate::contract::FeatureGeometryType::{LineString, Point, Polygon};
    vec![
        StyleRule {
            geometry_type: Some(Polygon),
            fill_color: Some("#4a7dd6".to_owned()),
            fill_opacity: Some(0.35),
            minimum_zoom: None,
            maximum_zoom: None,
            line_color: None,
            line_width_px: None,
            circle_color: None,
            circle_radius_px: None,
            label_property: None,
        },
        StyleRule {
            geometry_type: Some(LineString),
            line_color: Some("#4a7dd6".to_owned()),
            line_width_px: Some(2.0),
            minimum_zoom: None,
            maximum_zoom: None,
            fill_color: None,
            fill_opacity: None,
            circle_color: None,
            circle_radius_px: None,
            label_property: None,
        },
        StyleRule {
            geometry_type: Some(Point),
            circle_color: Some("#4a7dd6".to_owned()),
            circle_radius_px: Some(5.0),
            minimum_zoom: None,
            maximum_zoom: None,
            fill_color: None,
            fill_opacity: None,
            line_color: None,
            line_width_px: None,
            label_property: None,
        },
    ]
}

fn maplibre_layers(layer_id: &str, index: usize, rule: &StyleRule) -> Vec<serde_json::Value> {
    use crate::contract::FeatureGeometryType::{
        LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
    };
    let layer_type = match rule.geometry_type {
        Some(Point | MultiPoint) => "circle",
        Some(LineString | MultiLineString) => "line",
        Some(Polygon | MultiPolygon) | None => "fill",
    };
    let mut value = serde_json::json!({
        "id": format!("{layer_id}-{index}"),
        "type": layer_type,
        "source": "veoveo",
        "source-layer": "features",
        "paint": {}
    });
    apply_rule_bounds(&mut value, rule);
    let paint = value["paint"].as_object_mut().expect("paint is an object");
    match layer_type {
        "circle" => {
            paint.insert(
                "circle-color".to_owned(),
                serde_json::json!(rule.circle_color.as_deref().unwrap_or("#4a7dd6")),
            );
            paint.insert(
                "circle-radius".to_owned(),
                serde_json::json!(rule.circle_radius_px.unwrap_or(5.0)),
            );
        }
        "line" => {
            paint.insert(
                "line-color".to_owned(),
                serde_json::json!(rule.line_color.as_deref().unwrap_or("#4a7dd6")),
            );
            paint.insert(
                "line-width".to_owned(),
                serde_json::json!(rule.line_width_px.unwrap_or(2.0)),
            );
        }
        _ => {
            paint.insert(
                "fill-color".to_owned(),
                serde_json::json!(rule.fill_color.as_deref().unwrap_or("#4a7dd6")),
            );
            paint.insert(
                "fill-opacity".to_owned(),
                serde_json::json!(rule.fill_opacity.unwrap_or(0.35)),
            );
        }
    }
    let mut layers = vec![value];
    if let Some(property) = &rule.label_property {
        let mut label = serde_json::json!({
            "id": format!("{layer_id}-{index}-label"),
            "type": "symbol",
            "source": "veoveo",
            "source-layer": "features",
            "layout": {
                "text-field": ["get", property],
                "text-size": 12
            },
            "paint": {
                "text-color": "#18212b",
                "text-halo-color": "#ffffff",
                "text-halo-width": 1
            }
        });
        apply_rule_bounds(&mut label, rule);
        layers.push(label);
    }
    layers
}

fn apply_rule_bounds(value: &mut serde_json::Value, rule: &StyleRule) {
    use crate::contract::FeatureGeometryType::{
        LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
    };
    if let Some(minimum) = rule.minimum_zoom {
        value["minzoom"] = serde_json::json!(minimum);
    }
    if let Some(maximum) = rule.maximum_zoom {
        value["maxzoom"] = serde_json::json!(maximum);
    }
    if let Some(geometry_type) = rule.geometry_type {
        let geometry_type = match geometry_type {
            Point | MultiPoint => "Point",
            LineString | MultiLineString => "LineString",
            Polygon | MultiPolygon => "Polygon",
        };
        value["filter"] = serde_json::json!(["==", ["geometry-type"], geometry_type]);
    }
}

fn generated(
    bytes: Vec<u8>,
    format: LayerProductFormat,
    mime_type: &'static str,
    filename: String,
    feature_count: u64,
    tile_count: Option<u64>,
) -> GeneratedLayerProduct {
    let digest = Sha256::digest(&bytes);
    GeneratedLayerProduct {
        bytes,
        format,
        mime_type,
        filename,
        feature_count,
        digest_sha256: hex::encode(digest),
        tile_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geojson_sequence_requires_record_separators_and_line_feeds() {
        assert!(parse_geojson_sequence(br#"{"type":"Feature"}\n"#).is_err());
        assert!(parse_geojson_sequence(b"\x1e{}\n").is_ok());
        assert!(parse_geojson_sequence(b"\x1e{}").is_err());
    }

    #[test]
    fn external_geojson_ids_map_stably() {
        let first = import_feature_id(serde_json::json!("external-42")).unwrap();
        let second = import_feature_id(serde_json::json!("external-42")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn configured_duckdb_writes_geoparquet_and_mvt_products() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let request = root.path().join("request");
        let analytics =
            crate::analytics::MapAnalytics::open(crate::analytics::MapAnalyticsConfig {
                database_path: root.path().join("map.duckdb"),
                spill_dir: root.path().join("spill"),
                spatial_extension: extension.into(),
                memory_limit: "256MB".to_owned(),
                threads: 1,
            })
            .unwrap();
        analytics
            .connection(false)
            .unwrap()
            .execute(
                "INSERT INTO map_authored_feature_revision VALUES (?, ?, ?, ?, 1, 1, 1, ?, 1, false, 'Point', ST_Point(-89.2, 13.7), -89.2, 13.7, -89.2, 13.7, NULL, NULL, 'place', 'Test', '{\"name\":\"Test\"}'::JSON, '{}'::JSON, now())",
                params!["tenant", "context", "feature-layer-test", "feature-test", "changeset-test"],
            )
            .unwrap();
        let connection = analytics.task_connection(&request).unwrap();
        let parquet = request.join("layer.parquet");
        write_geoparquet(
            &connection,
            &parquet,
            "tenant",
            "context",
            "feature-layer-test",
            1,
        )
        .unwrap();
        assert!(std::fs::metadata(parquet).unwrap().len() > 0);
        let tile = vector_tile(
            &connection,
            "tenant",
            "context",
            "feature-layer-test",
            1,
            &TileCoordinate { z: 0, x: 0, y: 0 },
            &["name".to_owned()].into_iter().collect(),
        )
        .unwrap();
        assert!(!tile.is_empty());
    }

    #[test]
    fn maplibre_projection_filters_geometry_and_projects_labels() {
        let rule = StyleRule {
            geometry_type: Some(crate::contract::FeatureGeometryType::MultiPoint),
            minimum_zoom: Some(3.0),
            maximum_zoom: Some(12.0),
            fill_color: None,
            fill_opacity: None,
            line_color: None,
            line_width_px: None,
            circle_color: Some("#123456".to_owned()),
            circle_radius_px: Some(4.0),
            label_property: Some("name".to_owned()),
        };
        let layers = maplibre_layers("layer", 0, &rule);
        assert_eq!(layers.len(), 2);
        assert_eq!(
            layers[0]["filter"],
            serde_json::json!(["==", ["geometry-type"], "Point"])
        );
        assert_eq!(
            layers[1]["layout"]["text-field"],
            serde_json::json!(["get", "name"])
        );
    }
}
