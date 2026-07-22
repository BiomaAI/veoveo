use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use duckdb::{Connection, params};
use veoveo_duckdb_runtime::{EngineSettings, FileAccess, TrustedExtension, open_connection};

use crate::contract::{
    Facility, MapBoundary, MapBoundaryId, MapFamily, MapLocation, Meters, SearchLocationsOutput,
    SearchLocationsRequest, Wgs84BoundingBox, Wgs84LineString, Wgs84Position,
};

const SCHEMA_VERSION: i64 = 3;

#[derive(Clone, Debug)]
pub struct MapAnalyticsConfig {
    pub database_path: PathBuf,
    pub spill_dir: PathBuf,
    pub spatial_extension: PathBuf,
    pub memory_limit: String,
    pub threads: u32,
}

#[derive(Clone, Debug)]
pub struct MapAnalytics {
    database_path: PathBuf,
    settings: EngineSettings,
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
        let analytics = Self {
            database_path: config.database_path,
            settings,
        };
        analytics.initialize()?;
        Ok(analytics)
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn verify_spatial(&self) -> Result<()> {
        let connection = self.connection(false)?;
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
        let connection = self.connection(true)?;
        let mut locations = Vec::new();
        let location_longitude_predicate = longitude_predicate(&request.coverage, "longitude_deg");
        let sql = format!(
            "SELECT canonical_json FROM map_location WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND name ILIKE '%' || ? || '%' AND latitude_deg BETWEEN ? AND ? AND {location_longitude_predicate} ORDER BY name ASC LIMIT ?"
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
                "SELECT canonical_json FROM map_facility WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND name ILIKE '%' || ? || '%' AND latitude_deg BETWEEN ? AND ? AND {facility_longitude_predicate} ORDER BY name ASC LIMIT ?"
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
            &self.connection(true)?,
            "map_location",
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
            &self.connection(true)?,
            "map_facility",
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
        let connection = self.connection(true)?;
        let mut statement = connection.prepare(
            "SELECT canonical_json FROM map_facility \
             WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) \
             AND ST_Distance_Sphere(ST_Point(longitude_deg, latitude_deg), ST_Point(?, ?)) <= ? \
             ORDER BY ST_Distance_Sphere(ST_Point(longitude_deg, latitude_deg), ST_Point(?, ?)) ASC \
             LIMIT ?",
        )?;
        let mut rows = statement.query(params![
            tenant_key,
            tenant_key,
            position.longitude_deg,
            position.latitude_deg,
            radius.get(),
            position.longitude_deg,
            position.latitude_deg,
            limit,
        ])?;
        let mut facilities = Vec::new();
        while let Some(row) = rows.next()? {
            facilities.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(facilities)
    }

    pub fn list_facilities(&self, tenant_key: &str, limit: u32) -> Result<Vec<Facility>> {
        if !(1..=10_000).contains(&limit) {
            bail!("facility list limit must be within 1..=10000");
        }
        let connection = self.connection(true)?;
        let mut statement = connection
            .prepare("SELECT canonical_json FROM map_facility WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) ORDER BY name ASC LIMIT ?")?;
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
        let connection = self.connection(true)?;
        let mut statement = connection
            .prepare("SELECT canonical_json FROM map_location WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) ORDER BY name ASC LIMIT ?")?;
        let mut rows = statement.query(params![tenant_key, tenant_key, limit])?;
        let mut locations = Vec::new();
        while let Some(row) = rows.next()? {
            locations.push(serde_json::from_str(&row.get::<_, String>(0)?)?);
        }
        Ok(locations)
    }

    pub fn put_location(&self, tenant_key: &str, location: &MapLocation) -> Result<()> {
        location.position.validate()?;
        let connection = self.connection(false)?;
        connection.execute(
            "INSERT OR REPLACE INTO map_location VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                tenant_key,
                location.location_id.as_str(),
                location.name,
                location.position.longitude_deg,
                location.position.latitude_deg,
                serde_json::to_string(location)?,
                location.lineage.release_id.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn put_facility(&self, tenant_key: &str, facility: &Facility) -> Result<()> {
        facility.position.validate()?;
        let connection = self.connection(false)?;
        connection.execute(
            "INSERT OR REPLACE INTO map_facility VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                tenant_key,
                facility.facility_id.as_str(),
                facility.name,
                serde_json::to_string(&facility.kind)?,
                facility.position.longitude_deg,
                facility.position.latitude_deg,
                serde_json::to_string(facility)?,
                facility.lineage.release_id.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn put_boundary(&self, tenant_key: &str, boundary: &MapBoundary) -> Result<()> {
        boundary.geometry.validate()?;
        let geometry = polygon_geojson(&boundary.geometry)?;
        let connection = self.connection(false)?;
        connection.execute(
            "INSERT OR REPLACE INTO map_boundary VALUES (?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?)",
            params![
                tenant_key,
                boundary.boundary_id.as_str(),
                boundary.name,
                boundary.boundary_kind,
                geometry,
                serde_json::to_string(boundary)?,
                boundary.lineage.release_id.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn containing_boundary_ids(
        &self,
        tenant_key: &str,
        position: &Wgs84Position,
    ) -> Result<Vec<MapBoundaryId>> {
        position.validate()?;
        let connection = self.connection(true)?;
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
             FROM map_boundary AS boundary \
             WHERE boundary.tenant_key = ? \
               AND boundary.source_release_key IN ({}) \
               AND ST_Contains(boundary.geometry, ST_Point({}, {})) \
             ORDER BY boundary.boundary_key \
             LIMIT 1000",
            sql_string_list(&active_releases),
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
        let connection = self.connection(true)?;
        let active_releases = active_release_keys(&connection, tenant_key)?;
        if active_releases.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT boundary.boundary_key \
             FROM map_boundary AS boundary \
             WHERE boundary.tenant_key = ? \
               AND boundary.source_release_key IN ({}) \
               AND ST_Intersects(boundary.geometry, ST_GeomFromGeoJSON({})) \
             ORDER BY boundary.boundary_key \
             LIMIT 1000",
            sql_string_list(&active_releases),
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

    pub fn remove_release_products(
        &self,
        tenant_key: &str,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<()> {
        let mut connection = self.connection(false)?;
        let transaction = connection.transaction()?;
        for table in [
            "map_location",
            "map_facility",
            "map_boundary",
            "map_network_edge",
        ] {
            transaction.execute(
                &format!("DELETE FROM {table} WHERE tenant_key = ? AND source_release_key = ?"),
                params![tenant_key, release_id.as_str()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn activate_release(
        &self,
        tenant_key: &str,
        dataset_id: &crate::contract::MapDatasetId,
        release_id: &crate::contract::DatasetReleaseId,
    ) -> Result<()> {
        let mut connection = self.connection(false)?;
        let transaction = connection.transaction()?;
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

    pub fn replace_network_edges(
        &self,
        tenant_key: &str,
        release_id: &crate::contract::DatasetReleaseId,
        edges: &[NetworkEdge],
    ) -> Result<()> {
        let mut connection = self.connection(false)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM map_network_edge WHERE tenant_key = ? AND source_release_key = ?",
            params![tenant_key, release_id.as_str()],
        )?;
        {
            let mut statement = transaction
                .prepare("INSERT INTO map_network_edge VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")?;
            for edge in edges {
                edge.geometry.validate()?;
                if edge.source_release_id != *release_id
                    || !edge.distance_m.is_finite()
                    || edge.distance_m <= 0.0
                    || !edge.nominal_duration_s.is_finite()
                    || edge.nominal_duration_s <= 0.0
                {
                    bail!("network edge invariants are invalid");
                }
                statement.execute(params![
                    tenant_key,
                    edge.edge_id,
                    serde_json::to_value(edge.map_family)?
                        .as_str()
                        .context("map family wire value")?,
                    edge.from_node,
                    edge.to_node,
                    serde_json::to_string(&edge.geometry)?,
                    edge.distance_m,
                    edge.nominal_duration_s,
                    edge.bidirectional,
                    edge.source_release_id.as_str(),
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn put_network_edge(&self, tenant_key: &str, edge: &NetworkEdge) -> Result<()> {
        edge.geometry.validate()?;
        if !edge.distance_m.is_finite()
            || edge.distance_m <= 0.0
            || !edge.nominal_duration_s.is_finite()
            || edge.nominal_duration_s <= 0.0
        {
            bail!("network edge invariants are invalid");
        }
        let connection = self.connection(false)?;
        connection.execute(
            "INSERT OR REPLACE INTO map_network_edge VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                tenant_key,
                edge.edge_id,
                serde_json::to_value(edge.map_family)?
                    .as_str()
                    .context("map family wire value")?,
                edge.from_node,
                edge.to_node,
                serde_json::to_string(&edge.geometry)?,
                edge.distance_m,
                edge.nominal_duration_s,
                edge.bidirectional,
                edge.source_release_id.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn network_edges(
        &self,
        tenant_key: &str,
        map_family: MapFamily,
    ) -> Result<Vec<NetworkEdge>> {
        let connection = self.connection(true)?;
        let family = serde_json::to_value(map_family)?
            .as_str()
            .context("map family wire value")?
            .to_owned();
        let mut statement = connection.prepare(
            "SELECT edge_key, map_family, from_node, to_node, geometry_json, distance_m, nominal_duration_s, bidirectional, source_release_key FROM map_network_edge WHERE tenant_key = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) AND map_family = ? ORDER BY edge_key",
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
        let connection = self.connection(false)?;
        connection.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS map_schema (version BIGINT NOT NULL);\n\
             INSERT INTO map_schema SELECT {SCHEMA_VERSION} WHERE NOT EXISTS (SELECT 1 FROM map_schema);\n\
             CREATE TABLE IF NOT EXISTS map_location (\
               tenant_key VARCHAR NOT NULL, location_key VARCHAR NOT NULL, name VARCHAR NOT NULL, longitude_deg DOUBLE NOT NULL, latitude_deg DOUBLE NOT NULL, canonical_json VARCHAR NOT NULL, source_release_key VARCHAR NOT NULL, PRIMARY KEY (tenant_key, location_key)\
             );\n\
             CREATE INDEX IF NOT EXISTS map_location_name ON map_location(tenant_key, name);\n\
             CREATE TABLE IF NOT EXISTS map_facility (\
               tenant_key VARCHAR NOT NULL, facility_key VARCHAR NOT NULL, name VARCHAR NOT NULL, kind VARCHAR NOT NULL, longitude_deg DOUBLE NOT NULL, latitude_deg DOUBLE NOT NULL, canonical_json VARCHAR NOT NULL, source_release_key VARCHAR NOT NULL, PRIMARY KEY (tenant_key, facility_key)\
             );\n\
             CREATE INDEX IF NOT EXISTS map_facility_name ON map_facility(tenant_key, name);\n\
             CREATE TABLE IF NOT EXISTS map_active_release (\
               tenant_key VARCHAR NOT NULL, dataset_key VARCHAR NOT NULL, release_key VARCHAR NOT NULL, PRIMARY KEY (tenant_key, dataset_key), UNIQUE (tenant_key, release_key)\
             );\n\
             CREATE TABLE IF NOT EXISTS map_boundary (\
               tenant_key VARCHAR NOT NULL, boundary_key VARCHAR NOT NULL, name VARCHAR NOT NULL, kind VARCHAR NOT NULL, geometry GEOMETRY NOT NULL, canonical_json VARCHAR NOT NULL, source_release_key VARCHAR NOT NULL, PRIMARY KEY (tenant_key, boundary_key)\
             );\n\
             CREATE INDEX IF NOT EXISTS map_boundary_geometry ON map_boundary USING RTREE (geometry);\n\
             CREATE TABLE IF NOT EXISTS map_network_edge (\
               tenant_key VARCHAR NOT NULL, edge_key VARCHAR NOT NULL, map_family VARCHAR NOT NULL, from_node VARCHAR NOT NULL, to_node VARCHAR NOT NULL, geometry_json VARCHAR NOT NULL, distance_m DOUBLE NOT NULL, nominal_duration_s DOUBLE NOT NULL, bidirectional BOOLEAN NOT NULL, source_release_key VARCHAR NOT NULL, PRIMARY KEY (tenant_key, edge_key)\
             );\n\
             CREATE INDEX IF NOT EXISTS map_network_edge_family ON map_network_edge(tenant_key, map_family);
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
             UPDATE map_schema SET version = {SCHEMA_VERSION} WHERE version = 2;"
        ))?;
        let version: i64 =
            connection.query_row("SELECT max(version) FROM map_schema", [], |row| row.get(0))?;
        if version != SCHEMA_VERSION {
            bail!("unsupported map analytics schema version {version}");
        }
        self.verify_spatial()
    }

    pub(crate) fn connection(&self, read_only: bool) -> Result<Connection> {
        open_connection(
            &self.database_path,
            read_only,
            &[],
            &FileAccess::Denied,
            &self.settings,
        )
    }

    pub(crate) fn task_connection(&self, directory: &Path) -> Result<Connection> {
        open_connection(
            &self.database_path,
            false,
            &[],
            &FileAccess::RequestDirectory(directory.to_path_buf()),
            &self.settings,
        )
    }
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

fn duckdb_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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
        "SELECT canonical_json FROM {table} WHERE tenant_key = ? AND {key_column} = ? AND source_release_key IN (SELECT release_key FROM map_active_release WHERE tenant_key = ?) LIMIT 1"
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
    use super::*;

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
}
