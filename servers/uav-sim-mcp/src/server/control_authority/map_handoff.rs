//! UAV admission policy over Map's public contract.
use super::*;
use veoveo_map_mcp::contract::{MAP_ROUTE_HANDOFF_SCHEMA, RouteStatus};

const PLAN_VALIDATION_MAX_AGE: Duration = Duration::minutes(5);

/// Only executable Map statuses can supply SQL grant-selection parameters.
pub(super) struct RouteRequirement<'a> {
    pub(super) profile: &'a MapMobilityProfileUri,
    pub(super) advisory: bool,
}

impl<'a> RouteRequirement<'a> {
    pub(super) fn new(handoff: &'a MapRouteHandoff) -> Result<Self> {
        if handoff.schema_profile != MAP_ROUTE_HANDOFF_SCHEMA {
            return Err(ControlAuthorityError::Invalid(
                "Map route handoff uses an unsupported schema profile".into(),
            ));
        }
        let advisory = match handoff.route_status {
            RouteStatus::Validated => false,
            RouteStatus::PlanningAdvisory => true,
            RouteStatus::Stale | RouteStatus::Invalidated | RouteStatus::Unavailable => {
                return Err(ControlAuthorityError::Invalid(
                    "UAV admission requires a validated or permitted planning-advisory Map route"
                        .into(),
                ));
            }
        };
        Ok(Self {
            profile: &handoff.mobility_profile_uri,
            advisory,
        })
    }
}

pub(super) fn validate_map_handoff(
    request: &PrepareVehicleMissionRequest,
    grant: &VehicleControlGrant,
) -> Result<()> {
    let handoff = &request.map_route;
    let requirement = RouteRequirement::new(handoff)?;
    if requirement.profile != &grant.map_mobility_profile_uri
        || (requirement.advisory && !grant.allow_planning_advisory)
    {
        return Err(ControlAuthorityError::Forbidden);
    }
    if !single_resource_uri(&handoff.route_uri, "map://route/")
        || !valid_sha256(&handoff.route_digest_sha256)
        || !(2..=10_000).contains(&handoff.path.len())
    {
        return Err(ControlAuthorityError::Invalid(
            "Map route handoff identity, digest, or path bounds are invalid".into(),
        ));
    }
    if handoff
        .path
        .iter()
        .any(|position| position.validate().is_err() || position.ellipsoidal_height_m.is_none())
    {
        return Err(ControlAuthorityError::Invalid(
            "every executable Map route position requires valid ellipsoidal height".into(),
        ));
    }
    let now = Utc::now();
    if handoff.validated_at < now - PLAN_VALIDATION_MAX_AGE
        || handoff.validated_at > now + Duration::seconds(30)
        || handoff.prepared_at < handoff.validated_at
        || handoff.prepared_at > now + Duration::seconds(30)
    {
        return Err(ControlAuthorityError::Invalid(
            "Map route handoff validation is stale or temporally inconsistent".into(),
        ));
    }
    if !request.speed_mps.is_finite()
        || !(0.1..=100.0).contains(&request.speed_mps)
        || !request.hold_seconds_at_destination.is_finite()
        || !(0.0..=3_600.0).contains(&request.hold_seconds_at_destination)
    {
        return Err(ControlAuthorityError::Invalid(
            "mission speed or destination hold is outside UAV bounds".into(),
        ));
    }
    Ok(())
}
