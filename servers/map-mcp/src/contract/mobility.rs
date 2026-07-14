use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    Kilograms, Kilopascals, KilowattHours, Liters, Meters, MetersPerSecond, MobilityProfileId,
    QuantityError, Ratio, Seconds,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MapFamily {
    RoadStreet,
    ActiveMobility,
    RailTransit,
    OffRoadTerrain,
    Maritime,
    Aviation,
    Intermodal,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MobilityFamily {
    Human,
    RoadVehicle,
    OffRoadVehicle,
    RailVehicle,
    SurfaceVessel,
    SubsurfaceVessel,
    FixedWing,
    Rotorcraft,
    Uas,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MobilityProfileMetadata {
    pub profile_id: MobilityProfileId,
    pub name: String,
    pub version: u64,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub labels: BTreeSet<String>,
}

impl MobilityProfileMetadata {
    pub fn validate(&self) -> Result<(), MobilityProfileError> {
        if self.name.is_empty() || self.name.len() > 256 || self.name.chars().any(char::is_control)
        {
            return Err(MobilityProfileError::InvalidName);
        }
        if self.version == 0 {
            return Err(MobilityProfileError::InvalidVersion);
        }
        if self
            .valid_until
            .is_some_and(|until| until <= self.valid_from)
        {
            return Err(MobilityProfileError::InvalidValidity);
        }
        if self.labels.iter().any(|label| {
            label.is_empty() || label.len() > 128 || label.chars().any(char::is_control)
        }) {
            return Err(MobilityProfileError::InvalidLabel);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VehicleDimensions {
    pub length: Meters,
    pub width: Meters,
    pub height: Meters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnergySource {
    Human,
    Battery,
    Gasoline,
    Diesel,
    AviationFuel,
    Hydrogen,
    NaturalGas,
    Nuclear,
    Wind,
    Hybrid,
    ExternalElectric,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnergyProfile {
    pub source: EnergySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_capacity: Option<KilowattHours>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquid_fuel_capacity: Option<Liters>,
    pub minimum_reserve: Ratio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VehiclePerformance {
    pub maximum_speed: MetersPerSecond,
    pub nominal_speed: MetersPerSecond,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_range: Option<Meters>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_capacity: Option<Kilograms>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HumanMovementMode {
    Walk,
    Run,
    Hike,
    ManualMobilityAid,
    PoweredMobilityAid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HumanMobilityProfile {
    pub metadata: MobilityProfileMetadata,
    pub mode: HumanMovementMode,
    pub preferred_speed: MetersPerSecond,
    pub maximum_speed: MetersPerSecond,
    pub carried_load: Kilograms,
    pub maximum_slope: Ratio,
    pub maximum_step: Meters,
    pub maximum_continuous_duration: Seconds,
    #[serde(default)]
    pub stairs_allowed: bool,
    #[serde(default)]
    pub unpaved_allowed: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub accessibility_requirements: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoadVehicleClass {
    Bicycle,
    PoweredTwoWheeler,
    PassengerCar,
    LightCommercial,
    RigidTruck,
    ArticulatedTruck,
    BusCoach,
    EmergencyService,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoadVehicleProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: RoadVehicleClass,
    pub dimensions: VehicleDimensions,
    pub gross_mass: Kilograms,
    pub performance: VehiclePerformance,
    pub energy: EnergyProfile,
    pub axle_count: u16,
    pub maximum_axle_load: Kilograms,
    pub minimum_turning_radius: Meters,
    #[serde(default)]
    pub hazardous_cargo: bool,
    #[serde(default)]
    pub unpaved_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissions_class: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OffRoadLocomotionClass {
    Wheeled,
    Tracked,
    AtvUtv,
    HeavyEquipment,
    UncrewedGroundVehicle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OffRoadVehicleProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: OffRoadLocomotionClass,
    pub dimensions: VehicleDimensions,
    pub gross_mass: Kilograms,
    pub performance: VehiclePerformance,
    pub energy: EnergyProfile,
    pub maximum_slope: Ratio,
    pub maximum_cross_slope: Ratio,
    pub maximum_step: Meters,
    pub maximum_gap: Meters,
    pub maximum_water_depth: Meters,
    pub ground_clearance: Meters,
    pub ground_pressure: Kilopascals,
    pub minimum_turning_radius: Meters,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_surface_classes: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RailVehicleClass {
    LightRailMetro,
    PassengerTrain,
    FreightTrain,
    MaintenanceTrain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RailVehicleProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: RailVehicleClass,
    pub dimensions: VehicleDimensions,
    pub gross_mass: Kilograms,
    pub performance: VehiclePerformance,
    pub energy: EnergyProfile,
    pub gauge: Meters,
    pub train_length: Meters,
    pub maximum_axle_load: Kilograms,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub electrification_systems: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub operator_permissions: BTreeSet<String>,
    #[serde(default)]
    pub schedule_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceVesselClass {
    SmallCraft,
    Cargo,
    Tanker,
    PassengerFerry,
    TugWorkboat,
    FishingService,
    UncrewedSurfaceVessel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SurfaceVesselProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: SurfaceVesselClass,
    pub dimensions: VehicleDimensions,
    pub displacement: Kilograms,
    pub performance: VehiclePerformance,
    pub energy: EnergyProfile,
    pub draft: Meters,
    pub air_draft: Meters,
    pub minimum_under_keel_clearance: Meters,
    pub minimum_turning_radius: Meters,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub berth_requirements: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubsurfaceVesselClass {
    Submarine,
    AutonomousUnderwaterVehicle,
    RemotelyOperatedVehicle,
    UnderwaterGlider,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SubsurfaceVesselProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: SubsurfaceVesselClass,
    pub dimensions: VehicleDimensions,
    pub displacement: Kilograms,
    pub performance: VehiclePerformance,
    pub energy: EnergyProfile,
    pub minimum_operating_depth: Meters,
    pub maximum_operating_depth: Meters,
    pub minimum_bathymetric_clearance: Meters,
    pub maximum_submerged_duration: Seconds,
    #[serde(default)]
    pub periodic_surfacing_required: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AircraftPerformance {
    pub maximum_speed: MetersPerSecond,
    pub cruise_speed: MetersPerSecond,
    pub service_ceiling: Meters,
    pub maximum_range: Meters,
    pub payload_capacity: Kilograms,
    pub minimum_reserve: Ratio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FixedWingClass {
    Light,
    RegionalTransport,
    HeavyCargo,
    Amphibious,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FixedWingProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: FixedWingClass,
    pub dimensions: VehicleDimensions,
    pub maximum_takeoff_mass: Kilograms,
    pub performance: AircraftPerformance,
    pub energy: EnergyProfile,
    pub minimum_runway_length: Meters,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub navigation_capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub airspace_permissions: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RotorcraftClass {
    Helicopter,
    HeavyLiftHelicopter,
    Tiltrotor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RotorcraftProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: RotorcraftClass,
    pub dimensions: VehicleDimensions,
    pub maximum_takeoff_mass: Kilograms,
    pub performance: AircraftPerformance,
    pub energy: EnergyProfile,
    pub rotor_diameter: Meters,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub navigation_capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub airspace_permissions: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UasClass {
    Multirotor,
    FixedWing,
    HybridVtol,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UasProfile {
    pub metadata: MobilityProfileMetadata,
    pub class: UasClass,
    pub dimensions: VehicleDimensions,
    pub maximum_takeoff_mass: Kilograms,
    pub performance: AircraftPerformance,
    pub energy: EnergyProfile,
    #[serde(default)]
    pub beyond_visual_line_of_sight: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub navigation_capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub airspace_permissions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "family", content = "profile", rename_all = "snake_case")]
pub enum MobilityProfile {
    Human(HumanMobilityProfile),
    RoadVehicle(RoadVehicleProfile),
    OffRoadVehicle(OffRoadVehicleProfile),
    RailVehicle(RailVehicleProfile),
    SurfaceVessel(SurfaceVesselProfile),
    SubsurfaceVessel(SubsurfaceVesselProfile),
    FixedWing(FixedWingProfile),
    Rotorcraft(RotorcraftProfile),
    Uas(UasProfile),
}

impl MobilityProfile {
    pub fn family(&self) -> MobilityFamily {
        match self {
            Self::Human(_) => MobilityFamily::Human,
            Self::RoadVehicle(_) => MobilityFamily::RoadVehicle,
            Self::OffRoadVehicle(_) => MobilityFamily::OffRoadVehicle,
            Self::RailVehicle(_) => MobilityFamily::RailVehicle,
            Self::SurfaceVessel(_) => MobilityFamily::SurfaceVessel,
            Self::SubsurfaceVessel(_) => MobilityFamily::SubsurfaceVessel,
            Self::FixedWing(_) => MobilityFamily::FixedWing,
            Self::Rotorcraft(_) => MobilityFamily::Rotorcraft,
            Self::Uas(_) => MobilityFamily::Uas,
        }
    }

    pub fn metadata(&self) -> &MobilityProfileMetadata {
        match self {
            Self::Human(profile) => &profile.metadata,
            Self::RoadVehicle(profile) => &profile.metadata,
            Self::OffRoadVehicle(profile) => &profile.metadata,
            Self::RailVehicle(profile) => &profile.metadata,
            Self::SurfaceVessel(profile) => &profile.metadata,
            Self::SubsurfaceVessel(profile) => &profile.metadata,
            Self::FixedWing(profile) => &profile.metadata,
            Self::Rotorcraft(profile) => &profile.metadata,
            Self::Uas(profile) => &profile.metadata,
        }
    }

    pub fn compatible_map_families(&self) -> BTreeSet<MapFamily> {
        use MapFamily::*;
        match self {
            Self::Human(_) => BTreeSet::from([ActiveMobility, Intermodal]),
            Self::RoadVehicle(_) => BTreeSet::from([RoadStreet, Intermodal]),
            Self::OffRoadVehicle(_) => BTreeSet::from([OffRoadTerrain, RoadStreet, Intermodal]),
            Self::RailVehicle(_) => BTreeSet::from([RailTransit, Intermodal]),
            Self::SurfaceVessel(_) | Self::SubsurfaceVessel(_) => {
                BTreeSet::from([Maritime, Intermodal])
            }
            Self::FixedWing(_) | Self::Rotorcraft(_) | Self::Uas(_) => {
                BTreeSet::from([Aviation, Intermodal])
            }
        }
    }

    pub fn validate(&self) -> Result<(), MobilityProfileError> {
        self.metadata().validate()?;
        match self {
            Self::Human(profile) => {
                if profile.preferred_speed > profile.maximum_speed {
                    return Err(MobilityProfileError::InvalidPerformance);
                }
                validate_text_set(
                    "accessibility_requirements",
                    &profile.accessibility_requirements,
                )
            }
            Self::RoadVehicle(profile) => {
                validate_vehicle_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                if profile.axle_count == 0 {
                    return Err(MobilityProfileError::InvalidAxleCount);
                }
                validate_optional_text("emissions_class", profile.emissions_class.as_deref())
            }
            Self::OffRoadVehicle(profile) => {
                validate_vehicle_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                if profile.ground_pressure.get() == 0.0 {
                    return Err(MobilityProfileError::InvalidGroundPressure);
                }
                validate_text_set("allowed_surface_classes", &profile.allowed_surface_classes)
            }
            Self::RailVehicle(profile) => {
                validate_vehicle_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                validate_text_set("electrification_systems", &profile.electrification_systems)?;
                validate_text_set("operator_permissions", &profile.operator_permissions)
            }
            Self::SurfaceVessel(profile) => {
                validate_vehicle_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                validate_text_set("berth_requirements", &profile.berth_requirements)
            }
            Self::SubsurfaceVessel(profile) => {
                validate_vehicle_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                if profile.minimum_operating_depth > profile.maximum_operating_depth {
                    return Err(MobilityProfileError::InvalidDepthRange);
                }
                Ok(())
            }
            Self::FixedWing(profile) => {
                validate_aircraft_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                validate_aircraft_sets(
                    &profile.navigation_capabilities,
                    &profile.airspace_permissions,
                )
            }
            Self::Rotorcraft(profile) => {
                validate_aircraft_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                validate_aircraft_sets(
                    &profile.navigation_capabilities,
                    &profile.airspace_permissions,
                )
            }
            Self::Uas(profile) => {
                validate_aircraft_performance(&profile.performance)?;
                validate_energy(&profile.energy)?;
                validate_aircraft_sets(
                    &profile.navigation_capabilities,
                    &profile.airspace_permissions,
                )
            }
        }
    }
}

fn validate_vehicle_performance(
    performance: &VehiclePerformance,
) -> Result<(), MobilityProfileError> {
    if performance.nominal_speed > performance.maximum_speed {
        return Err(MobilityProfileError::InvalidPerformance);
    }
    Ok(())
}

fn validate_aircraft_performance(
    performance: &AircraftPerformance,
) -> Result<(), MobilityProfileError> {
    if performance.cruise_speed > performance.maximum_speed {
        return Err(MobilityProfileError::InvalidPerformance);
    }
    Ok(())
}

fn validate_energy(energy: &EnergyProfile) -> Result<(), MobilityProfileError> {
    if energy.source == EnergySource::Battery && energy.battery_capacity.is_none() {
        return Err(MobilityProfileError::MissingEnergyCapacity);
    }
    if matches!(
        energy.source,
        EnergySource::Gasoline
            | EnergySource::Diesel
            | EnergySource::AviationFuel
            | EnergySource::NaturalGas
    ) && energy.liquid_fuel_capacity.is_none()
    {
        return Err(MobilityProfileError::MissingEnergyCapacity);
    }
    Ok(())
}

fn validate_aircraft_sets(
    navigation_capabilities: &BTreeSet<String>,
    airspace_permissions: &BTreeSet<String>,
) -> Result<(), MobilityProfileError> {
    validate_text_set("navigation_capabilities", navigation_capabilities)?;
    validate_text_set("airspace_permissions", airspace_permissions)
}

fn validate_text_set(
    _name: &'static str,
    values: &BTreeSet<String>,
) -> Result<(), MobilityProfileError> {
    if values
        .iter()
        .any(|value| value.is_empty() || value.len() > 128 || value.chars().any(char::is_control))
    {
        return Err(MobilityProfileError::InvalidControlledValue);
    }
    Ok(())
}

fn validate_optional_text(
    _name: &'static str,
    value: Option<&str>,
) -> Result<(), MobilityProfileError> {
    if value.is_some_and(|value| {
        value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
    }) {
        return Err(MobilityProfileError::InvalidControlledValue);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MobilityProfileError {
    #[error("mobility profile name must be non-empty, bounded, and contain no control characters")]
    InvalidName,
    #[error("mobility profile version must be greater than zero")]
    InvalidVersion,
    #[error("mobility profile valid_until must be later than valid_from")]
    InvalidValidity,
    #[error("mobility profile label is invalid")]
    InvalidLabel,
    #[error("nominal or preferred speed must not exceed maximum speed")]
    InvalidPerformance,
    #[error("road vehicle axle count must be greater than zero")]
    InvalidAxleCount,
    #[error("off-road ground pressure must be finite and non-negative")]
    InvalidGroundPressure,
    #[error("minimum operating depth must not exceed maximum operating depth")]
    InvalidDepthRange,
    #[error("energy capacity is required for the selected energy source")]
    MissingEnergyCapacity,
    #[error("controlled mobility profile value is invalid")]
    InvalidControlledValue,
    #[error(transparent)]
    Quantity(#[from] QuantityError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> MobilityProfileMetadata {
        MobilityProfileMetadata {
            profile_id: MobilityProfileId::new(),
            name: "pedestrian".to_owned(),
            version: 1,
            valid_from: Utc::now(),
            valid_until: None,
            labels: BTreeSet::new(),
        }
    }

    #[test]
    fn taxonomy_has_one_human_and_eight_vehicle_families() {
        let families = [
            MobilityFamily::Human,
            MobilityFamily::RoadVehicle,
            MobilityFamily::OffRoadVehicle,
            MobilityFamily::RailVehicle,
            MobilityFamily::SurfaceVessel,
            MobilityFamily::SubsurfaceVessel,
            MobilityFamily::FixedWing,
            MobilityFamily::Rotorcraft,
            MobilityFamily::Uas,
        ];
        assert_eq!(families.len(), 9);
    }

    #[test]
    fn human_profile_selects_active_and_intermodal_maps() {
        let profile = MobilityProfile::Human(HumanMobilityProfile {
            metadata: metadata(),
            mode: HumanMovementMode::Walk,
            preferred_speed: MetersPerSecond::new(1.4).unwrap(),
            maximum_speed: MetersPerSecond::new(2.0).unwrap(),
            carried_load: Kilograms::new(5.0).unwrap(),
            maximum_slope: Ratio::new(0.1).unwrap(),
            maximum_step: Meters::new(0.2).unwrap(),
            maximum_continuous_duration: Seconds::new(3600.0).unwrap(),
            stairs_allowed: true,
            unpaved_allowed: true,
            accessibility_requirements: BTreeSet::new(),
        });
        profile.validate().unwrap();
        assert_eq!(profile.family(), MobilityFamily::Human);
        assert_eq!(
            profile.compatible_map_families(),
            BTreeSet::from([MapFamily::ActiveMobility, MapFamily::Intermodal])
        );
    }
}
