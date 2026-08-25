use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use duckdb::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_duckdb_runtime::{
    EngineSettings, FileAccess, SharedDatabase, SpatialAxisPolicy, TrustedExtension,
    verify_spatial_axis_policy,
};
use veoveo_mcp_contract::WorkContextId;

use crate::contract::{
    Facility, MapBoundaryId, MapFamily, MapLocation, Meters, NearbyFacility, NearbyLocation,
    QuerySourceFeaturesOutput, QuerySourceFeaturesRequest, RasterDerivation, RasterDerivationId,
    RasterProduct, RasterProductId, SearchLocationsOutput, SearchLocationsRequest, SourceFeature,
    SourceFeatureId, SourceFeatureMatch, SourceSpatialQuery, SpatialDerivation,
    SpatialDerivationId, Wgs84BoundingBox, Wgs84LineString, Wgs84Position,
};

mod projection;

pub(crate) use projection::ReleaseProjectionWriter;

const SCHEMA_VERSION: i64 = 10;
const REBUILD_SPATIAL_INDEXES_FROM_SCHEMA_VERSION: i64 = 9;

#[derive(Clone, Copy)]
struct SpatialIndexDefinition {
    name: &'static str,
    table: &'static str,
    column: &'static str,
}

const SPATIAL_INDEXES: [SpatialIndexDefinition; 4] = [
    SpatialIndexDefinition {
        name: "map_boundary_geometry",
        table: "map_boundary",
        column: "geometry",
    },
    SpatialIndexDefinition {
        name: "map_source_feature_geometry",
        table: "map_source_feature",
        column: "geometry",
    },
    SpatialIndexDefinition {
        name: "map_authored_revision_geometry",
        table: "map_authored_feature_revision",
        column: "geometry",
    },
    SpatialIndexDefinition {
        name: "map_authored_head_geometry",
        table: "map_authored_feature_head",
        column: "geometry",
    },
];
const NEARBY_FACILITIES_SQL: &str = "WITH scored AS MATERIALIZED (\
       SELECT canonical_json, facility_key, source_release_key, \
       ST_Distance_Sphere(ST_Point2D(longitude_deg, latitude_deg), ST_Point2D(?, ?)) AS distance_m \
       FROM map_visible_facility \
       WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?)\
     ) \
     SELECT canonical_json, distance_m FROM scored WHERE distance_m <= ? \
     ORDER BY distance_m ASC, facility_key ASC, source_release_key ASC LIMIT ?";
const NEARBY_LOCATIONS_SQL: &str = "WITH scored AS MATERIALIZED (\
       SELECT canonical_json, location_key, source_release_key, \
       ST_Distance_Sphere(ST_Point2D(longitude_deg, latitude_deg), ST_Point2D(?, ?)) AS distance_m \
       FROM map_visible_location \
       WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?)\
     ) \
     SELECT canonical_json, distance_m FROM scored WHERE distance_m <= ? \
     ORDER BY distance_m ASC, location_key ASC, source_release_key ASC LIMIT ?";

#[derive(Clone, Debug)]
pub struct MapAnalyticsConfig {
    pub database_path: PathBuf,
    pub authoring_task_root: PathBuf,
    pub spill_dir: PathBuf,
    pub spatial_extension: PathBuf,
    pub memory_limit: String,
    pub threads: u32,
}

#[derive(Clone, Debug)]
pub struct MapAnalytics {
    database_path: PathBuf,
    authoring_task_root: PathBuf,
    instance: SharedDatabase,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetworkEdge {
    pub edge_id: String,
    pub map_family: MapFamily,
    pub from_node: String,
    pub to_node: String,
    pub geometry: crate::contract::Wgs84LineString,
    pub distance_m: f64,
    pub nominal_duration_s: f64,
    pub bidirectional: bool,
    pub source_release_id: crate::contract::DatasetReleaseId,
}

impl MapAnalytics {
    pub fn open(config: MapAnalyticsConfig) -> Result<Self> {
        if let Some(parent) = config.database_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating map database directory {}", parent.display()))?;
        }
        let mut settings = EngineSettings::new(config.spill_dir);
        settings.memory_limit = config.memory_limit;
        settings.threads = config.threads;
        settings
            .trusted_extensions
            .push(TrustedExtension::new("spatial", config.spatial_extension)?);
        settings.spatial_axis_policy = SpatialAxisPolicy::GeoJsonLongitudeLatitude;
        if !config.authoring_task_root.is_absolute() {
            bail!("authoring task root must be absolute");
        }
        std::fs::create_dir_all(&config.authoring_task_root).with_context(|| {
            format!(
                "creating authoring task root {}",
                config.authoring_task_root.display()
            )
        })?;
        let authoring_task_root = config.authoring_task_root.canonicalize()?;
        let instance = SharedDatabase::open(
            &config.database_path,
            &[],
            &FileAccess::ServiceRoot(authoring_task_root.clone()),
            &settings,
        )?;
        let analytics = Self {
            database_path: config.database_path,
            authoring_task_root,
            instance,
        };
        analytics.initialize()?;
        Ok(analytics)
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn verify_spatial(&self) -> Result<()> {
        let connection = self.connection()?;
        verify_spatial_axis_policy(&connection, SpatialAxisPolicy::GeoJsonLongitudeLatitude)?;
        let text: String = connection
            .query_row("SELECT ST_AsText(ST_Point(1, 2))", [], |row| row.get(0))
            .context("verifying DuckDB Spatial")?;
        if text != "POINT (1 2)" {
            bail!("DuckDB Spatial verification returned {text:?}");
        }
        Ok(())
    }

    pub fn search_locations(
        &self,
        tenant_key: &str,
        request: &SearchLocationsRequest,
    ) -> Result<SearchLocationsOutput> {
        request.coverage.validate()?;
        if request.query.trim().is_empty() || request.query.len() > 256 {
            bail!("location query must be non-empty and at most 256 bytes");
        }
        if !(1..=100).contains(&request.limit) {
            bail!("location search limit must be within 1..=100");
        }
        let connection = self.read_connection()?;
        let mut locations = Vec::new();
        let location_longitude_predicate = longitude_predicate(&request.coverage, "longitude_deg");
        let sql = format!(
            "SELECT canonical_json FROM map_visible_location WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND name ILIKE '%' || ? || '%' AND latitude_deg BETWEEN ? AND ? AND {location_longitude_predicate} ORDER BY name ASC, location_key ASC, source_release_key ASC LIMIT ?"
        );
        let mut statement = connection.prepare(&sql)?;
        let mut rows = if request.coverage.west <= request.coverage.east {
            statement.query(params![
                tenant_key,
                tenant_key,
                request.query.trim(),
                request.coverage.south,
                request.coverage.north,
                request.coverage.west,
                request.coverage.east,
                request.limit,
            ])?
        } else {
            statement.query(params![
                tenant_key,
                tenant_key,
                request.query.trim(),
                request.coverage.south,
                request.coverage.north,
                request.coverage.west,
                request.coverage.east,
                request.limit,
            ])?
        };
        while let Some(row) = rows.next()? {
            let json: String = row.get(0)?;
            locations.push(serde_json::from_str::<MapLocation>(&json)?);
        }

        let facilities = if request.include_facilities {
            let mut facilities = Vec::new();
            let facility_longitude_predicate =
                longitude_predicate(&request.coverage, "longitude_deg");
            let sql = format!(
                "SELECT canonical_json FROM map_visible_facility WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND name ILIKE '%' || ? || '%' AND latitude_deg BETWEEN ? AND ? AND {facility_longitude_predicate} ORDER BY name ASC, facility_key ASC, source_release_key ASC LIMIT ?"
            );
            let mut statement = connection.prepare(&sql)?;
            let mut rows = statement.query(params![
                tenant_key,
                tenant_key,
                request.query.trim(),
                request.coverage.south,
                request.coverage.north,
                request.coverage.west,
                request.coverage.east,
                request.limit,
            ])?;
            while let Some(row) = rows.next()? {
                let json: String = row.get(0)?;
                facilities.push(serde_json::from_str::<Facility>(&json)?);
            }
            facilities
        } else {
            Vec::new()
        };
        Ok(SearchLocationsOutput {
            locations,
            facilities,
        })
    }

    pub fn location(
        &self,
        tenant_key: &str,
        location_id: &crate::contract::LocationId,
    ) -> Result<Option<MapLocation>> {
        select_canonical(
            &self.read_connection()?,
            "map_visible_location",
            "location_key",
            location_id.as_str(),
            tenant_key,
        )
    }

    pub fn facility(
        &self,
        tenant_key: &str,
        facility_id: &crate::contract::FacilityId,
    ) -> Result<Option<Facility>> {
        select_canonical(
            &self.read_connection()?,
            "map_visible_facility",
            "facility_key",
            facility_id.as_str(),
            tenant_key,
        )
    }

    pub fn nearby_facilities(
        &self,
        tenant_key: &str,
        position: &Wgs84Position,
        radius: Meters,
        limit: u32,
    ) -> Result<Vec<Facility>> {
        position.validate()?;
        if radius.get() <= 0.0 || radius.get() > 1_000_000.0 {
            bail!("nearby facility radius must be within (0, 1000000] meters");
        }
        if !(1..=100).contains(&limit) {
            bail!("nearby facility limit must be within 1..=100");
        }
        Ok(self
            .nearby_facility_matches(tenant_key, position, radius, limit)?
            .into_iter()
            .map(|item| item.facility)
            .collect())
    }

    pub fn nearby_location_matches(
        &self,
        tenant_key: &str,
        position: &Wgs84Position,
        radius: Meters,
        limit: u32,
    ) -> Result<Vec<NearbyLocation>> {
        validate_nearby_query(position, radius, limit, "location")?;
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(NEARBY_LOCATIONS_SQL)?;
        let mut rows = statement.query(params![
            position.longitude_deg,
            position.latitude_deg,
            tenant_key,
            tenant_key,
            radius.get(),
            limit,
        ])?;
        let mut locations = Vec::new();
        while let Some(row) = rows.next()? {
            locations.push(NearbyLocation {
                location: serde_json::from_str(&row.get::<_, String>(0)?)?,
                distance: Meters::new(row.get(1)?)?,
            });
        }
        Ok(locations)
    }

    pub fn nearby_facility_matches(
        &self,
        tenant_key: &str,
        position: &Wgs84Position,
        radius: Meters,
        limit: u32,
    ) -> Result<Vec<NearbyFacility>> {
        validate_nearby_query(position, radius, limit, "facility")?;
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(NEARBY_FACILITIES_SQL)?;
        let mut rows = statement.query(params![
            position.longitude_deg,
            position.latitude_deg,
            tenant_key,
            tenant_key,
            radius.get(),
            limit,
        ])?;
        let mut facilities = Vec::new();
        while let Some(row) = rows.next()? {
            facilities.push(NearbyFacility {
                facility: serde_json::from_str(&row.get::<_, String>(0)?)?,
                distance: Meters::new(row.get(1)?)?,
            });
        }
        Ok(facilities)
    }

    pub fn active_release_ids(
        &self,
        tenant_key: &str,
    ) -> Result<Vec<crate::contract::DatasetReleaseId>> {
        active_release_keys(&self.read_connection()?, tenant_key)?
            .into_iter()
            .map(|release| release.parse().map_err(Into::into))
            .collect()
    }

    pub fn list_facilities(&self, tenant_key: &str, limit: u32) -> Result<Vec<Facility>> {
        if !(1..=10_000).contains(&limit) {
            bail!("facility list limit must be within 1..=10000");
        }
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare("SELECT canonical_json FROM map_visible_facility WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) ORDER BY name ASC, facility_key ASC, source_release_key ASC LIMIT ?")?;
        let mut rows = statement.query(params![tenant_key, tenant_key, limit])?;
        let mut facilities = Vec::new();
        while let Some(row) = rows.next()? {
            facilities.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(facilities)
    }

    pub fn list_locations(&self, tenant_key: &str, limit: u32) -> Result<Vec<MapLocation>> {
        if !(1..=10_000).contains(&limit) {
            bail!("location list limit must be within 1..=10000");
        }
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare("SELECT canonical_json FROM map_visible_location WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) ORDER BY name ASC, location_key ASC, source_release_key ASC LIMIT ?")?;
        let mut rows = statement.query(params![tenant_key, tenant_key, limit])?;
        let mut locations = Vec::new();
        while let Some(row) = rows.next()? {
            locations.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(locations)
    }

    pub fn source_feature(
        &self,
        tenant_key: &str,
        release_id: &crate::contract::DatasetReleaseId,
        feature_id: &SourceFeatureId,
    ) -> Result<Option<SourceFeature>> {
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_visible_source_feature WHERE tenant_key = ? AND release_key = ? AND feature_key = ? LIMIT 1",
        )?;
        let mut rows = statement.query(params![
            tenant_key,
            release_id.as_str(),
            feature_id.as_str()
        ])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?))
    }

    pub fn raster_product(
        &self,
        tenant_key: &str,
        raster_id: &RasterProductId,
    ) -> Result<Option<RasterProduct>> {
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_visible_raster_product WHERE tenant_key = ? AND raster_key = ? ORDER BY release_key ASC LIMIT 1",
        )?;
        let mut rows = statement.query(params![tenant_key, raster_id.as_str()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?))
    }

    pub fn list_raster_products(
        &self,
        tenant_key: &str,
        release_id: Option<&crate::contract::DatasetReleaseId>,
        limit: u32,
    ) -> Result<Vec<RasterProduct>> {
        if !(1..=10_000).contains(&limit) {
            bail!("raster product limit must be within 1..=10000");
        }
        let connection = self.read_connection()?;
        let sql = if release_id.is_some() {
            "SELECT canonical_json FROM map_visible_raster_product WHERE tenant_key = ? AND release_key = ? ORDER BY raster_key LIMIT ?"
        } else {
            "SELECT canonical_json FROM map_visible_raster_product WHERE tenant_key = ? ORDER BY release_key, raster_key LIMIT ?"
        };
        let mut statement = connection.prepare(sql)?;
        let mut rows = if let Some(release_id) = release_id {
            statement.query(params![tenant_key, release_id.as_str(), limit])?
        } else {
            statement.query(params![tenant_key, limit])?
        };
        let mut rasters = Vec::new();
        while let Some(row) = rows.next()? {
            rasters.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(rasters)
    }

    pub fn put_raster_derivation(
        &self,
        tenant_key: &str,
        derivation: &RasterDerivation,
    ) -> Result<()> {
        derivation.validate()?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR REPLACE INTO map_raster_derivation VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                tenant_key,
                derivation.work_context.as_str(),
                derivation.created_by.as_str(),
                derivation.derivation_id.as_str(),
                derivation.source_raster_id.as_str(),
                derivation.source_release_id.as_str(),
                serde_json::to_string(derivation)?,
            ],
        )?;
        Ok(())
    }

    pub fn raster_derivation(
        &self,
        tenant_key: &str,
        work_context: &WorkContextId,
        derivation_id: &RasterDerivationId,
    ) -> Result<Option<RasterDerivation>> {
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_raster_derivation WHERE tenant_key = ? AND work_context_key = ? AND derivation_key = ? LIMIT 1",
        )?;
        let mut rows = statement.query(params![
            tenant_key,
            work_context.as_str(),
            derivation_id.as_str()
        ])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?))
    }

    pub fn list_raster_derivations(
        &self,
        tenant_key: &str,
        work_context: &WorkContextId,
        limit: u32,
    ) -> Result<Vec<RasterDerivation>> {
        if !(1..=10_000).contains(&limit) {
            bail!("raster derivation limit must be within 1..=10000");
        }
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_raster_derivation WHERE tenant_key = ? AND work_context_key = ? ORDER BY derivation_key LIMIT ?",
        )?;
        let mut rows = statement.query(params![tenant_key, work_context.as_str(), limit])?;
        let mut derivations = Vec::new();
        while let Some(row) = rows.next()? {
            derivations.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(derivations)
    }

    pub fn put_spatial_derivation(
        &self,
        tenant_key: &str,
        derivation: &SpatialDerivation,
    ) -> Result<()> {
        derivation.validate()?;
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR REPLACE INTO map_spatial_derivation VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                tenant_key,
                derivation.work_context.as_str(),
                derivation.created_by.as_str(),
                derivation.derivation_id.as_str(),
                derivation.mobility_profile_id.as_str(),
                derivation.mobility_profile_version,
                serde_json::to_string(derivation)?,
            ],
        )?;
        Ok(())
    }

    pub fn spatial_derivation(
        &self,
        tenant_key: &str,
        work_context: &WorkContextId,
        derivation_id: &SpatialDerivationId,
    ) -> Result<Option<SpatialDerivation>> {
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_spatial_derivation WHERE tenant_key = ? AND work_context_key = ? AND derivation_key = ? LIMIT 1",
        )?;
        let mut rows = statement.query(params![
            tenant_key,
            work_context.as_str(),
            derivation_id.as_str()
        ])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?)?))
    }

    pub fn list_spatial_derivations(
        &self,
        tenant_key: &str,
        work_context: &WorkContextId,
        limit: u32,
    ) -> Result<Vec<SpatialDerivation>> {
        if !(1..=10_000).contains(&limit) {
            bail!("spatial derivation limit must be within 1..=10000");
        }
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_spatial_derivation WHERE tenant_key = ? AND work_context_key = ? ORDER BY derivation_key LIMIT ?",
        )?;
        let mut rows = statement.query(params![tenant_key, work_context.as_str(), limit])?;
        let mut derivations = Vec::new();
        while let Some(row) = rows.next()? {
            derivations.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(derivations)
    }

    pub fn query_source_features(
        &self,
        tenant_key: &str,
        request: &QuerySourceFeaturesRequest,
    ) -> Result<QuerySourceFeaturesOutput> {
        request.validate()?;
        let query_digest = source_query_digest(request)?;
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_source_cursor)
            .transpose()?;
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.query_digest_sha256 != query_digest)
        {
            bail!("source-feature cursor belongs to a different query");
        }
        let sql = source_feature_query_sql(tenant_key, request, cursor.as_ref())?;
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query([])?;
        let mut features = Vec::new();
        while let Some(row) = rows.next()? {
            let feature: SourceFeature = serde_json::from_str(&row.get::<_, String>(0)?)?;
            let distance = row.get::<_, Option<f64>>(1)?.map(Meters::new).transpose()?;
            features.push(SourceFeatureMatch { feature, distance });
        }
        let has_more = features.len() > request.limit as usize;
        features.truncate(request.limit as usize);
        let next_cursor = if has_more {
            features.last().map(|item| {
                encode_source_cursor(&SourceFeatureCursor {
                    query_domain: SOURCE_FEATURE_QUERY_DOMAIN.to_owned(),
                    query_digest_sha256: query_digest.clone(),
                    distance_m: item.distance.map(Meters::get),
                    feature_id: item.feature.feature_id.to_string(),
                })
            })
        } else {
            None
        }
        .transpose()?;
        Ok(QuerySourceFeaturesOutput {
            release_id: request.release_id.clone(),
            query_digest_sha256: query_digest,
            features,
            next_cursor,
        })
    }

    pub fn containing_boundary_ids(
        &self,
        tenant_key: &str,
        position: &Wgs84Position,
    ) -> Result<Vec<MapBoundaryId>> {
        position.validate()?;
        let connection = self.read_connection()?;
        let active_releases = active_release_keys(&connection, tenant_key)?;
        if active_releases.is_empty() {
            return Ok(Vec::new());
        }
        // DuckDB requires the query geometry at planning time for an R-tree
        // scan. These values have passed the typed finite WGS84 validator, so
        // materialize that geometry and the catalog-derived release ids as
        // escaped constants while keeping the tenant value parameterized.
        let sql = format!(
            "SELECT boundary.boundary_key \
             FROM map_visible_boundary AS boundary \
             WHERE boundary.tenant_key = ? \
               AND boundary.source_release_key IN ({}) \
               AND boundary.boundary_key IN (\
                 SELECT spatial.boundary_key FROM map_boundary AS spatial \
                 WHERE ST_Contains(spatial.geometry, ST_Point({}, {}))\
               ) \
               AND ST_Contains(boundary.geometry, ST_Point({}, {})) \
             ORDER BY boundary.boundary_key, boundary.source_release_key \
             LIMIT 1000",
            sql_string_list(&active_releases),
            position.longitude_deg,
            position.latitude_deg,
            position.longitude_deg,
            position.latitude_deg
        );
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query(params![tenant_key])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(row.get::<_, String>(0)?.parse()?);
        }
        Ok(result)
    }

    pub fn intersecting_boundary_ids(
        &self,
        tenant_key: &str,
        corridor: &Wgs84LineString,
    ) -> Result<Vec<MapBoundaryId>> {
        corridor.validate()?;
        let geometry = line_geojson(corridor)?;
        let connection = self.read_connection()?;
        let active_releases = active_release_keys(&connection, tenant_key)?;
        if active_releases.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT boundary.boundary_key \
             FROM map_visible_boundary AS boundary \
             WHERE boundary.tenant_key = ? \
               AND boundary.source_release_key IN ({}) \
               AND boundary.boundary_key IN (\
                 SELECT spatial.boundary_key FROM map_boundary AS spatial \
                 WHERE ST_Intersects(spatial.geometry, ST_GeomFromGeoJSON({}))\
               ) \
               AND ST_Intersects(boundary.geometry, ST_GeomFromGeoJSON({})) \
             ORDER BY boundary.boundary_key, boundary.source_release_key \
             LIMIT 1000",
            sql_string_list(&active_releases),
            duckdb_string_literal(&geometry),
            duckdb_string_literal(&geometry)
        );
        let mut statement = connection.prepare(&sql)?;
        let mut rows = statement.query(params![tenant_key])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(row.get::<_, String>(0)?.parse()?);
        }
        Ok(result)
    }

    pub(crate) fn replace_release_products<T>(
        &self,
        tenant_key: &str,
        release_id: &crate::contract::DatasetReleaseId,
        operation: impl FnOnce(&ReleaseProjectionWriter) -> Result<T>,
    ) -> Result<T> {
        let writer = ReleaseProjectionWriter::new(self.clone(), tenant_key, release_id)?;
        match operation(&writer) {
            Ok(output) => match writer.finish() {
                Ok(()) => Ok(output),
                Err(error) => {
                    writer.abort();
                    Err(error)
                }
            },
            Err(error) => {
                writer.abort();
                Err(error)
            }
        }
    }

    pub(crate) fn release_projection_complete(
        &self,
        tenant_key: &str,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<bool> {
        let connection = self.read_connection()?;
        connection
            .query_row(
                "SELECT count(*) = 1 FROM map_release_projection WHERE tenant_key = ? AND release_key = ?",
                params![tenant_key, release_id.as_str()],
                |row| row.get(0),
            )
            .context("checking Map release projection completion")
    }

    pub fn activate_release(
        &self,
        tenant_key: &str,
        dataset_id: &crate::contract::MapDatasetId,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let complete: bool = transaction.query_row(
            "SELECT count(*) = 1 FROM map_release_projection WHERE tenant_key = ? AND release_key = ?",
            params![tenant_key, release_id.as_str()],
            |row| row.get(0),
        )?;
        if !complete {
            bail!("Map release projection is incomplete and cannot be activated");
        }
        transaction.execute(
            "DELETE FROM map_active_release WHERE tenant_key = ? AND dataset_key = ?",
            params![tenant_key, dataset_id.as_str()],
        )?;
        transaction.execute(
            "INSERT INTO map_active_release VALUES (?, ?, ?)",
            params![tenant_key, dataset_id.as_str(), release_id.as_str()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn network_edges(
        &self,
        tenant_key: &str,
        map_family: MapFamily,
    ) -> Result<Vec<NetworkEdge>> {
        let connection = self.read_connection()?;
        let family = serde_json::to_value(map_family)?
            .as_str()
            .context("map family wire value")?
            .to_owned();
        let mut statement = connection.prepare(
            "SELECT edge_key, map_family, from_node, to_node, geometry_json, distance_m, nominal_duration_s, bidirectional, source_release_key FROM map_visible_network_edge WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND map_family = ? ORDER BY edge_key, source_release_key",
        )?;
        let mut rows = statement.query(params![tenant_key, tenant_key, family])?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            let family: String = row.get(1)?;
            result.push(NetworkEdge {
                edge_id: row.get(0)?,
                map_family: serde_json::from_value(serde_json::Value::String(family))?,
                from_node: row.get(2)?,
                to_node: row.get(3)?,
                geometry: serde_json::from_str(&row.get::<_, String>(4)?)?,
                distance_m: row.get(5)?,
                nominal_duration_s: row.get(6)?,
                bidirectional: row.get(7)?,
                source_release_id: row.get::<_, String>(8)?.parse()?,
            });
        }
        Ok(result)
    }

    fn initialize(&self) -> Result<()> {
        let connection = self.connection()?;
        let managed_table_count: u64 = connection.query_row(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'main' AND starts_with(table_name, 'map_')",
            [],
            |row| row.get(0),
        )?;
        let schema_exists: bool = connection.query_row(
            "SELECT count(*) > 0 FROM information_schema.tables WHERE table_schema = 'main' AND table_name = 'map_schema'",
            [],
            |row| row.get(0),
        )?;
        if schema_exists {
            let version: Option<i64> = connection.query_row(
                "SELECT CASE WHEN count(*) = 1 THEN max(version) ELSE NULL END FROM map_schema",
                [],
                |row| row.get(0),
            )?;
            match version {
                Some(SCHEMA_VERSION) => {}
                Some(REBUILD_SPATIAL_INDEXES_FROM_SCHEMA_VERSION) => {
                    rebuild_spatial_indexes_for_schema_upgrade(&connection)?;
                }
                _ => {
                    bail!(
                        "unsupported map analytics schema marker; rebuild the derived Map projection"
                    );
                }
            }
        } else if managed_table_count != 0 {
            bail!(
                "map analytics tables exist without a schema marker; rebuild the derived Map projection"
            );
        }
        connection.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS map_schema (version BIGINT NOT NULL);\n\
             INSERT INTO map_schema SELECT {SCHEMA_VERSION} WHERE NOT EXISTS (SELECT 1 FROM map_schema);\n\
             CREATE TABLE IF NOT EXISTS map_location (\
               tenant_key VARCHAR NOT NULL, location_key VARCHAR NOT NULL, name VARCHAR NOT NULL, longitude_deg DOUBLE NOT NULL, latitude_deg DOUBLE NOT NULL, canonical_json VARCHAR NOT NULL, source_release_key VARCHAR NOT NULL, projection_attempt_key VARCHAR NOT NULL, projection_ordinal UBIGINT NOT NULL\
             );\n\
             CREATE TABLE IF NOT EXISTS map_facility (\
               tenant_key VARCHAR NOT NULL, facility_key VARCHAR NOT NULL, name VARCHAR NOT NULL, kind VARCHAR NOT NULL, longitude_deg DOUBLE NOT NULL, latitude_deg DOUBLE NOT NULL, canonical_json VARCHAR NOT NULL, source_release_key VARCHAR NOT NULL, projection_attempt_key VARCHAR NOT NULL, projection_ordinal UBIGINT NOT NULL\
             );\n\
             CREATE TABLE IF NOT EXISTS map_active_release (\
               tenant_key VARCHAR NOT NULL, dataset_key VARCHAR NOT NULL, release_key VARCHAR NOT NULL, PRIMARY KEY (tenant_key, dataset_key), UNIQUE (tenant_key, release_key)\
             );\n\
             CREATE TABLE IF NOT EXISTS map_release_projection (\
               tenant_key VARCHAR NOT NULL, release_key VARCHAR NOT NULL, projection_attempt_key VARCHAR NOT NULL, location_count UBIGINT NOT NULL, facility_count UBIGINT NOT NULL, boundary_count UBIGINT NOT NULL, network_edge_count UBIGINT NOT NULL, source_feature_count UBIGINT NOT NULL, raster_product_count UBIGINT NOT NULL, completed_at TIMESTAMPTZ NOT NULL, PRIMARY KEY (tenant_key, release_key)\
             );\n\
             CREATE TABLE IF NOT EXISTS map_release_projection_attempt (\
               tenant_key VARCHAR NOT NULL, release_key VARCHAR NOT NULL, projection_attempt_key VARCHAR NOT NULL, started_at TIMESTAMPTZ NOT NULL, PRIMARY KEY (tenant_key, release_key, projection_attempt_key)\
             );\n\
             CREATE TABLE IF NOT EXISTS map_boundary (\
               tenant_key VARCHAR NOT NULL, boundary_key VARCHAR NOT NULL, name VARCHAR NOT NULL, kind VARCHAR NOT NULL, geometry GEOMETRY NOT NULL, canonical_json VARCHAR NOT NULL, source_release_key VARCHAR NOT NULL, projection_attempt_key VARCHAR NOT NULL, projection_ordinal UBIGINT NOT NULL\
             );\n\
             CREATE INDEX IF NOT EXISTS map_boundary_geometry ON map_boundary USING RTREE (geometry);\n\
             CREATE TABLE IF NOT EXISTS map_network_edge (\
               tenant_key VARCHAR NOT NULL, edge_key VARCHAR NOT NULL, map_family VARCHAR NOT NULL, from_node VARCHAR NOT NULL, to_node VARCHAR NOT NULL, geometry_json VARCHAR NOT NULL, distance_m DOUBLE NOT NULL, nominal_duration_s DOUBLE NOT NULL, bidirectional BOOLEAN NOT NULL, source_release_key VARCHAR NOT NULL, projection_attempt_key VARCHAR NOT NULL, projection_ordinal UBIGINT NOT NULL\
             );\n\
             CREATE TABLE IF NOT EXISTS map_source_feature (
               tenant_key VARCHAR NOT NULL,
               release_key VARCHAR NOT NULL,
               feature_key VARCHAR NOT NULL,
               source_key VARCHAR NOT NULL,
               source_element_type VARCHAR NOT NULL,
               source_element_key VARCHAR NOT NULL,
               source_element_version VARCHAR NOT NULL,
               representation VARCHAR NOT NULL,
               geometry_digest_sha256 VARCHAR NOT NULL,
               geometry GEOMETRY NOT NULL,
               normalized_text VARCHAR NOT NULL,
               tags_json JSON NOT NULL,
               canonical_json JSON NOT NULL,
               source_digest_sha256 VARCHAR NOT NULL,
               projection_attempt_key VARCHAR NOT NULL,
               projection_ordinal UBIGINT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS map_source_feature_geometry ON map_source_feature USING RTREE (geometry);
             CREATE TABLE IF NOT EXISTS map_raster_product (
               tenant_key VARCHAR NOT NULL,
               raster_key VARCHAR NOT NULL,
               release_key VARCHAR NOT NULL,
               source_key VARCHAR NOT NULL,
               checksum_sha256 VARCHAR NOT NULL,
               canonical_json JSON NOT NULL,
               projection_attempt_key VARCHAR NOT NULL,
               PRIMARY KEY (tenant_key, release_key, projection_attempt_key, raster_key)
             );
             CREATE VIEW IF NOT EXISTS map_visible_location AS
               SELECT item.* FROM map_location AS item JOIN map_release_projection AS projection ON projection.tenant_key = item.tenant_key AND projection.release_key = item.source_release_key AND projection.projection_attempt_key = item.projection_attempt_key;
             CREATE VIEW IF NOT EXISTS map_visible_facility AS
               SELECT item.* FROM map_facility AS item JOIN map_release_projection AS projection ON projection.tenant_key = item.tenant_key AND projection.release_key = item.source_release_key AND projection.projection_attempt_key = item.projection_attempt_key;
             CREATE VIEW IF NOT EXISTS map_visible_boundary AS
               SELECT item.* FROM map_boundary AS item JOIN map_release_projection AS projection ON projection.tenant_key = item.tenant_key AND projection.release_key = item.source_release_key AND projection.projection_attempt_key = item.projection_attempt_key;
             CREATE VIEW IF NOT EXISTS map_visible_network_edge AS
               SELECT item.* FROM map_network_edge AS item JOIN map_release_projection AS projection ON projection.tenant_key = item.tenant_key AND projection.release_key = item.source_release_key AND projection.projection_attempt_key = item.projection_attempt_key;
             CREATE VIEW IF NOT EXISTS map_visible_source_feature AS
               SELECT item.* FROM map_source_feature AS item JOIN map_release_projection AS projection ON projection.tenant_key = item.tenant_key AND projection.release_key = item.release_key AND projection.projection_attempt_key = item.projection_attempt_key;
             CREATE VIEW IF NOT EXISTS map_visible_raster_product AS
               SELECT item.* FROM map_raster_product AS item JOIN map_release_projection AS projection ON projection.tenant_key = item.tenant_key AND projection.release_key = item.release_key AND projection.projection_attempt_key = item.projection_attempt_key;
             CREATE TABLE IF NOT EXISTS map_raster_derivation (
               tenant_key VARCHAR NOT NULL,
               work_context_key VARCHAR NOT NULL,
               principal_key VARCHAR NOT NULL,
               derivation_key VARCHAR NOT NULL,
               raster_key VARCHAR NOT NULL,
               release_key VARCHAR NOT NULL,
               canonical_json JSON NOT NULL,
               PRIMARY KEY (tenant_key, work_context_key, derivation_key)
             );
             CREATE INDEX IF NOT EXISTS map_raster_derivation_source ON map_raster_derivation(tenant_key, work_context_key, raster_key, derivation_key);
             CREATE TABLE IF NOT EXISTS map_spatial_derivation (
               tenant_key VARCHAR NOT NULL,
               work_context_key VARCHAR NOT NULL,
               principal_key VARCHAR NOT NULL,
               derivation_key VARCHAR NOT NULL,
               mobility_profile_key VARCHAR NOT NULL,
               mobility_profile_version BIGINT NOT NULL,
               canonical_json JSON NOT NULL,
               PRIMARY KEY (tenant_key, work_context_key, derivation_key)
             );
             CREATE INDEX IF NOT EXISTS map_spatial_derivation_profile ON map_spatial_derivation(tenant_key, work_context_key, mobility_profile_key, mobility_profile_version, derivation_key);
             CREATE TABLE IF NOT EXISTS map_authored_feature_revision (
               tenant_key VARCHAR NOT NULL,
               work_context_key VARCHAR NOT NULL,
               layer_key VARCHAR NOT NULL,
               feature_key VARCHAR NOT NULL,
               feature_revision BIGINT NOT NULL,
               layer_revision BIGINT NOT NULL,
               schema_version BIGINT NOT NULL,
               changeset_key VARCHAR NOT NULL,
               commit_sequence BIGINT NOT NULL,
               deleted BOOLEAN NOT NULL,
               geometry_type VARCHAR NOT NULL,
               geometry GEOMETRY NOT NULL,
               bbox_west DOUBLE NOT NULL,
               bbox_south DOUBLE NOT NULL,
               bbox_east DOUBLE NOT NULL,
               bbox_north DOUBLE NOT NULL,
               valid_from TIMESTAMPTZ,
               valid_until TIMESTAMPTZ,
               semantic_type VARCHAR NOT NULL,
               title VARCHAR,
               properties_json JSON NOT NULL,
               canonical_json JSON NOT NULL,
               created_at TIMESTAMPTZ NOT NULL,
               PRIMARY KEY (tenant_key, layer_key, feature_key, feature_revision)
             );
             CREATE INDEX IF NOT EXISTS map_authored_revision_geometry ON map_authored_feature_revision USING RTREE (geometry);
             CREATE INDEX IF NOT EXISTS map_authored_revision_layer ON map_authored_feature_revision(tenant_key, work_context_key, layer_key, layer_revision, feature_key);
             CREATE TABLE IF NOT EXISTS map_authored_feature_head (
               tenant_key VARCHAR NOT NULL,
               work_context_key VARCHAR NOT NULL,
               layer_key VARCHAR NOT NULL,
               feature_key VARCHAR NOT NULL,
               feature_revision BIGINT NOT NULL,
               layer_revision BIGINT NOT NULL,
               schema_version BIGINT NOT NULL,
               changeset_key VARCHAR NOT NULL,
               commit_sequence BIGINT NOT NULL,
               deleted BOOLEAN NOT NULL,
               geometry_type VARCHAR NOT NULL,
               geometry GEOMETRY NOT NULL,
               bbox_west DOUBLE NOT NULL,
               bbox_south DOUBLE NOT NULL,
               bbox_east DOUBLE NOT NULL,
               bbox_north DOUBLE NOT NULL,
               valid_from TIMESTAMPTZ,
               valid_until TIMESTAMPTZ,
               semantic_type VARCHAR NOT NULL,
               title VARCHAR,
               properties_json JSON NOT NULL,
               canonical_json JSON NOT NULL,
               updated_at TIMESTAMPTZ NOT NULL,
               PRIMARY KEY (tenant_key, layer_key, feature_key)
             );
             CREATE INDEX IF NOT EXISTS map_authored_head_geometry ON map_authored_feature_head USING RTREE (geometry);
             CREATE INDEX IF NOT EXISTS map_authored_head_layer ON map_authored_feature_head(tenant_key, work_context_key, layer_key, deleted, feature_key);
             CREATE TABLE IF NOT EXISTS map_authored_projection (
               consumer VARCHAR PRIMARY KEY,
               last_sequence BIGINT NOT NULL,
               updated_at TIMESTAMPTZ NOT NULL
             );
             "
        ))?;
        let version: i64 =
            connection.query_row("SELECT max(version) FROM map_schema", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            bail!("unsupported map analytics schema version {version}");
        }
        self.verify_spatial()?;
        verify_spatial_indexes(&connection)
    }

    pub(crate) fn connection(&self) -> Result<Connection> {
        self.instance.connection()
    }

    pub(crate) fn read_connection(&self) -> Result<Connection> {
        self.instance.read_connection()
    }

    pub(crate) fn task_connection(&self, directory: &Path) -> Result<Connection> {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("creating task directory {}", directory.display()))?;
        let directory = directory
            .canonicalize()
            .with_context(|| format!("canonicalizing task directory {}", directory.display()))?;
        if directory.parent() != Some(self.authoring_task_root.as_path()) {
            bail!("task directory must be a direct child of the authoring task root");
        }
        self.instance.connection()
    }
}

fn verify_spatial_indexes(connection: &Connection) -> Result<()> {
    for index in SPATIAL_INDEXES {
        connection
            .query_row(
                &format!("SELECT count(*) FROM rtree_index_dump('{}')", index.name),
                [],
                |row| row.get::<_, u64>(0),
            )
            .with_context(|| {
                format!("binding and verifying DuckDB Spatial index {}", index.name)
            })?;
    }
    Ok(())
}

fn rebuild_spatial_indexes_for_schema_upgrade(connection: &Connection) -> Result<()> {
    let mut drop_sql = String::new();
    for index in SPATIAL_INDEXES {
        drop_sql.push_str(&format!("DROP INDEX IF EXISTS {};\n", index.name));
    }
    connection.execute_batch(&drop_sql).with_context(|| {
        format!(
            "dropping DuckDB Spatial indexes while upgrading Map analytics schema from {REBUILD_SPATIAL_INDEXES_FROM_SCHEMA_VERSION} to {SCHEMA_VERSION}"
        )
    })?;

    let mut create_sql = String::from("BEGIN TRANSACTION;\n");
    for index in SPATIAL_INDEXES {
        create_sql.push_str(&format!(
            "CREATE INDEX {} ON {} USING RTREE ({});\n",
            index.name, index.table, index.column
        ));
    }
    create_sql.push_str(&format!(
        "UPDATE map_schema SET version = {SCHEMA_VERSION} WHERE version = {REBUILD_SPATIAL_INDEXES_FROM_SCHEMA_VERSION};\nCOMMIT;"
    ));
    connection.execute_batch(&create_sql).with_context(|| {
        format!(
            "rebuilding DuckDB Spatial indexes while upgrading Map analytics schema from {REBUILD_SPATIAL_INDEXES_FROM_SCHEMA_VERSION} to {SCHEMA_VERSION}"
        )
    })
}

fn polygon_geojson(polygon: &crate::contract::Wgs84Polygon) -> Result<String> {
    let mut rings = Vec::with_capacity(polygon.interiors.len() + 1);
    rings.push(
        polygon
            .exterior
            .iter()
            .map(|position| {
                geojson::Position::from([position.longitude_deg, position.latitude_deg])
            })
            .collect(),
    );
    rings.extend(polygon.interiors.iter().map(|ring| {
        ring.iter()
            .map(|position| {
                geojson::Position::from([position.longitude_deg, position.latitude_deg])
            })
            .collect()
    }));
    Ok(serde_json::to_string(&geojson::Geometry::new(
        geojson::GeometryValue::Polygon { coordinates: rings },
    ))?)
}

fn line_geojson(line: &Wgs84LineString) -> Result<String> {
    Ok(serde_json::to_string(&geojson::Geometry::new(
        geojson::GeometryValue::LineString {
            coordinates: line
                .coordinates
                .iter()
                .map(|position| {
                    geojson::Position::from([position.longitude_deg, position.latitude_deg])
                })
                .collect(),
        },
    ))?)
}

const SOURCE_FEATURE_QUERY_DOMAIN: &str = "veoveo.io/map/source-feature-query/v2";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFeatureCursor {
    query_domain: String,
    query_digest_sha256: String,
    distance_m: Option<f64>,
    feature_id: String,
}

fn source_query_digest(request: &QuerySourceFeaturesRequest) -> Result<String> {
    let mut canonical = request.clone();
    canonical.cursor = None;
    let bytes = serde_json::to_vec(&canonical)?;
    let mut digest = Sha256::new();
    digest.update(SOURCE_FEATURE_QUERY_DOMAIN.as_bytes());
    digest.update(b"\0");
    digest.update(bytes);
    Ok(hex::encode(digest.finalize()))
}

fn encode_source_cursor(cursor: &SourceFeatureCursor) -> Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor)?))
}

fn decode_source_cursor(value: &str) -> Result<SourceFeatureCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .context("source-feature cursor is not canonical base64url")?;
    let cursor: SourceFeatureCursor =
        serde_json::from_slice(&bytes).context("source-feature cursor is invalid")?;
    if cursor.query_domain != SOURCE_FEATURE_QUERY_DOMAIN
        || cursor.query_digest_sha256.len() != 64
        || !cursor
            .query_digest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || SourceFeatureId::parse(cursor.feature_id.clone()).is_err()
        || cursor
            .distance_m
            .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        bail!("source-feature cursor fields are invalid");
    }
    Ok(cursor)
}

fn validate_source_cursor_order(
    cursor: &SourceFeatureCursor,
    distance_ordered: bool,
) -> Result<()> {
    if cursor.distance_m.is_some() != distance_ordered {
        bail!("source-feature cursor does not match the query ordering");
    }
    Ok(())
}

fn source_feature_query_sql(
    tenant_key: &str,
    request: &QuerySourceFeaturesRequest,
    cursor: Option<&SourceFeatureCursor>,
) -> Result<String> {
    let mut source_predicates = vec![
        format!("feature.tenant_key = {}", duckdb_string_literal(tenant_key)),
        format!(
            "feature.release_key = {}",
            duckdb_string_literal(request.release_id.as_str())
        ),
    ];
    if let Some(source_id) = &request.source_id {
        source_predicates.push(format!(
            "feature.source_key = {}",
            duckdb_string_literal(source_id.as_str())
        ));
    }
    if let Some(source_element_id) = &request.source_element_id {
        source_predicates.push(format!(
            "feature.source_element_key = {}",
            duckdb_string_literal(source_element_id)
        ));
    }
    if let Some(representation) = request.representation {
        let representation = enum_wire(representation)?;
        source_predicates.push(format!(
            "feature.representation = {}",
            duckdb_string_literal(&representation)
        ));
    }
    if let Some(text) = request.normalized_text.as_deref() {
        source_predicates.push(format!(
            "feature.normalized_text LIKE {}",
            duckdb_string_literal(&format!("%{}%", text.trim().to_lowercase()))
        ));
    }
    for filter in &request.tags_equal {
        let path = json_pointer_path(&filter.key);
        source_predicates.push(format!(
            "json_exists(feature.tags_json, {}) AND json_type(feature.tags_json, {}) = 'VARCHAR' AND json_extract_string(feature.tags_json, {}) = {}",
            duckdb_string_literal(&path),
            duckdb_string_literal(&path),
            duckdb_string_literal(&path),
            duckdb_string_literal(&filter.value)
        ));
    }
    for key in &request.tags_exist {
        let path = json_pointer_path(key);
        source_predicates.push(format!(
            "json_exists(feature.tags_json, {})",
            duckdb_string_literal(&path)
        ));
    }

    let distance_expression = request
        .spatial
        .as_ref()
        .and_then(source_distance_expression);
    let distance_ordered = distance_expression.is_some();
    if let Some(cursor) = cursor {
        validate_source_cursor_order(cursor, distance_ordered)?;
    }
    if let Some(spatial) = request.spatial.as_ref()
        && let Some(predicate) = source_base_spatial_predicate(spatial, "feature")?
    {
        source_predicates.push(predicate);
        source_predicates.push(format!(
            "feature.feature_key IN ({})",
            source_spatial_candidate_query(spatial)?
                .context("index-eligible spatial predicate has no candidate query")?
        ));
    }

    let mut scored_predicates = Vec::new();
    if let Some(spatial) = request.spatial.as_ref()
        && let Some(limit) = source_distance_limit(spatial)
    {
        scored_predicates.push(format!("scored.distance_m <= {limit}"));
    }
    if let Some(cursor) = cursor {
        if let Some(distance) = cursor.distance_m {
            scored_predicates.push(format!(
                "(scored.distance_m > {distance} OR (scored.distance_m = {distance} AND scored.feature_key > {}))",
                duckdb_string_literal(&cursor.feature_id)
            ));
        } else {
            scored_predicates.push(format!(
                "scored.feature_key > {}",
                duckdb_string_literal(&cursor.feature_id)
            ));
        }
    }

    let distance_projection = distance_expression.as_deref().unwrap_or("NULL::DOUBLE");
    let ordering = if distance_ordered {
        "scored.distance_m ASC, scored.feature_key ASC"
    } else {
        "scored.feature_key ASC"
    };
    let scored_filter = if scored_predicates.is_empty() {
        "TRUE".to_owned()
    } else {
        scored_predicates.join(" AND ")
    };
    Ok(format!(
        "WITH scored AS MATERIALIZED (\
           SELECT feature.canonical_json, feature.feature_key, {distance_projection} AS distance_m \
           FROM map_visible_source_feature AS feature \
           WHERE {}\
         ) \
         SELECT scored.canonical_json, scored.distance_m \
         FROM scored WHERE {scored_filter} ORDER BY {ordering} LIMIT {}",
        source_predicates.join(" AND "),
        u64::from(request.limit) + 1
    ))
}

fn source_feature_normalized_text(feature: &SourceFeature) -> Result<String> {
    let mut values = Vec::new();
    values.extend(feature.original_names.values().cloned());
    values.extend(feature.original_references.iter().cloned());
    for (key, value) in &feature.normalized_tags {
        values.push(key.clone());
        values.push(match value {
            serde_json::Value::String(value) => value.clone(),
            value => serde_json::to_string(value)?,
        });
    }
    Ok(values.join("\n").to_lowercase())
}

fn source_distance_expression(spatial: &SourceSpatialQuery) -> Option<String> {
    let position = match spatial {
        SourceSpatialQuery::WithinDistance { position, .. }
        | SourceSpatialQuery::Nearest { position, .. } => position,
        _ => return None,
    };
    Some(format!(
        "ST_Distance_Sphere(CAST(ST_Centroid(feature.geometry) AS POINT_2D), ST_Point2D({}, {}))",
        position.longitude_deg, position.latitude_deg
    ))
}

fn source_base_spatial_predicate(
    spatial: &SourceSpatialQuery,
    relation: &str,
) -> Result<Option<String>> {
    match spatial {
        SourceSpatialQuery::BoundingBox { bounds } => {
            if bounds.west <= bounds.east {
                Ok(Some(format!(
                    "ST_Intersects({relation}.geometry, ST_MakeEnvelope({}, {}, {}, {}))",
                    bounds.west, bounds.south, bounds.east, bounds.north
                )))
            } else {
                Ok(Some(format!(
                    "(ST_Intersects({relation}.geometry, ST_MakeEnvelope({}, {}, 180, {})) OR ST_Intersects({relation}.geometry, ST_MakeEnvelope(-180, {}, {}, {})))",
                    bounds.west,
                    bounds.south,
                    bounds.north,
                    bounds.south,
                    bounds.east,
                    bounds.north
                )))
            }
        }
        SourceSpatialQuery::Intersects { geometry } => Ok(Some(format!(
            "ST_Intersects({relation}.geometry, ST_GeomFromGeoJSON({}))",
            duckdb_string_literal(&geometry.to_geojson_string()?)
        ))),
        SourceSpatialQuery::Contains { geometry } => Ok(Some(format!(
            "ST_Contains({relation}.geometry, ST_GeomFromGeoJSON({}))",
            duckdb_string_literal(&geometry.to_geojson_string()?)
        ))),
        SourceSpatialQuery::Within { geometry } => Ok(Some(format!(
            "ST_Within({relation}.geometry, ST_GeomFromGeoJSON({}))",
            duckdb_string_literal(&geometry.to_geojson_string()?)
        ))),
        SourceSpatialQuery::WithinDistance { .. } | SourceSpatialQuery::Nearest { .. } => Ok(None),
    }
}

fn source_spatial_candidate_query(spatial: &SourceSpatialQuery) -> Result<Option<String>> {
    if let SourceSpatialQuery::BoundingBox { bounds } = spatial
        && bounds.west > bounds.east
    {
        let west = SourceSpatialQuery::BoundingBox {
            bounds: Wgs84BoundingBox {
                west: bounds.west,
                south: bounds.south,
                east: 180.0,
                north: bounds.north,
            },
        };
        let east = SourceSpatialQuery::BoundingBox {
            bounds: Wgs84BoundingBox {
                west: -180.0,
                south: bounds.south,
                east: bounds.east,
                north: bounds.north,
            },
        };
        return Ok(Some(format!(
            "SELECT spatial.feature_key FROM map_source_feature AS spatial WHERE {} UNION SELECT spatial.feature_key FROM map_source_feature AS spatial WHERE {}",
            source_base_spatial_predicate(&west, "spatial")?
                .context("western dateline segment has no spatial predicate")?,
            source_base_spatial_predicate(&east, "spatial")?
                .context("eastern dateline segment has no spatial predicate")?,
        )));
    }
    Ok(
        source_base_spatial_predicate(spatial, "spatial")?.map(|predicate| {
            format!(
                "SELECT spatial.feature_key FROM map_source_feature AS spatial WHERE {predicate}"
            )
        }),
    )
}

fn source_distance_limit(spatial: &SourceSpatialQuery) -> Option<f64> {
    match spatial {
        SourceSpatialQuery::WithinDistance { distance, .. } => Some(distance.get()),
        SourceSpatialQuery::Nearest {
            maximum_distance, ..
        } => maximum_distance.map(Meters::get),
        _ => None,
    }
}

fn enum_wire<T: Serialize>(value: T) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .context("enum has no string wire representation")
}

fn duckdb_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn json_pointer_path(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn sql_string_list(values: &[String]) -> String {
    values
        .iter()
        .map(|value| duckdb_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn active_release_keys(connection: &Connection, tenant_key: &str) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT release_key FROM map_active_release WHERE tenant_key = ? ORDER BY release_key",
    )?;
    let mut rows = statement.query(params![tenant_key])?;
    let mut releases = Vec::new();
    while let Some(row) = rows.next()? {
        releases.push(row.get(0)?);
    }
    Ok(releases)
}

fn validate_nearby_query(
    position: &Wgs84Position,
    radius: Meters,
    limit: u32,
    entity: &str,
) -> Result<()> {
    position.validate()?;
    if radius.get() <= 0.0 || radius.get() > 1_000_000.0 {
        bail!("nearby {entity} radius must be within (0, 1000000] meters");
    }
    if !(1..=100).contains(&limit) {
        bail!("nearby {entity} limit must be within 1..=100");
    }
    Ok(())
}

fn longitude_predicate(coverage: &Wgs84BoundingBox, column: &str) -> String {
    if coverage.west <= coverage.east {
        format!("{column} BETWEEN ? AND ?")
    } else {
        format!("({column} >= ? OR {column} <= ?)")
    }
}

fn select_canonical<T: serde::de::DeserializeOwned>(
    connection: &Connection,
    table: &'static str,
    key_column: &'static str,
    key: &str,
    tenant_key: &str,
) -> Result<Option<T>> {
    let sql = format!(
        "SELECT canonical_json FROM {table} WHERE tenant_key = ? AND {key_column} = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) ORDER BY source_release_key ASC LIMIT 1"
    );
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params![tenant_key, key, tenant_key])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let value: String = row.get(0)?;
    Ok(Some(serde_json::from_str(&value)?))
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::TempDir;

    use crate::contract::{
        AuthorityClass, DatasetLicense, DatasetReleaseId, FeatureGeometry, GeoJsonPosition,
        HttpsEndpoint, LocationId, MapDatasetId, MapSourceId, SOURCE_FEATURE_SCHEMA_VERSION,
        SourceElementType, SourceFeatureRepresentation, SourceLineage, SourceTagEquality,
    };

    use super::*;

    fn analytics_config(root: &TempDir, spatial_extension: &std::ffi::OsStr) -> MapAnalyticsConfig {
        MapAnalyticsConfig {
            database_path: root.path().join("map.duckdb"),
            authoring_task_root: root.path().join("tasks"),
            spill_dir: root.path().join("spill"),
            spatial_extension: spatial_extension.into(),
            memory_limit: "256MB".to_owned(),
            threads: 1,
        }
    }

    fn configured_analytics(root: &TempDir, spatial_extension: &std::ffi::OsStr) -> MapAnalytics {
        MapAnalytics::open(analytics_config(root, spatial_extension)).unwrap()
    }

    fn test_location(release_id: &DatasetReleaseId, name: &str) -> MapLocation {
        MapLocation {
            location_id: LocationId::from_stable_key(b"stable-location"),
            name: name.to_owned(),
            position: Wgs84Position::new(-73.9857, 40.7484, None).unwrap(),
            alternate_names: Default::default(),
            lineage: SourceLineage {
                release_id: release_id.clone(),
                source_feature_id: "source-feature".to_owned(),
                authority: AuthorityClass::SyntheticTest,
                valid_from: Utc::now(),
                valid_until: None,
            },
        }
    }

    fn test_source_feature(release_id: &DatasetReleaseId, index: usize) -> SourceFeature {
        test_source_feature_at(release_id, index, -73.9857, 40.7484)
    }

    fn test_source_feature_at(
        release_id: &DatasetReleaseId,
        index: usize,
        longitude_deg: f64,
        latitude_deg: f64,
    ) -> SourceFeature {
        let geometry =
            FeatureGeometry::Point(GeoJsonPosition::new(longitude_deg, latitude_deg, None));
        let geometry_digest_sha256 =
            hex::encode(Sha256::digest(geometry.to_geojson_string().unwrap()));
        SourceFeature {
            schema_version: SOURCE_FEATURE_SCHEMA_VERSION,
            feature_id: SourceFeatureId::from_stable_key(format!("feature-{index}").as_bytes()),
            source_id: MapSourceId::from_stable_key(b"synthetic-source"),
            release_id: release_id.clone(),
            source_element_type: SourceElementType::Node,
            source_element_id: format!("node-{index}"),
            source_element_version: "1".to_owned(),
            representation: SourceFeatureRepresentation::Point,
            source_geometry_path: Vec::new(),
            geometry,
            geometry_digest_sha256,
            normalized_tags: [
                (
                    "source/ref~id".to_owned(),
                    serde_json::Value::String(format!("lamp-{index}")),
                ),
                ("height".to_owned(), serde_json::json!(42)),
                ("nullable".to_owned(), serde_json::Value::Null),
            ]
            .into_iter()
            .collect(),
            original_names: Default::default(),
            original_references: Default::default(),
            operating_area_ids: Default::default(),
            source_digest_sha256: hex::encode(Sha256::digest(b"synthetic-source")),
            license: DatasetLicense {
                license_id: "synthetic-license".to_owned(),
                source_terms_uri: HttpsEndpoint::parse("https://example.com/terms").unwrap(),
                attribution: "Synthetic test data".to_owned(),
                redistribution_allowed: true,
                derivatives_allowed: true,
                offline_bundle_allowed: true,
                expires_at: None,
            },
            acquired_at: Utc::now(),
        }
    }

    #[test]
    fn longitude_predicate_supports_dateline_crossing() {
        let crossing = Wgs84BoundingBox {
            west: 170.0,
            south: -10.0,
            east: -170.0,
            north: 10.0,
        };
        assert_eq!(
            longitude_predicate(&crossing, "longitude_deg"),
            "(longitude_deg >= ? OR longitude_deg <= ?)"
        );
    }

    #[test]
    fn duckdb_literals_escape_single_quotes() {
        assert_eq!(duckdb_string_literal("a'b"), "'a''b'");
        assert_eq!(
            sql_string_list(&["release-a".to_owned(), "release-'b".to_owned()]),
            "'release-a', 'release-''b'"
        );
    }

    #[test]
    fn source_cursor_is_bound_to_the_query_digest() {
        let request = QuerySourceFeaturesRequest {
            release_id: crate::contract::DatasetReleaseId::new(),
            source_id: None,
            source_element_id: None,
            representation: None,
            tags_equal: Vec::new(),
            tags_exist: vec!["highway".to_owned()],
            normalized_text: None,
            spatial: None,
            limit: 50,
            cursor: None,
        };
        let digest = source_query_digest(&request).unwrap();
        let encoded = encode_source_cursor(&SourceFeatureCursor {
            query_domain: SOURCE_FEATURE_QUERY_DOMAIN.to_owned(),
            query_digest_sha256: digest.clone(),
            distance_m: None,
            feature_id: SourceFeatureId::new().to_string(),
        })
        .unwrap();
        assert_eq!(
            decode_source_cursor(&encoded).unwrap().query_digest_sha256,
            digest
        );
    }

    #[test]
    fn source_cursor_rejects_the_previous_domain_and_invalid_distance_shape() {
        let feature_id = SourceFeatureId::new().to_string();
        let legacy = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&serde_json::json!({
                "query_digest_sha256": hex::encode(Sha256::digest(b"legacy-query")),
                "distance_m": null,
                "feature_id": feature_id,
            }))
            .unwrap(),
        );
        assert!(decode_source_cursor(&legacy).is_err());

        let negative = encode_source_cursor(&SourceFeatureCursor {
            query_domain: SOURCE_FEATURE_QUERY_DOMAIN.to_owned(),
            query_digest_sha256: hex::encode(Sha256::digest(b"current-query")),
            distance_m: Some(-1.0),
            feature_id: SourceFeatureId::new().to_string(),
        })
        .unwrap();
        assert!(decode_source_cursor(&negative).is_err());

        let feature_ordered = SourceFeatureCursor {
            query_domain: SOURCE_FEATURE_QUERY_DOMAIN.to_owned(),
            query_digest_sha256: hex::encode(Sha256::digest(b"feature-query")),
            distance_m: None,
            feature_id: SourceFeatureId::new().to_string(),
        };
        assert!(validate_source_cursor_order(&feature_ordered, true).is_err());

        let distance_ordered = SourceFeatureCursor {
            query_domain: SOURCE_FEATURE_QUERY_DOMAIN.to_owned(),
            query_digest_sha256: hex::encode(Sha256::digest(b"distance-query")),
            distance_m: Some(1.0),
            feature_id: SourceFeatureId::new().to_string(),
        };
        assert!(validate_source_cursor_order(&distance_ordered, false).is_err());
    }

    #[test]
    fn source_distance_query_materializes_one_typed_score() {
        let request = QuerySourceFeaturesRequest {
            release_id: crate::contract::DatasetReleaseId::new(),
            source_id: None,
            source_element_id: None,
            representation: None,
            tags_equal: Vec::new(),
            tags_exist: Vec::new(),
            normalized_text: None,
            spatial: Some(SourceSpatialQuery::Nearest {
                position: Wgs84Position::new(-89.2, 13.7, None).unwrap(),
                maximum_distance: Some(Meters::new(1_000.0).unwrap()),
            }),
            limit: 50,
            cursor: None,
        };
        let sql = source_feature_query_sql("tenant", &request, None).unwrap();
        assert!(sql.contains("AS MATERIALIZED"));
        assert_eq!(sql.matches("ST_Distance_Sphere").count(), 1);
        assert!(sql.contains("CAST(ST_Centroid(feature.geometry) AS POINT_2D)"));
        assert!(sql.contains("ST_Point2D(-89.2, 13.7)"));
        assert!(sql.contains("scored.distance_m <= 1000"));
        assert!(sql.contains("ORDER BY scored.distance_m ASC"));

        assert!(NEARBY_FACILITIES_SQL.contains("AS MATERIALIZED"));
        assert_eq!(
            NEARBY_FACILITIES_SQL.matches("ST_Distance_Sphere").count(),
            1
        );
        assert!(NEARBY_FACILITIES_SQL.contains("ST_Point2D(longitude_deg, latitude_deg)"));
    }

    #[test]
    fn source_bbox_query_has_rtree_candidates_and_exact_dateline_filtering() {
        let request = QuerySourceFeaturesRequest {
            release_id: crate::contract::DatasetReleaseId::new(),
            source_id: None,
            source_element_id: None,
            representation: None,
            tags_equal: Vec::new(),
            tags_exist: Vec::new(),
            normalized_text: None,
            spatial: Some(SourceSpatialQuery::BoundingBox {
                bounds: Wgs84BoundingBox {
                    west: 170.0,
                    south: -10.0,
                    east: -170.0,
                    north: 10.0,
                },
            }),
            limit: 50,
            cursor: None,
        };
        let sql = source_feature_query_sql("tenant", &request, None).unwrap();
        assert!(sql.contains("feature.feature_key IN ("));
        assert!(sql.contains("FROM map_source_feature AS spatial"));
        assert!(sql.contains(" UNION "));
        assert_eq!(sql.matches("ST_Intersects(spatial.geometry").count(), 2);
        assert!(sql.contains("(ST_Intersects(feature.geometry"));
    }

    #[test]
    fn geojson_axis_distance_order_and_cursor_survive_restart() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let release = DatasetReleaseId::new();
        let first = test_source_feature_at(&release, 0, -89.2, 13.7);
        let second = test_source_feature_at(&release, 1, -88.2, 13.7);
        let analytics = configured_analytics(&root, &extension);
        analytics
            .replace_release_products("tenant", &release, |writer| {
                writer.put_source_feature("tenant", &first)?;
                writer.put_source_feature("tenant", &second)
            })
            .unwrap();
        let always_xy: bool = analytics
            .connection()
            .unwrap()
            .query_row("SELECT current_setting('geometry_always_xy')", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(always_xy);

        let request = QuerySourceFeaturesRequest {
            release_id: release,
            source_id: None,
            source_element_id: None,
            representation: None,
            tags_equal: Vec::new(),
            tags_exist: Vec::new(),
            normalized_text: None,
            spatial: Some(SourceSpatialQuery::Nearest {
                position: Wgs84Position::new(-89.2, 13.7, None).unwrap(),
                maximum_distance: Some(Meters::new(150_000.0).unwrap()),
            }),
            limit: 1,
            cursor: None,
        };
        let first_page = analytics.query_source_features("tenant", &request).unwrap();
        assert_eq!(first_page.features[0].feature.feature_id, first.feature_id);
        assert!(first_page.features[0].distance.unwrap().get() < 0.01);
        let mut second_request = request.clone();
        second_request.cursor = first_page.next_cursor;
        let second_page = analytics
            .query_source_features("tenant", &second_request)
            .unwrap();
        assert_eq!(
            second_page.features[0].feature.feature_id,
            second.feature_id
        );
        let second_distance = second_page.features[0].distance.unwrap().get();
        assert!((107_000.0..109_500.0).contains(&second_distance));

        drop(analytics);
        let reopened = configured_analytics(&root, &extension);
        let replay = reopened
            .query_source_features("tenant", &second_request)
            .unwrap();
        assert_eq!(replay.features[0].feature.feature_id, second.feature_id);
        assert!((replay.features[0].distance.unwrap().get() - second_distance).abs() < 0.001);
    }

    #[test]
    fn completed_release_attempts_preserve_stable_ids_across_releases() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let analytics = configured_analytics(&root, &extension);
        let releases = [DatasetReleaseId::new(), DatasetReleaseId::new()];
        for (index, release) in releases.iter().enumerate() {
            let location = test_location(release, "shared location");
            analytics
                .replace_release_products("tenant", release, |writer| {
                    writer.put_location("tenant", &location)
                })
                .unwrap();
            analytics
                .activate_release("tenant", &MapDatasetId::new(), release)
                .unwrap();
            assert!(
                analytics
                    .release_projection_complete("tenant", &releases[index])
                    .unwrap()
            );
        }
        let locations = analytics.list_locations("tenant", 10).unwrap();
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].location_id, locations[1].location_id);
        assert!(
            locations[0].lineage.release_id.as_str() < locations[1].lineage.release_id.as_str()
        );
    }

    #[test]
    fn nearby_locations_are_resolved_and_distance_ordered_in_one_query() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let analytics = configured_analytics(&root, &extension);
        let release = DatasetReleaseId::new();
        let mut near = test_location(&release, "Brooklyn Bridge Center");
        near.location_id = LocationId::from_stable_key(b"near-location");
        near.position = Wgs84Position::new(-73.9969, 40.7061, None).unwrap();
        let mut far = test_location(&release, "Times Square");
        far.location_id = LocationId::from_stable_key(b"far-location");
        far.position = Wgs84Position::new(-73.9855, 40.7580, None).unwrap();
        analytics
            .replace_release_products("tenant", &release, |writer| {
                writer.put_location("tenant", &far)?;
                writer.put_location("tenant", &near)
            })
            .unwrap();
        analytics
            .activate_release("tenant", &MapDatasetId::new(), &release)
            .unwrap();

        let matches = analytics
            .nearby_location_matches(
                "tenant",
                &Wgs84Position::new(-73.97267, 40.70571, None).unwrap(),
                Meters::new(10_000.0).unwrap(),
                10,
            )
            .unwrap();

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].location.name, "Brooklyn Bridge Center");
        assert!(matches[0].distance.get() < matches[1].distance.get());
        assert_eq!(
            analytics.active_release_ids("tenant").unwrap(),
            vec![release]
        );
    }

    #[test]
    fn invalid_projection_never_becomes_visible_or_activatable() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let analytics = configured_analytics(&root, &extension);
        let release = DatasetReleaseId::new();
        let location = test_location(&release, "duplicate location");
        let error = analytics
            .replace_release_products("tenant", &release, |writer| {
                writer.put_location("tenant", &location)?;
                writer.put_location("tenant", &location)
            })
            .unwrap_err();
        assert!(error.to_string().contains("duplicate stored identities"));
        assert!(
            !analytics
                .release_projection_complete("tenant", &release)
                .unwrap()
        );
        assert!(analytics.list_locations("tenant", 10).unwrap().is_empty());
        assert!(
            analytics
                .activate_release("tenant", &MapDatasetId::new(), &release)
                .unwrap_err()
                .to_string()
                .contains("incomplete")
        );
    }

    #[test]
    fn obsolete_projection_schema_requires_an_explicit_rebuild() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let database_path = root.path().join("map.duckdb");
        let connection = duckdb::Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE map_schema(version BIGINT); INSERT INTO map_schema VALUES (6)",
            )
            .unwrap();
        drop(connection);
        let error = MapAnalytics::open(analytics_config(&root, &extension)).unwrap_err();
        assert!(error.to_string().contains("schema marker"));
    }

    #[test]
    fn schema_nine_upgrade_rebuilds_spatial_indexes_without_losing_mixed_geometries() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let analytics = configured_analytics(&root, &extension);
        let connection = analytics.connection().unwrap();
        connection
            .execute_batch(
                r#"
                INSERT INTO map_source_feature (
                  tenant_key, release_key, feature_key, source_key,
                  source_element_type, source_element_key, source_element_version,
                  representation, geometry_digest_sha256, geometry, normalized_text,
                  tags_json, canonical_json, source_digest_sha256,
                  projection_attempt_key, projection_ordinal
                ) VALUES
                  ('tenant', 'release', 'point', 'source', 'node', 'point', '1',
                   'center', 'point-digest', ST_GeomFromGeoJSON('{"type":"Point","coordinates":[-89.214,13.696]}'),
                   'point', '{}', '{}', 'source-digest', 'attempt', 0),
                  ('tenant', 'release', 'line', 'source', 'way', 'line', '1',
                   'centerline', 'line-digest', ST_GeomFromGeoJSON('{"type":"LineString","coordinates":[[-89.22,13.69],[-89.21,13.70]]}'),
                   'line', '{}', '{}', 'source-digest', 'attempt', 1),
                  ('tenant', 'release', 'polygon', 'source', 'relation', 'polygon', '1',
                   'footprint', 'polygon-digest', ST_GeomFromGeoJSON('{"type":"Polygon","coordinates":[[[-89.22,13.69],[-89.21,13.69],[-89.21,13.70],[-89.22,13.70],[-89.22,13.69]]]}'),
                   'polygon', '{}', '{}', 'source-digest', 'attempt', 2);
                UPDATE map_schema SET version = 9;
                "#,
            )
            .unwrap();
        drop(connection);
        drop(analytics);

        let upgraded = configured_analytics(&root, &extension);
        let connection = upgraded.connection().unwrap();
        let version: i64 = connection
            .query_row("SELECT version FROM map_schema", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        let stored: u64 = connection
            .query_row("SELECT count(*) FROM map_source_feature", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, 3);
        let indexed: u64 = connection
            .query_row(
                "SELECT count(*) FROM rtree_index_dump('map_source_feature_geometry') WHERE row_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, stored);
    }

    #[test]
    fn interrupted_batches_stay_invisible_and_retry_under_a_new_attempt() {
        let Some(extension) = std::env::var_os("VEOVEO_TEST_DUCKDB_SPATIAL_EXTENSION") else {
            return;
        };
        let root = TempDir::new().unwrap();
        let analytics = configured_analytics(&root, &extension);
        let release = DatasetReleaseId::new();
        let interrupted =
            ReleaseProjectionWriter::new(analytics.clone(), "tenant", &release).unwrap();
        for index in 0..=256 {
            interrupted
                .put_source_feature("tenant", &test_source_feature(&release, index))
                .unwrap();
        }
        drop(interrupted);
        let feature = test_source_feature(&release, 0);
        assert!(
            analytics
                .source_feature("tenant", &release, &feature.feature_id)
                .unwrap()
                .is_none()
        );

        analytics
            .replace_release_products("tenant", &release, |writer| {
                writer.put_source_feature("tenant", &feature)
            })
            .unwrap();
        assert!(
            analytics
                .source_feature("tenant", &release, &feature.feature_id)
                .unwrap()
                .is_some()
        );
        let query = analytics
            .query_source_features(
                "tenant",
                &QuerySourceFeaturesRequest {
                    release_id: release.clone(),
                    source_id: None,
                    source_element_id: None,
                    representation: None,
                    tags_equal: vec![SourceTagEquality {
                        key: "source/ref~id".to_owned(),
                        value: "lamp-0".to_owned(),
                    }],
                    tags_exist: vec!["nullable".to_owned()],
                    normalized_text: None,
                    spatial: None,
                    limit: 10,
                    cursor: None,
                },
            )
            .unwrap();
        assert_eq!(query.features.len(), 1);
        let numeric_does_not_match_string = analytics
            .query_source_features(
                "tenant",
                &QuerySourceFeaturesRequest {
                    release_id: release,
                    source_id: None,
                    source_element_id: None,
                    representation: None,
                    tags_equal: vec![SourceTagEquality {
                        key: "height".to_owned(),
                        value: "42".to_owned(),
                    }],
                    tags_exist: Vec::new(),
                    normalized_text: None,
                    spatial: None,
                    limit: 10,
                    cursor: None,
                },
            )
            .unwrap();
        assert!(numeric_does_not_match_string.features.is_empty());
    }
}
