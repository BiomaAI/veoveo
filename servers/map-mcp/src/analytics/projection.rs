use std::cell::RefCell;

use anyhow::{Context, Result, bail};
use duckdb::{Connection, params};

use super::{MapAnalytics, NetworkEdge, enum_wire, polygon_geojson};
use crate::contract::{
    DatasetReleaseId, Facility, MapBoundary, MapLocation, RasterProduct, SourceFeature,
};

const BATCH_FEATURES: usize = 256;
const BATCH_BYTES: usize = 32 * 1024 * 1024;
const MAX_ATTEMPTS: u64 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionAttemptId(String);

impl ProjectionAttemptId {
    fn new() -> Self {
        Self(uuid::Uuid::now_v7().to_string())
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProjectionCounts {
    locations: u64,
    facilities: u64,
    boundaries: u64,
    network_edges: u64,
    source_features: u64,
    raster_products: u64,
}

struct ProjectionBatch {
    connection: Option<Connection>,
    feature_count: usize,
    serialized_bytes: usize,
    counts: ProjectionCounts,
}

pub(crate) struct ReleaseProjectionWriter {
    analytics: MapAnalytics,
    tenant_key: String,
    release_id: DatasetReleaseId,
    attempt_id: ProjectionAttemptId,
    state: RefCell<ProjectionBatch>,
}

impl ReleaseProjectionWriter {
    pub(crate) fn new(
        analytics: MapAnalytics,
        tenant_key: &str,
        release_id: &DatasetReleaseId,
    ) -> Result<Self> {
        let attempt_id = ProjectionAttemptId::new();
        let connection = analytics.connection()?;
        connection.execute_batch("BEGIN TRANSACTION")?;
        let setup = (|| {
            let complete: bool = connection.query_row(
                "SELECT count(*) = 1 FROM map_release_projection WHERE tenant_key = ? AND release_key = ?",
                params![tenant_key, release_id.as_str()],
                |row| row.get(0),
            )?;
            if complete {
                bail!("refusing to replace a complete Map release projection");
            }
            let attempts: u64 = connection.query_row(
                "SELECT count(*) FROM map_release_projection_attempt WHERE tenant_key = ? AND release_key = ?",
                params![tenant_key, release_id.as_str()],
                |row| row.get(0),
            )?;
            if attempts >= MAX_ATTEMPTS {
                bail!(
                    "Map release projection exhausted its {MAX_ATTEMPTS} retained attempts; rebuild the derived Map projection before retrying"
                );
            }
            connection.execute(
                "INSERT INTO map_release_projection_attempt VALUES (?, ?, ?, now())",
                params![tenant_key, release_id.as_str(), attempt_id.as_str()],
            )?;
            Ok(())
        })();
        if let Err(error) = setup {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
        Ok(Self {
            analytics,
            tenant_key: tenant_key.to_owned(),
            release_id: release_id.clone(),
            attempt_id,
            state: RefCell::new(ProjectionBatch {
                connection: Some(connection),
                feature_count: 0,
                serialized_bytes: 0,
                counts: ProjectionCounts::default(),
            }),
        })
    }

    fn ensure_scope(&self, tenant_key: &str, release_id: &str) -> Result<()> {
        if tenant_key != self.tenant_key || release_id != self.release_id.as_str() {
            bail!("release projection write escaped its tenant or release scope");
        }
        Ok(())
    }

    fn with_connection<T>(&self, operation: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let state = self.state.borrow();
        operation(
            state
                .connection
                .as_ref()
                .context("release projection batch is not active")?,
        )
    }

    fn rotate_before(&self, next_bytes: usize) -> Result<()> {
        let rotate = {
            let state = self.state.borrow();
            state.feature_count >= BATCH_FEATURES
                || (state.feature_count > 0
                    && state.serialized_bytes.saturating_add(next_bytes) > BATCH_BYTES)
        };
        if !rotate {
            return Ok(());
        }
        {
            let mut state = self.state.borrow_mut();
            state
                .connection
                .take()
                .context("release projection batch is not active")?
                .execute_batch("COMMIT")?;
            state.feature_count = 0;
            state.serialized_bytes = 0;
        }
        let connection = self.analytics.connection()?;
        connection.execute_batch("BEGIN TRANSACTION")?;
        self.state.borrow_mut().connection = Some(connection);
        Ok(())
    }

    pub(crate) fn put_source_feature(
        &self,
        tenant_key: &str,
        feature: &SourceFeature,
    ) -> Result<()> {
        self.ensure_scope(tenant_key, feature.release_id.as_str())?;
        feature.validate()?;
        let serialized_bytes = serde_json::to_vec(feature)?.len();
        self.rotate_before(serialized_bytes)?;
        let ordinal = self.state.borrow().counts.source_features;
        self.with_connection(|connection| {
            let geometry = feature.geometry.to_geojson_string()?;
            connection.execute(
                "INSERT INTO map_source_feature VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?::JSON, ?::JSON, ?, ?, ?)",
                params![
                    tenant_key,
                    feature.release_id.as_str(),
                    feature.feature_id.as_str(),
                    feature.source_id.as_str(),
                    enum_wire(feature.source_element_type)?,
                    feature.source_element_id,
                    feature.source_element_version,
                    enum_wire(feature.representation)?,
                    feature.geometry_digest_sha256,
                    geometry,
                    super::source_feature_normalized_text(feature)?,
                    serde_json::to_string(&feature.normalized_tags)?,
                    serde_json::to_string(feature)?,
                    feature.source_digest_sha256,
                    self.attempt_id.as_str(),
                    ordinal,
                ],
            )?;
            Ok(())
        })?;
        let mut state = self.state.borrow_mut();
        state.counts.source_features = checked_next(ordinal, "source feature")?;
        state.feature_count = state
            .feature_count
            .checked_add(1)
            .context("Map release projection batch feature count overflow")?;
        state.serialized_bytes = state.serialized_bytes.saturating_add(serialized_bytes);
        Ok(())
    }

    pub(crate) fn put_location(&self, tenant_key: &str, location: &MapLocation) -> Result<()> {
        self.ensure_scope(tenant_key, location.lineage.release_id.as_str())?;
        location.position.validate()?;
        let ordinal = self.state.borrow().counts.locations;
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO map_location VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    tenant_key,
                    location.location_id.as_str(),
                    location.name,
                    location.position.longitude_deg,
                    location.position.latitude_deg,
                    serde_json::to_string(location)?,
                    location.lineage.release_id.as_str(),
                    self.attempt_id.as_str(),
                    ordinal,
                ],
            )?;
            Ok(())
        })?;
        self.state.borrow_mut().counts.locations = checked_next(ordinal, "location")?;
        Ok(())
    }

    pub(crate) fn put_facility(&self, tenant_key: &str, facility: &Facility) -> Result<()> {
        self.ensure_scope(tenant_key, facility.lineage.release_id.as_str())?;
        facility.position.validate()?;
        let ordinal = self.state.borrow().counts.facilities;
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO map_facility VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    tenant_key,
                    facility.facility_id.as_str(),
                    facility.name,
                    serde_json::to_string(&facility.kind)?,
                    facility.position.longitude_deg,
                    facility.position.latitude_deg,
                    serde_json::to_string(facility)?,
                    facility.lineage.release_id.as_str(),
                    self.attempt_id.as_str(),
                    ordinal,
                ],
            )?;
            Ok(())
        })?;
        self.state.borrow_mut().counts.facilities = checked_next(ordinal, "facility")?;
        Ok(())
    }

    pub(crate) fn put_boundary(&self, tenant_key: &str, boundary: &MapBoundary) -> Result<()> {
        self.ensure_scope(tenant_key, boundary.lineage.release_id.as_str())?;
        boundary.geometry.validate()?;
        let ordinal = self.state.borrow().counts.boundaries;
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO map_boundary VALUES (?, ?, ?, ?, ST_GeomFromGeoJSON(?), ?, ?, ?, ?)",
                params![
                    tenant_key,
                    boundary.boundary_id.as_str(),
                    boundary.name,
                    boundary.boundary_kind,
                    polygon_geojson(&boundary.geometry)?,
                    serde_json::to_string(boundary)?,
                    boundary.lineage.release_id.as_str(),
                    self.attempt_id.as_str(),
                    ordinal,
                ],
            )?;
            Ok(())
        })?;
        self.state.borrow_mut().counts.boundaries = checked_next(ordinal, "boundary")?;
        Ok(())
    }

    pub(crate) fn put_network_edge(&self, tenant_key: &str, edge: &NetworkEdge) -> Result<()> {
        self.ensure_scope(tenant_key, edge.source_release_id.as_str())?;
        edge.geometry.validate()?;
        if !edge.distance_m.is_finite()
            || edge.distance_m <= 0.0
            || !edge.nominal_duration_s.is_finite()
            || edge.nominal_duration_s <= 0.0
        {
            bail!("network edge invariants are invalid");
        }
        let ordinal = self.state.borrow().counts.network_edges;
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO map_network_edge VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    tenant_key,
                    edge.edge_id,
                    enum_wire(edge.map_family)?,
                    edge.from_node,
                    edge.to_node,
                    serde_json::to_string(&edge.geometry)?,
                    edge.distance_m,
                    edge.nominal_duration_s,
                    edge.bidirectional,
                    edge.source_release_id.as_str(),
                    self.attempt_id.as_str(),
                    ordinal,
                ],
            )?;
            Ok(())
        })?;
        self.state.borrow_mut().counts.network_edges = checked_next(ordinal, "network edge")?;
        Ok(())
    }

    pub(crate) fn put_raster_product(
        &self,
        tenant_key: &str,
        raster: &RasterProduct,
    ) -> Result<()> {
        self.ensure_scope(tenant_key, raster.release_id.as_str())?;
        raster.validate()?;
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO map_raster_product VALUES (?, ?, ?, ?, ?, ?::JSON, ?)",
                params![
                    tenant_key,
                    raster.raster_id.as_str(),
                    raster.release_id.as_str(),
                    raster.source_id.as_str(),
                    raster.checksum_sha256,
                    serde_json::to_string(raster)?,
                    self.attempt_id.as_str(),
                ],
            )?;
            Ok(())
        })?;
        let count = self.state.borrow().counts.raster_products;
        self.state.borrow_mut().counts.raster_products = checked_next(count, "raster product")?;
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<()> {
        let mut state = self.state.borrow_mut();
        let connection = state
            .connection
            .take()
            .context("release projection batch is not active")?;
        let result = (|| {
            validate_projection(
                &connection,
                &self.tenant_key,
                &self.release_id,
                &self.attempt_id,
                state.counts,
            )?;
            connection.execute(
                "INSERT INTO map_release_projection VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, now())",
                params![
                    self.tenant_key,
                    self.release_id.as_str(),
                    self.attempt_id.as_str(),
                    state.counts.locations,
                    state.counts.facilities,
                    state.counts.boundaries,
                    state.counts.network_edges,
                    state.counts.source_features,
                    state.counts.raster_products,
                ],
            )?;
            connection.execute_batch("COMMIT")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = connection.execute_batch("ROLLBACK");
        }
        result
    }

    pub(crate) fn abort(&self) {
        if let Some(connection) = self.state.borrow_mut().connection.take() {
            let _ = connection.execute_batch("ROLLBACK");
        }
    }
}

impl Drop for ReleaseProjectionWriter {
    fn drop(&mut self) {
        if let Some(connection) = self.state.get_mut().connection.take() {
            let _ = connection.execute_batch("ROLLBACK");
        }
    }
}

fn checked_next(value: u64, name: &str) -> Result<u64> {
    value
        .checked_add(1)
        .with_context(|| format!("Map {name} projection ordinal overflow"))
}

fn validate_projection(
    connection: &Connection,
    tenant_key: &str,
    release_id: &DatasetReleaseId,
    attempt_id: &ProjectionAttemptId,
    counts: ProjectionCounts,
) -> Result<()> {
    let attempt_rows: u64 = connection.query_row(
        "SELECT count(*) FROM map_release_projection_attempt WHERE tenant_key = ? AND release_key = ? AND projection_attempt_key = ?",
        params![tenant_key, release_id.as_str(), attempt_id.as_str()],
        |row| row.get(0),
    )?;
    if attempt_rows != 1 {
        bail!("Map release projection attempt ledger is inconsistent");
    }
    for (table, release_column, key_column, expected) in [
        (
            "map_location",
            "source_release_key",
            "location_key",
            counts.locations,
        ),
        (
            "map_facility",
            "source_release_key",
            "facility_key",
            counts.facilities,
        ),
        (
            "map_boundary",
            "source_release_key",
            "boundary_key",
            counts.boundaries,
        ),
        (
            "map_network_edge",
            "source_release_key",
            "edge_key",
            counts.network_edges,
        ),
        (
            "map_source_feature",
            "release_key",
            "feature_key",
            counts.source_features,
        ),
    ] {
        validate_heap_table(
            connection,
            table,
            release_column,
            key_column,
            tenant_key,
            release_id,
            attempt_id,
            expected,
        )?;
    }
    let raster_count: u64 = connection.query_row(
        "SELECT count(*) FROM map_raster_product WHERE tenant_key = ? AND release_key = ? AND projection_attempt_key = ?",
        params![tenant_key, release_id.as_str(), attempt_id.as_str()],
        |row| row.get(0),
    )?;
    if raster_count != counts.raster_products {
        bail!("Map raster-product projection count is inconsistent");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_heap_table(
    connection: &Connection,
    table: &str,
    release_column: &str,
    key_column: &str,
    tenant_key: &str,
    release_id: &DatasetReleaseId,
    attempt_id: &ProjectionAttemptId,
    expected: u64,
) -> Result<()> {
    let sql = format!(
        "SELECT count(*), count(DISTINCT projection_ordinal), min(projection_ordinal), max(projection_ordinal) FROM {table} WHERE tenant_key = ? AND {release_column} = ? AND projection_attempt_key = ?"
    );
    let (count, distinct, minimum, maximum): (u64, u64, Option<u64>, Option<u64>) = connection
        .query_row(
            &sql,
            params![tenant_key, release_id.as_str(), attempt_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let expected_maximum = expected.checked_sub(1);
    if count != expected
        || distinct != expected
        || minimum != (expected > 0).then_some(0)
        || maximum != expected_maximum
    {
        bail!("Map {table} projection ordinals are inconsistent");
    }
    let duplicate_sql = format!(
        "SELECT count(*) FROM (SELECT {key_column} FROM {table} WHERE tenant_key = ? AND {release_column} = ? AND projection_attempt_key = ? GROUP BY {key_column} HAVING count(*) > 1 LIMIT 1)"
    );
    let duplicate_domains: u64 = connection.query_row(
        &duplicate_sql,
        params![tenant_key, release_id.as_str(), attempt_id.as_str()],
        |row| row.get(0),
    )?;
    if duplicate_domains != 0 {
        bail!("Map {table} projection contains duplicate stored identities");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_rotation_is_bounded() {
        let rotates = |features: usize, bytes: usize, next: usize| {
            features >= BATCH_FEATURES || (features > 0 && bytes.saturating_add(next) > BATCH_BYTES)
        };
        assert!(!rotates(0, 0, BATCH_BYTES + 1));
        assert!(rotates(1, BATCH_BYTES, 1));
        assert!(rotates(BATCH_FEATURES, 0, 1));
    }
}
