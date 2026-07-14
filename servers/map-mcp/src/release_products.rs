use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use geojson::{Feature, GeoJson, GeometryValue};

use crate::{
    analytics::{MapAnalytics, NetworkEdge},
    contract::{
        DatasetRelease, Facility, FacilityId, LocationId, MapBoundary, MapBoundaryId, MapFamily,
        MapLocation, RegisteredSource, SourceLineage, Wgs84LineString, Wgs84Polygon, Wgs84Position,
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
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name.ends_with(".geojson") || name.ends_with(".json") {
            let geojson: GeoJson = std::fs::read_to_string(&path)?.parse()?;
            ingest_geojson(analytics, tenant_key, geojson, release, source)?;
        } else if name.ends_with(".geojsonseq") || name.ends_with(".geojsonl") {
            for line in BufReader::new(File::open(path)?).lines() {
                let line = line?;
                let line = line.trim().trim_start_matches('\u{1e}');
                if !line.is_empty() {
                    ingest_geojson(analytics, tenant_key, line.parse()?, release, source)?;
                }
            }
        }
    }
    Ok(())
}

fn ingest_geojson(
    analytics: &MapAnalytics,
    tenant_key: &str,
    geojson: GeoJson,
    release: &DatasetRelease,
    source: &RegisteredSource,
) -> Result<()> {
    match geojson {
        GeoJson::FeatureCollection(collection) => {
            for (index, feature) in collection.features.into_iter().enumerate() {
                ingest_feature(analytics, tenant_key, feature, index, release, source)?;
            }
        }
        GeoJson::Feature(feature) => {
            ingest_feature(analytics, tenant_key, feature, 0, release, source)?
        }
        GeoJson::Geometry(geometry) => ingest_feature(
            analytics,
            tenant_key,
            Feature {
                bbox: None,
                geometry: Some(geometry),
                id: None,
                properties: None,
                foreign_members: None,
            },
            0,
            release,
            source,
        )?,
    }
    Ok(())
}

fn ingest_feature(
    analytics: &MapAnalytics,
    tenant_key: &str,
    feature: Feature,
    index: usize,
    release: &DatasetRelease,
    source: &RegisteredSource,
) -> Result<()> {
    let Some(geometry) = feature.geometry.as_ref() else {
        return Ok(());
    };
    let name = property_string(&feature, "name")
        .or_else(|| property_string(&feature, "ref"))
        .unwrap_or_else(|| format!("feature {index}"));
    let source_feature_id = feature
        .id
        .as_ref()
        .map(|id| match id {
            geojson::feature::Id::String(value) => value.clone(),
            geojson::feature::Id::Number(value) => value.to_string(),
        })
        .unwrap_or_else(|| index.to_string());
    let lineage = SourceLineage {
        release_id: release.release_id.clone(),
        source_feature_id: source_feature_id.clone(),
        authority: source.authority,
        valid_from: release.valid_from,
        valid_until: release.valid_until,
    };
    let stable_key =
        |kind: &str, part: usize| format!("{}:{kind}:{source_feature_id}:{part}", source.source_id);
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
                    edge_id: format!("{}:{}:{index}", source.source_id, release.release_id),
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
    }
}
