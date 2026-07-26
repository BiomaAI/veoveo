mod derive;
mod projection;
mod validation;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::GatewayInternalIdentity;

use crate::{
    analytics::MapAnalytics,
    catalog::{MapCatalog, MapScope},
    contract::{
        DatasetReleaseState, DeriveSpatialGeometryRequest, MAX_SPATIAL_OUTPUT_COORDINATES,
        SPATIAL_DERIVATION_SCHEMA_VERSION, SpatialDerivation, SpatialDerivationId,
        SpatialFindingSeverity,
    },
};

use projection::LocalProjection;

pub(crate) fn validate_route_lines(
    profile: &crate::contract::MobilityProfile,
    lines: &[crate::contract::Wgs84LineString],
    restrictions: &[crate::contract::Restriction],
) -> Result<Vec<crate::contract::SpatialFinding>> {
    let coordinates = lines
        .iter()
        .flat_map(|line| line.coordinates.iter().cloned())
        .collect::<Vec<_>>();
    if coordinates.len() < 2 {
        bail!("route validation requires at least two positions");
    }
    let operation = crate::contract::SpatialDerivationOperation::ValidateRoute {
        route: crate::contract::Wgs84LineString { coordinates },
    };
    let projection = LocalProjection::for_operation(&operation)?;
    let geometries = lines
        .iter()
        .enumerate()
        .map(|(index, line)| crate::contract::SpatialGeometry {
            role: crate::contract::SpatialGeometryRole::ValidatedRoute,
            ordinal: index as u32,
            geometry: crate::contract::FeatureGeometry::LineString(
                line.coordinates
                    .iter()
                    .map(|position| {
                        crate::contract::GeoJsonPosition::new(
                            position.longitude_deg,
                            position.latitude_deg,
                            position.ellipsoidal_height_m,
                        )
                    })
                    .collect(),
            ),
        })
        .collect::<Vec<_>>();
    Ok(validation::validate(
        profile,
        &std::collections::BTreeSet::new(),
        &geometries,
        restrictions,
        &projection,
    )
    .findings)
}

#[derive(Clone, Debug)]
pub struct SpatialService {
    catalog: MapCatalog,
    analytics: MapAnalytics,
}

impl SpatialService {
    pub fn new(catalog: MapCatalog, analytics: MapAnalytics) -> Self {
        Self { catalog, analytics }
    }

    pub async fn derive(
        &self,
        scope: &MapScope,
        identity: &GatewayInternalIdentity,
        request: DeriveSpatialGeometryRequest,
    ) -> Result<SpatialDerivation> {
        request.validate()?;
        let profile = self
            .catalog
            .mobility_profile(
                scope,
                &request.mobility_profile_id,
                request.mobility_profile_version,
            )
            .await?
            .context("mobility profile version is unavailable")?;
        profile.validate()?;
        if request.effective_at < profile.metadata().valid_from
            || profile
                .metadata()
                .valid_until
                .is_some_and(|until| request.effective_at >= until)
        {
            bail!("mobility profile is not valid at effective_at");
        }
        let releases = self.catalog.list_releases(scope).await?;
        for release_id in &request.source_release_ids {
            let release = releases
                .iter()
                .find(|release| &release.release_id == release_id)
                .context("source release is unavailable")?;
            if release.state == DatasetReleaseState::Quarantined
                || request.effective_at < release.valid_from
                || release
                    .valid_until
                    .is_some_and(|until| request.effective_at >= until)
            {
                bail!("source release is unavailable at effective_at");
            }
        }
        let restrictions = self
            .catalog
            .list_restrictions(scope)
            .await?
            .into_iter()
            .filter(|restriction| {
                restriction.valid_from <= request.effective_at
                    && restriction
                        .valid_until
                        .is_none_or(|until| request.effective_at < until)
            })
            .collect::<Vec<_>>();
        let projection = LocalProjection::for_operation(&request.operation)?;
        let derived = derive::derive(&request.operation, &projection)?;
        let output_coordinate_count = derived
            .geometries
            .iter()
            .map(|geometry| geometry.geometry.coordinate_count())
            .sum::<usize>();
        if output_coordinate_count == 0 || output_coordinate_count > MAX_SPATIAL_OUTPUT_COORDINATES
        {
            bail!("spatial derivation output exceeds the 50000-coordinate bound");
        }
        for geometry in &derived.geometries {
            geometry.geometry.validate()?;
        }
        let validated = validation::validate(
            &profile,
            &request.terrain_classes,
            &derived.geometries,
            &restrictions,
            &projection,
        );
        let request_digest_sha256 = hex::encode(Sha256::digest(serde_json::to_vec(&request)?));
        let geometry_digest_sha256 =
            hex::encode(Sha256::digest(serde_json::to_vec(&derived.geometries)?));
        let derivation_id = SpatialDerivationId::new();
        let derivation = SpatialDerivation {
            schema_version: SPATIAL_DERIVATION_SCHEMA_VERSION,
            resource_uri: crate::uris::spatial_derivation_uri(derivation_id.as_str()),
            derivation_id,
            operation: request.operation,
            geometries: derived.geometries,
            ordered_input_ids: derived.ordered_input_ids,
            connected_components: derived.connected_components,
            valid: validated
                .findings
                .iter()
                .all(|finding| finding.severity != SpatialFindingSeverity::Violation),
            findings: validated.findings,
            mobility_profile_id: request.mobility_profile_id,
            mobility_profile_version: request.mobility_profile_version,
            source_release_ids: request.source_release_ids,
            intersected_restriction_ids: validated.intersected_restriction_ids,
            terrain_classes: request.terrain_classes,
            effective_at: request.effective_at,
            projection: projection.contract(),
            algorithm_revision: request.algorithm_revision,
            request_digest_sha256,
            geometry_digest_sha256,
            created_by: identity.actor.id.clone(),
            work_context: identity.authority.work_context.clone(),
            created_at: Utc::now(),
        };
        derivation.validate()?;
        self.analytics
            .put_spatial_derivation(&scope.tenant_key(), &derivation)?;
        Ok(derivation)
    }
}
