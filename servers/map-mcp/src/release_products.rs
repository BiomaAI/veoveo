use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use geojson::{Feature, GeoJson, Geometry, GeometryValue};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    analytics::{MapAnalytics, NetworkEdge},
    contract::{
        DatasetRelease, Facility, FacilityId, FeatureGeometry, GeoJsonPosition, LocationId,
        MAX_SOURCE_GEOMETRY_COLLECTION_DEPTH, MAX_SOURCE_GEOMETRY_PARTS, MapBoundary,
        MapBoundaryId, MapFamily, MapLocation, RASTER_PRODUCT_SCHEMA_VERSION, RasterBand,
        RasterProduct, RasterProductId, RegisteredSource, SOURCE_FEATURE_SCHEMA_VERSION,
        SourceElementType, SourceFeature, SourceFeatureId, SourceFeatureRepresentation,
        SourceLineage, Wgs84LineString, Wgs84Polygon, Wgs84Position, representation_for_geometry,
    },
};

#[derive(Clone, Debug)]
pub struct ReleaseProductConfig {
    pub release_root: PathBuf,
    pub valhalla_active_dir: PathBuf,
    pub maximum_routing_expanded_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct ReleaseProducts {
    config: ReleaseProductConfig,
    analytics: MapAnalytics,
}

impl ReleaseProducts {
    pub fn new(config: ReleaseProductConfig, analytics: MapAnalytics) -> Result<Self> {
        if !config.release_root.is_absolute()
            || !config.valhalla_active_dir.is_absolute()
            || config.maximum_routing_expanded_bytes == 0
        {
            bail!("release-product paths must be absolute");
        }
        std::fs::create_dir_all(&config.release_root)?;
        Ok(Self { config, analytics })
    }

    pub async fn stage(
        &self,
        release: &DatasetRelease,
        normalized_paths: &[PathBuf],
        routing_build_path: Option<&Path>,
    ) -> Result<()> {
        let root = self.config.release_root.clone();
        let release_id = release.release_id.to_string();
        let normalized_paths = normalized_paths.to_vec();
        let routing_build_path = routing_build_path.map(Path::to_owned);
        let maximum_routing_expanded_bytes = self.config.maximum_routing_expanded_bytes;
        tokio::task::spawn_blocking(move || {
            let destination = root.join(&release_id);
            if destination.exists() {
                bail!("release product directory already exists");
            }
            let temporary = root.join(format!(".{release_id}.{}", uuid::Uuid::now_v7()));
            std::fs::create_dir(&temporary)?;
            let result = (|| -> Result<()> {
                for (index, source) in normalized_paths.iter().enumerate() {
                    copy_product(source, &temporary, index, false)?;
                }
                if let Some(source) = routing_build_path.as_deref() {
                    copy_product(source, &temporary, 0, true)?;
                    extract_routing_build(
                        &temporary.join("routing-build.tar.gz"),
                        &temporary.join("routing-tiles"),
                        maximum_routing_expanded_bytes,
                    )?;
                }
                std::fs::rename(&temporary, &destination)?;
                Ok(())
            })();
            if result.is_err() {
                let _ = std::fs::remove_dir_all(&temporary);
            }
            result
        })
        .await?
    }

    pub async fn discard(&self, release: &DatasetRelease) {
        let path = self.config.release_root.join(release.release_id.as_str());
        let _ = tokio::fs::remove_dir_all(path).await;
    }

    pub async fn prepare(
        &self,
        tenant_key: &str,
        release: &DatasetRelease,
        source: &RegisteredSource,
    ) -> Result<()> {
        let directory = self.config.release_root.join(release.release_id.as_str());
        let tenant_key = tenant_key.to_owned();
        let release = release.clone();
        let source = source.clone();
        let analytics = self.analytics.clone();
        tokio::task::spawn_blocking(move || {
            if !directory.is_dir() {
                bail!(
                    "local products for release {} are unavailable",
                    release.release_id
                );
            }
            analytics.remove_release_products(&tenant_key, &release.release_id)?;
            let result = ingest_directory(&analytics, &tenant_key, &directory, &release, &source);
            if result.is_err() {
                let _ = analytics.remove_release_products(&tenant_key, &release.release_id);
            }
            result
        })
        .await?
    }

    pub async fn activate(&self, tenant_key: &str, release: &DatasetRelease) -> Result<()> {
        let directory = self.config.release_root.join(release.release_id.as_str());
        let active = self.config.valhalla_active_dir.clone();
        let routing = directory.join("routing-tiles");
        if routing.is_dir() {
            tokio::task::spawn_blocking(move || activate_routing_build(&routing, &active))
                .await??;
        }
        let analytics = self.analytics.clone();
        let tenant_key = tenant_key.to_owned();
        let dataset_id = release.dataset_id.clone();
        let release_id = release.release_id.clone();
        tokio::task::spawn_blocking(move || {
            analytics.activate_release(&tenant_key, &dataset_id, &release_id)
        })
        .await??;
        Ok(())
    }
}

fn copy_product(source: &Path, destination: &Path, index: usize, routing: bool) -> Result<()> {
    if !source.is_file() {
        bail!("release product is not a regular file");
    }
    let name = if routing {
        "routing-build.tar.gz".to_owned()
    } else {
        let source_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .context("release product name is not UTF-8")?;
        format!("{index:03}-{source_name}")
    };
    std::fs::copy(source, destination.join(name))?;
    Ok(())
}

fn ingest_directory(
    analytics: &MapAnalytics,
    tenant_key: &str,
    directory: &Path,
    release: &DatasetRelease,
    source: &RegisteredSource,
) -> Result<()> {
    let mut paths = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in &paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let context = SourceIngestContext {
            analytics,
            tenant_key,
            release,
            source,
            source_element_hint: source_element_hint(name),
        };
        if name.ends_with(".geojson")
            || (name.ends_with(".json") && !name.ends_with(".raster.json"))
        {
            let geojson: GeoJson = std::fs::read_to_string(path)?.parse()?;
            ingest_geojson(&context, geojson, name, 0)?;
        } else if name.ends_with(".geojsonseq") || name.ends_with(".geojsonl") {
            for (record_index, line) in BufReader::new(File::open(path)?).lines().enumerate() {
                let line = line?;
                let line = line.trim().trim_start_matches('\u{1e}');
                if !line.is_empty() {
                    ingest_geojson(&context, line.parse()?, name, record_index)?;
                }
            }
        }
    }
    ingest_raster_products(analytics, tenant_key, &paths, release, source)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RasterMetadataSidecar {
    schema_version: u64,
    source_file: String,
    checksum_sha256: String,
    crs: String,
    transform: [f64; 6],
    width: u32,
    height: u32,
    extent: [f64; 4],
    resolution: [f64; 2],
    bands: Vec<RasterBand>,
}

fn ingest_raster_products(
    analytics: &MapAnalytics,
    tenant_key: &str,
    paths: &[PathBuf],
    release: &DatasetRelease,
    source: &RegisteredSource,
) -> Result<()> {
    for metadata_path in paths.iter().filter(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".raster.json"))
    }) {
        let metadata: RasterMetadataSidecar =
            serde_json::from_slice(&std::fs::read(metadata_path)?)?;
        if metadata.schema_version != RASTER_PRODUCT_SCHEMA_VERSION {
            bail!("raster metadata uses an unsupported schema version");
        }
        if Path::new(&metadata.source_file)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(metadata.source_file.as_str())
        {
            bail!("raster metadata source filename is not canonical");
        }
        let raster_path = paths
            .iter()
            .find(|path| {
                staged_original_filename(path).is_some_and(|name| name == metadata.source_file)
            })
            .context("raster metadata source file is absent from the release")?;
        if sha256_file(raster_path)? != metadata.checksum_sha256 {
            bail!("raster metadata checksum does not match its immutable product");
        }
        let product_index = staged_product_index(raster_path)?;
        let artifact_uri = release
            .normalized_artifact_uris
            .get(product_index)
            .context("raster product has no corresponding immutable artifact")?
            .clone();
        let raster = RasterProduct {
            schema_version: metadata.schema_version,
            raster_id: RasterProductId::from_stable_key(
                format!("{}:{}", release.release_id, metadata.checksum_sha256).as_bytes(),
            ),
            source_id: source.source_id.clone(),
            release_id: release.release_id.clone(),
            artifact_uri,
            checksum_sha256: metadata.checksum_sha256,
            crs: metadata.crs,
            transform: metadata.transform,
            width: metadata.width,
            height: metadata.height,
            extent: metadata.extent,
            resolution: metadata.resolution,
            bands: metadata.bands,
            license: release.license.clone(),
            attribution: release.license.attribution.clone(),
        };
        analytics.put_raster_product(tenant_key, &raster)?;
    }
    Ok(())
}

fn staged_product_index(path: &Path) -> Result<usize> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("staged product filename is invalid")?;
    let (index, _) = name
        .split_once('-')
        .context("staged product has no ordinal prefix")?;
    index.parse().context("staged product ordinal is invalid")
}

fn staged_original_filename(path: &Path) -> Option<&str> {
    path.file_name()?
        .to_str()?
        .split_once('-')
        .map(|(_, name)| name)
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[derive(Clone, Copy)]
struct SourceIngestContext<'a> {
    analytics: &'a MapAnalytics,
    tenant_key: &'a str,
    release: &'a DatasetRelease,
    source: &'a RegisteredSource,
    source_element_hint: Option<SourceElementType>,
}

fn ingest_geojson(
    context: &SourceIngestContext<'_>,
    geojson: GeoJson,
    product_name: &str,
    record_index: usize,
) -> Result<()> {
    match geojson {
        GeoJson::FeatureCollection(collection) => {
            for (index, feature) in collection.features.into_iter().enumerate() {
                ingest_feature(
                    context,
                    feature,
                    &format!("{product_name}:{record_index}:{index}"),
                )?;
            }
        }
        GeoJson::Feature(feature) => ingest_feature(
            context,
            feature,
            &format!("{product_name}:{record_index}:0"),
        )?,
        GeoJson::Geometry(geometry) => ingest_feature(
            context,
            Feature {
                bbox: None,
                geometry: Some(geometry),
                id: None,
                properties: None,
                foreign_members: None,
            },
            &format!("{product_name}:{record_index}:0"),
        )?,
    }
    Ok(())
}

fn ingest_feature(
    context: &SourceIngestContext<'_>,
    mut feature: Feature,
    fallback_source_element_id: &str,
) -> Result<()> {
    let Some(geometry) = feature.geometry.as_ref() else {
        return Ok(());
    };
    let geometry_parts = source_geometry_parts(geometry)?;
    let normalized_tags = normalized_source_tags(&mut feature)?;
    for (source_geometry_path, geometry) in geometry_parts {
        let mut part = feature.clone();
        part.geometry = Some(geometry);
        ingest_feature_part(
            context,
            part,
            fallback_source_element_id,
            &source_geometry_path,
            &normalized_tags,
        )?;
    }
    Ok(())
}

fn ingest_feature_part(
    context: &SourceIngestContext<'_>,
    feature: Feature,
    fallback_source_element_id: &str,
    source_geometry_path: &[u32],
    normalized_tags: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<()> {
    let SourceIngestContext {
        analytics,
        tenant_key,
        release,
        source,
        ..
    } = *context;
    let Some(geometry) = feature.geometry.as_ref() else {
        return Ok(());
    };
    let name = property_string(&feature, "name")
        .or_else(|| property_string(&feature, "ref"))
        .unwrap_or_else(|| format!("feature {fallback_source_element_id}"));
    let source_element_id = feature
        .id
        .as_ref()
        .map(|id| match id {
            geojson::feature::Id::String(value) => value.clone(),
            geojson::feature::Id::Number(value) => value.to_string(),
        })
        .or_else(|| property_identity(&feature))
        .unwrap_or_else(|| fallback_source_element_id.to_owned());
    let source_feature_id = ingest_complete_source_feature(
        context,
        &feature,
        &source_element_id,
        source_geometry_path,
        normalized_tags,
    )?;
    let lineage = SourceLineage {
        release_id: release.release_id.clone(),
        source_feature_id: source_feature_id.to_string(),
        authority: source.authority,
        valid_from: release.valid_from,
        valid_until: release.valid_until,
    };
    let stable_key = |kind: &str, part: usize| format!("{kind}:{source_feature_id}:{part}");
    match &geometry.value {
        GeometryValue::Point {
            coordinates: position,
        } => {
            let position = position_from_slice(position.as_slice())?;
            if let Some(kind) = property_string(&feature, "facility_kind")
                .and_then(|kind| serde_json::from_value(serde_json::Value::String(kind)).ok())
            {
                analytics.put_facility(
                    tenant_key,
                    &Facility {
                        facility_id: FacilityId::from_stable_key(
                            stable_key("facility", 0).as_bytes(),
                        ),
                        name,
                        kind,
                        position,
                        supported_mobility_families: property_enum_set(
                            &feature,
                            "supported_mobility_families",
                        ),
                        transfer_map_families: property_enum_set(&feature, "transfer_map_families"),
                        operating_intervals: Vec::new(),
                        capabilities: property_string_set(&feature, "capabilities"),
                        lineage,
                    },
                )?;
            } else if property_string(&feature, "name").is_some() {
                analytics.put_location(
                    tenant_key,
                    &MapLocation {
                        location_id: LocationId::from_stable_key(
                            stable_key("location", 0).as_bytes(),
                        ),
                        name,
                        position,
                        alternate_names: Default::default(),
                        lineage,
                    },
                )?;
            }
        }
        GeometryValue::Polygon { coordinates: rings } => analytics.put_boundary(
            tenant_key,
            &MapBoundary {
                boundary_id: MapBoundaryId::from_stable_key(stable_key("boundary", 0).as_bytes()),
                name,
                boundary_kind: property_string(&feature, "boundary_kind")
                    .unwrap_or_else(|| "administrative".to_owned()),
                geometry: polygon_from_coordinates(rings)?,
                lineage,
            },
        )?,
        GeometryValue::MultiPolygon {
            coordinates: polygons,
        } => {
            for (part, polygon) in polygons.iter().enumerate() {
                analytics.put_boundary(
                    tenant_key,
                    &MapBoundary {
                        boundary_id: MapBoundaryId::from_stable_key(
                            stable_key("boundary", part).as_bytes(),
                        ),
                        name: format!("{name} part {}", part + 1),
                        boundary_kind: property_string(&feature, "boundary_kind")
                            .unwrap_or_else(|| "administrative".to_owned()),
                        geometry: polygon_from_coordinates(polygon)?,
                        lineage: lineage.clone(),
                    },
                )?;
            }
        }
        GeometryValue::LineString { coordinates } => {
            let Some(from_node) = property_string(&feature, "from_node") else {
                return Ok(());
            };
            let Some(to_node) = property_string(&feature, "to_node") else {
                return Ok(());
            };
            let Some(map_family) = property_string(&feature, "map_family").and_then(|value| {
                serde_json::from_value::<MapFamily>(serde_json::Value::String(value)).ok()
            }) else {
                return Ok(());
            };
            let Some(duration) = property_f64(&feature, "nominal_duration_s") else {
                return Ok(());
            };
            let geometry = Wgs84LineString {
                coordinates: coordinates
                    .iter()
                    .map(|value| position_from_slice(value.as_slice()))
                    .collect::<Result<_>>()?,
            };
            geometry.validate()?;
            let distance = property_f64(&feature, "distance_m")
                .unwrap_or_else(|| approximate_length_m(&geometry));
            analytics.put_network_edge(
                tenant_key,
                &NetworkEdge {
                    edge_id: format!(
                        "{}:{}:{}",
                        source.source_id, release.release_id, source_feature_id
                    ),
                    map_family,
                    from_node,
                    to_node,
                    geometry,
                    distance_m: distance,
                    nominal_duration_s: duration,
                    bidirectional: property_bool(&feature, "bidirectional").unwrap_or(false),
                    source_release_id: release.release_id.clone(),
                },
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn ingest_complete_source_feature(
    context: &SourceIngestContext<'_>,
    feature: &Feature,
    source_element_id: &str,
    source_geometry_path: &[u32],
    normalized_tags: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<SourceFeatureId> {
    let SourceIngestContext {
        analytics,
        tenant_key,
        release,
        source,
        source_element_hint,
    } = *context;
    let geometry = feature
        .geometry
        .as_ref()
        .context("source feature part has no geometry")?;
    let geometry = feature_geometry(&geometry.value)?;
    let source_element_type = source_element_type(feature, source_element_hint);
    let representation = if source_element_type == SourceElementType::Relation {
        SourceFeatureRepresentation::Relation
    } else {
        representation_for_geometry(&geometry)
    };
    let source_element_version = ["source_version", "osm_version", "@version", "version"]
        .into_iter()
        .find_map(|key| {
            feature.property(key).and_then(|value| match value {
                serde_json::Value::String(value) => Some(value.clone()),
                serde_json::Value::Number(value) => Some(value.to_string()),
                _ => None,
            })
        })
        .unwrap_or_else(|| release.version_label.clone());
    let original_names = normalized_tags
        .iter()
        .filter(|(key, value)| {
            (*key == "name" || key.starts_with("name:")) && value.as_str().is_some()
        })
        .map(|(key, value)| {
            (
                key.strip_prefix("name:").unwrap_or("und").to_owned(),
                value.as_str().expect("filtered string").to_owned(),
            )
        })
        .collect();
    let original_references = normalized_tags
        .iter()
        .filter(|(key, value)| {
            (*key == "ref" || key.starts_with("ref:")) && value.as_str().is_some()
        })
        .map(|(_, value)| value.as_str().expect("filtered string").to_owned())
        .collect();
    let operating_area_ids = ["operating_area_id", "operating_area_ids"]
        .into_iter()
        .filter_map(|key| normalized_tags.get(key))
        .flat_map(|value| match value {
            serde_json::Value::String(value) => vec![value.clone()],
            serde_json::Value::Array(values) => values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let geometry_bytes = geometry.to_geojson_string()?.into_bytes();
    let stable_key = serde_json::to_vec(&(
        source.source_id.as_str(),
        source_element_type,
        source_element_id,
        source_geometry_path,
    ))?;
    let feature_id = SourceFeatureId::from_stable_key(&stable_key);
    analytics.put_source_feature(
        tenant_key,
        &SourceFeature {
            schema_version: SOURCE_FEATURE_SCHEMA_VERSION,
            feature_id: feature_id.clone(),
            source_id: source.source_id.clone(),
            release_id: release.release_id.clone(),
            source_element_type,
            source_element_id: source_element_id.to_owned(),
            source_element_version,
            representation,
            source_geometry_path: source_geometry_path.to_vec(),
            geometry,
            geometry_digest_sha256: hex::encode(Sha256::digest(geometry_bytes)),
            normalized_tags: normalized_tags.clone(),
            original_names,
            original_references,
            operating_area_ids,
            source_digest_sha256: release.source_digest_sha256.clone(),
            license: release.license.clone(),
            acquired_at: release.acquired_at,
        },
    )?;
    Ok(feature_id)
}

fn source_element_type(
    feature: &Feature,
    source_element_hint: Option<SourceElementType>,
) -> SourceElementType {
    if feature
        .property("osm_way_id")
        .is_some_and(|value| !value.is_null())
    {
        return SourceElementType::Way;
    }
    let explicit = ["source_element_type", "osm_type", "@type"]
        .into_iter()
        .find_map(|key| feature.property(key).and_then(serde_json::Value::as_str));
    match explicit {
        Some("node") => SourceElementType::Node,
        Some("way") => SourceElementType::Way,
        Some("relation") => SourceElementType::Relation,
        _ => source_element_hint.unwrap_or(SourceElementType::Feature),
    }
}

fn source_element_hint(name: &str) -> Option<SourceElementType> {
    if name.contains("other_relations")
        || name.contains("multilinestrings")
        || name.contains("multipolygons")
    {
        Some(SourceElementType::Relation)
    } else if name.contains("points") {
        Some(SourceElementType::Node)
    } else if name.contains("lines") {
        Some(SourceElementType::Way)
    } else {
        None
    }
}

fn source_geometry_parts(geometry: &Geometry) -> Result<Vec<(Vec<u32>, Geometry)>> {
    fn collect(
        geometry: &Geometry,
        path: &mut Vec<u32>,
        parts: &mut Vec<(Vec<u32>, Geometry)>,
    ) -> Result<()> {
        if path.len() > MAX_SOURCE_GEOMETRY_COLLECTION_DEPTH {
            bail!("source GeometryCollection exceeds its governed depth");
        }
        match &geometry.value {
            GeometryValue::GeometryCollection { geometries } => {
                if geometries.is_empty() {
                    bail!("source GeometryCollection is empty");
                }
                for (index, child) in geometries.iter().enumerate() {
                    if parts.len() >= MAX_SOURCE_GEOMETRY_PARTS {
                        bail!("source GeometryCollection exceeds its governed part limit");
                    }
                    path.push(u32::try_from(index).context("geometry part index exceeds u32")?);
                    collect(child, path, parts)?;
                    path.pop();
                }
            }
            _ => parts.push((path.clone(), geometry.clone())),
        }
        Ok(())
    }

    let mut parts = Vec::new();
    collect(geometry, &mut Vec::new(), &mut parts)?;
    Ok(parts)
}

fn normalized_source_tags(
    feature: &mut Feature,
) -> Result<std::collections::BTreeMap<String, serde_json::Value>> {
    const SOURCE_METADATA_FIELDS: &[&str] = &[
        "osm_id",
        "osm_way_id",
        "osm_version",
        "osm_timestamp",
        "osm_uid",
        "osm_user",
        "osm_changeset",
        "z_order",
    ];

    let Some(properties) = feature.properties.as_mut() else {
        return Ok(Default::default());
    };
    let mut embedded = std::collections::BTreeMap::new();
    let mut complete_tags = None;
    for carrier in ["all_tags", "other_tags"] {
        let Some(value) = properties.remove(carrier) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let value = match value {
            serde_json::Value::String(value) => serde_json::from_str(&value)
                .with_context(|| format!("{carrier} is not a JSON object"))?,
            value => value,
        };
        let serde_json::Value::Object(values) = value else {
            bail!("{carrier} is not a JSON object");
        };
        let values = values
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        if carrier == "all_tags" {
            complete_tags = Some(values.clone());
        }
        embedded.extend(values);
    }
    for (key, value) in &embedded {
        if properties.get(key).is_none_or(serde_json::Value::is_null) {
            properties.insert(key.clone(), value.clone());
        }
    }
    if let Some(tags) = complete_tags {
        return Ok(tags);
    }
    let mut tags = embedded;
    for (key, value) in properties.iter() {
        if !SOURCE_METADATA_FIELDS.contains(&key.as_str()) {
            tags.insert(key.clone(), value.clone());
        }
    }
    Ok(tags)
}

fn property_identity(feature: &Feature) -> Option<String> {
    ["osm_id", "osm_way_id", "source_id", "id"]
        .into_iter()
        .find_map(|key| {
            feature.property(key).and_then(|value| match value {
                serde_json::Value::String(value) => Some(value.clone()),
                serde_json::Value::Number(value) => Some(value.to_string()),
                _ => None,
            })
        })
}

fn feature_geometry(value: &GeometryValue) -> Result<FeatureGeometry> {
    let position = |value: &geojson::Position| -> Result<GeoJsonPosition> {
        if value.len() < 2 {
            bail!("GeoJSON position has fewer than two ordinates");
        }
        let position = GeoJsonPosition::new(value[0], value[1], value.as_slice().get(2).copied());
        position.validate()?;
        Ok(position)
    };
    let positions = |values: &[geojson::Position]| -> Result<Vec<GeoJsonPosition>> {
        values.iter().map(position).collect()
    };
    let geometry = match value {
        GeometryValue::Point { coordinates } => FeatureGeometry::Point(position(coordinates)?),
        GeometryValue::MultiPoint { coordinates } => {
            FeatureGeometry::MultiPoint(positions(coordinates)?)
        }
        GeometryValue::LineString { coordinates } => {
            FeatureGeometry::LineString(positions(coordinates)?)
        }
        GeometryValue::MultiLineString { coordinates } => FeatureGeometry::MultiLineString(
            coordinates
                .iter()
                .map(|line| positions(line))
                .collect::<Result<_>>()?,
        ),
        GeometryValue::Polygon { coordinates } => FeatureGeometry::Polygon(
            coordinates
                .iter()
                .map(|ring| positions(ring))
                .collect::<Result<_>>()?,
        ),
        GeometryValue::MultiPolygon { coordinates } => FeatureGeometry::MultiPolygon(
            coordinates
                .iter()
                .map(|polygon| {
                    polygon
                        .iter()
                        .map(|ring| positions(ring))
                        .collect::<Result<_>>()
                })
                .collect::<Result<_>>()?,
        ),
        GeometryValue::GeometryCollection { .. } => {
            bail!("source GeometryCollections must be normalized into individual features")
        }
    };
    geometry.validate()?;
    Ok(geometry)
}

fn position_from_slice(value: &[f64]) -> Result<Wgs84Position> {
    if value.len() < 2 {
        bail!("GeoJSON position has fewer than two ordinates");
    }
    Ok(Wgs84Position::new(
        value[0],
        value[1],
        value.get(2).copied(),
    )?)
}

fn polygon_from_coordinates(value: &[Vec<geojson::Position>]) -> Result<Wgs84Polygon> {
    let mut rings = value.iter().map(|ring| {
        ring.iter()
            .map(|position| position_from_slice(position.as_slice()))
            .collect::<Result<Vec<_>>>()
    });
    let polygon = Wgs84Polygon {
        exterior: rings
            .next()
            .context("GeoJSON polygon omitted its exterior")??,
        interiors: rings.collect::<Result<Vec<_>>>()?,
    };
    polygon.validate()?;
    Ok(polygon)
}

fn property_string(feature: &Feature, name: &str) -> Option<String> {
    feature
        .property(name)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}
fn property_f64(feature: &Feature, name: &str) -> Option<f64> {
    feature
        .property(name)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
}
fn property_bool(feature: &Feature, name: &str) -> Option<bool> {
    feature.property(name).and_then(serde_json::Value::as_bool)
}

fn property_enum_set<T>(feature: &Feature, name: &str) -> std::collections::BTreeSet<T>
where
    T: Ord + serde::de::DeserializeOwned,
{
    feature
        .property(name)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value(value.clone()).ok())
        .collect()
}

fn property_string_set(feature: &Feature, name: &str) -> std::collections::BTreeSet<String> {
    feature
        .property(name)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .map(ToOwned::to_owned)
        .collect()
}

fn approximate_length_m(line: &Wgs84LineString) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_008.8;
    line.coordinates
        .windows(2)
        .map(|pair| {
            let lat1 = pair[0].latitude_deg.to_radians();
            let lat2 = pair[1].latitude_deg.to_radians();
            let dlat = lat2 - lat1;
            let dlon = (pair[1].longitude_deg - pair[0].longitude_deg).to_radians();
            let a =
                (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
            2.0 * EARTH_RADIUS_M * a.sqrt().asin()
        })
        .sum()
}

fn extract_routing_build(
    archive_path: &Path,
    destination: &Path,
    maximum_expanded_bytes: u64,
) -> Result<()> {
    const MAXIMUM_ENTRIES: u64 = 5_000_000;
    let parent = destination
        .parent()
        .context("routing-build destination has no parent")?;
    let temporary = parent.join(format!(".routing-tiles.{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&temporary)?;
    let result = (|| -> Result<()> {
        let decoder = GzDecoder::new(File::open(archive_path)?);
        let mut archive = tar::Archive::new(decoder);
        let mut entry_count = 0_u64;
        let mut expanded_bytes = 0_u64;
        for entry in archive.entries()? {
            let mut entry = entry?;
            entry_count = entry_count.saturating_add(1);
            expanded_bytes = expanded_bytes.saturating_add(entry.header().size()?);
            if entry_count > MAXIMUM_ENTRIES || expanded_bytes > maximum_expanded_bytes {
                bail!("routing build exceeds its expansion limits");
            }
            let path = entry.path()?;
            if path.is_absolute()
                || path.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
                || !(entry.header().entry_type().is_file() || entry.header().entry_type().is_dir())
            {
                bail!("routing build contains an unsafe archive entry");
            }
            entry.unpack_in(&temporary)?;
        }
        std::fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temporary);
    }
    result
}

#[cfg(unix)]
fn activate_routing_build(routing_tiles: &Path, active_path: &Path) -> Result<()> {
    if !routing_tiles.is_dir() {
        bail!("prepared routing tiles are unavailable");
    }
    let parent = active_path
        .parent()
        .context("Valhalla active path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let replacement = parent.join(format!(".active-link.{}", uuid::Uuid::now_v7()));
    std::os::unix::fs::symlink(routing_tiles, &replacement)?;

    let metadata = std::fs::symlink_metadata(active_path).ok();
    let initial_directory = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_dir());
    let backup = parent.join(format!(".active-initial.{}", uuid::Uuid::now_v7()));
    if initial_directory {
        std::fs::rename(active_path, &backup)?;
    }
    if let Err(error) = std::fs::rename(&replacement, active_path) {
        if initial_directory {
            let _ = std::fs::rename(&backup, active_path);
        }
        let _ = std::fs::remove_file(&replacement);
        return Err(error.into());
    }
    if initial_directory {
        std::fs::remove_dir_all(backup)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn activate_routing_build(_routing_tiles: &Path, _active_path: &Path) -> Result<()> {
    bail!("Valhalla routing activation requires Unix symlink semantics")
}

#[cfg(test)]
mod tests {
    use flate2::{Compression, write::GzEncoder};
    use tempfile::TempDir;

    use super::*;

    fn routing_archive(path: &Path, payload_bytes: usize) {
        let encoder = GzEncoder::new(File::create(path).unwrap(), Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        let bytes = vec![b'x'; payload_bytes];
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o444);
        header.set_cksum();
        archive
            .append_data(&mut header, "2/000/000.gph", bytes.as_slice())
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    fn routing_build_is_bounded_before_activation() {
        let root = TempDir::new().unwrap();
        let archive = root.path().join("tiles.tar.gz");
        routing_archive(&archive, 64);
        let destination = root.path().join("tiles");
        let error = extract_routing_build(&archive, &destination, 32)
            .unwrap_err()
            .to_string();
        assert!(error.contains("expansion limits"));
        assert!(!destination.exists());
    }

    #[cfg(unix)]
    #[test]
    fn activation_switches_to_cached_release_tiles() {
        let root = TempDir::new().unwrap();
        let archive = root.path().join("tiles.tar.gz");
        routing_archive(&archive, 64);
        let cached = root.path().join("release-a/routing-tiles");
        std::fs::create_dir(root.path().join("release-a")).unwrap();
        extract_routing_build(&archive, &cached, 1024).unwrap();
        let active = root.path().join("valhalla/active");
        std::fs::create_dir_all(&active).unwrap();

        activate_routing_build(&cached, &active).unwrap();

        assert!(
            std::fs::symlink_metadata(&active)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::canonicalize(&active).unwrap(), cached);
        assert_eq!(
            std::fs::read(active.join("2/000/000.gph")).unwrap(),
            vec![b'x'; 64]
        );
    }

    #[test]
    fn stable_feature_ids_do_not_change_across_activation() {
        let key = b"source:test-feature:location";
        assert_eq!(
            LocationId::from_stable_key(key),
            LocationId::from_stable_key(key)
        );
        assert_eq!(
            SourceFeatureId::from_stable_key(key),
            SourceFeatureId::from_stable_key(key)
        );
    }

    #[test]
    fn relation_geometry_collections_keep_deterministic_leaf_paths() {
        let point = |longitude| {
            Geometry::new(GeometryValue::Point {
                coordinates: vec![longitude, 13.7].into(),
            })
        };
        let nested = Geometry::new(GeometryValue::GeometryCollection {
            geometries: vec![point(-89.2), point(-89.1)],
        });
        let collection = Geometry::new(GeometryValue::GeometryCollection {
            geometries: vec![point(-89.3), nested],
        });

        let parts = source_geometry_parts(&collection).unwrap();

        assert_eq!(
            parts
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>(),
            vec![vec![0], vec![1, 0], vec![1, 1]]
        );
    }

    #[test]
    fn osm_all_tags_are_complete_and_source_metadata_stays_separate() {
        let mut properties = serde_json::Map::new();
        properties.insert("osm_id".to_owned(), serde_json::json!("42"));
        properties.insert("osm_version".to_owned(), serde_json::json!("7"));
        properties.insert(
            "all_tags".to_owned(),
            serde_json::json!(r#"{"name":"Original","name:es":"Original ES","ref":"A-1"}"#),
        );
        let mut feature = Feature {
            bbox: None,
            geometry: None,
            id: None,
            properties: Some(properties),
            foreign_members: None,
        };

        let tags = normalized_source_tags(&mut feature).unwrap();

        assert_eq!(tags.len(), 3);
        assert_eq!(tags.get("name"), Some(&serde_json::json!("Original")));
        assert!(!tags.contains_key("osm_version"));
        assert_eq!(
            feature.property("name"),
            Some(&serde_json::json!("Original"))
        );
    }

    #[test]
    fn osm_multipolygon_ways_override_the_relation_layer_hint() {
        let mut properties = serde_json::Map::new();
        properties.insert("osm_way_id".to_owned(), serde_json::json!("101"));
        let feature = Feature {
            bbox: None,
            geometry: None,
            id: None,
            properties: Some(properties),
            foreign_members: None,
        };

        assert_eq!(
            source_element_type(&feature, Some(SourceElementType::Relation)),
            SourceElementType::Way
        );
    }

    #[test]
    fn staged_raster_products_bind_an_exact_original_filename_and_digest() {
        let root = TempDir::new().unwrap();
        let raster = root.path().join("007-environmental-raster.tif");
        std::fs::write(&raster, b"immutable-raster").unwrap();
        assert_eq!(
            staged_original_filename(&raster),
            Some("environmental-raster.tif")
        );
        assert_eq!(
            sha256_file(&raster).unwrap(),
            hex::encode(Sha256::digest(b"immutable-raster"))
        );
    }
}
